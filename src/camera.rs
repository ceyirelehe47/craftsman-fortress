//! 自由透视相机：焦点 + 轨道模型（任务书 4.5）。
//!
//! - Perspective 投影，围绕可移动"焦点"旋转 / 俯仰 / 缩放；
//! - 俯仰、距离、焦点范围全部受限且稳定；
//! - 相机不得停留于实体体素内：从焦点向相机方向做 DDA，命中则压缩轨道距离；
//!   一阶指数平滑收敛，无振荡（A08）；
//! - 验收模式由脚本注入位姿（`control_locked` 锁定用户输入）。

use crate::picking::{pick_voxel, Ray};
use crate::world::World;
use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;

use crate::app_state::GameState;

/// 相机求解系统的调度集合（0.19 起 `before`/`after` 只接受 SystemSet）。
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CameraSolve;

/// 俯仰角范围（弧度，相对水平面仰角）。
pub const PITCH_MIN: f32 = 0.21; // ~12°
pub const PITCH_MAX: f32 = 1.52; // ~87°
/// 轨道距离范围（米）。
pub const DIST_MIN: f32 = 6.0;
pub const DIST_MAX: f32 = 150.0;
/// 相机与实体表面的最小留白（米）。
pub const COLLISION_MARGIN: f32 = 0.5;
/// 平滑系数（1/s）：越大收敛越快。一阶滤波不会振荡。
const DIST_SMOOTHING: f32 = 14.0;

/// 相机轨道状态（权威，`Transform` 是它的投影）。
#[derive(Clone, Debug)]
pub struct CameraRig {
    pub focus: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    /// 用户期望距离（受碰撞钳制前的目标）。
    pub target_dist: f32,
    /// 实际距离（平滑后）。
    pub dist: f32,
    /// 世界边界（焦点活动范围）。
    pub bounds: (f32, f32, f32, f32), // min_x, max_x, min_z, max_z
    pub max_focus_y: f32,
    /// 用户输入锁定（脚本相机接管时为 true）。
    pub control_locked: bool,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self::new(
            (256.0, 128.0, 256.0),
            Vec3::new(128.0, 60.0, 128.0),
            0.8,
            0.9,
            60.0,
        )
    }
}

impl CameraRig {
    pub fn new(world_size: (f32, f32, f32), focus: Vec3, yaw: f32, pitch: f32, dist: f32) -> Self {
        Self {
            focus,
            yaw,
            pitch: pitch.clamp(PITCH_MIN, PITCH_MAX),
            target_dist: dist.clamp(DIST_MIN, DIST_MAX),
            dist: dist.clamp(DIST_MIN, DIST_MAX),
            bounds: (8.0, world_size.0 - 8.0, 8.0, world_size.2 - 8.0),
            max_focus_y: world_size.1 - 2.0,
            control_locked: false,
        }
    }

    /// 钳制焦点/俯仰/距离到稳定范围。
    /// 脚本接管（control_locked）时允许小于常规最小轨道距离（地下洞穴机位）。
    pub fn clamp_all(&mut self) {
        self.focus.x = self.focus.x.clamp(self.bounds.0, self.bounds.1);
        self.focus.z = self.focus.z.clamp(self.bounds.2, self.bounds.3);
        self.focus.y = self.focus.y.clamp(2.0, self.max_focus_y);
        self.pitch = self.pitch.clamp(PITCH_MIN, PITCH_MAX);
        let lo = if self.control_locked { 1.5 } else { DIST_MIN };
        self.target_dist = self.target_dist.clamp(lo, DIST_MAX);
    }

    /// 理想相机位置（未考虑碰撞）。
    pub fn desired_eye(&self) -> Vec3 {
        let horiz = self.dist * self.pitch.cos();
        Vec3::new(
            self.focus.x + horiz * self.yaw.sin(),
            self.focus.y + self.dist * self.pitch.sin(),
            self.focus.z + horiz * self.yaw.cos(),
        )
    }

    /// 按当前实际距离求相机位置（供系统写入 Transform）。
    pub fn eye(&self) -> Vec3 {
        let mut rig = self.clone();
        rig.dist = self.dist;
        rig.desired_eye()
    }

    /// 碰撞钳制：从焦点向理想相机方向步进，命中实体则收缩距离。
    /// 返回钳制后的实际距离。
    pub fn collision_clamped_dist(&self, world: &World) -> f32 {
        let probe = Self {
            dist: self.target_dist,
            ..self.clone()
        };
        let eye = probe.desired_eye();
        let dir = (eye - self.focus).normalize_or_zero();
        if dir.length_squared() < f32::EPSILON {
            return self.target_dist;
        }
        let ray = Ray {
            origin: self.focus,
            dir,
        };
        let limit = match pick_voxel(world, &ray, self.target_dist + COLLISION_MARGIN) {
            Some(hit) => (hit.t - COLLISION_MARGIN).max(1.5),
            None => self.target_dist,
        };
        limit.min(self.target_dist).max(1.5)
    }
}

/// 相机控制：读取键鼠输入更新 rig（仅 Ready 且未锁定）。
pub fn camera_input_system(
    mut rig: ResMut<CameraRigRes>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time>,
) {
    let rig = &mut rig.0;
    if rig.control_locked {
        return;
    }
    let dt = time.delta_secs().min(0.1);
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let speed = (rig.target_dist * 0.9).max(6.0) * if shift { 3.0 } else { 1.0 };

    // 平移（相机朝向相对，水平面内）。
    let fwd = Vec3::new(rig.yaw.sin(), 0.0, rig.yaw.cos());
    let right = Vec3::new(fwd.z, 0.0, -fwd.x);
    let mut move_delta = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        move_delta += fwd;
    }
    if keys.pressed(KeyCode::KeyS) {
        move_delta -= fwd;
    }
    if keys.pressed(KeyCode::KeyD) {
        move_delta += right;
    }
    if keys.pressed(KeyCode::KeyA) {
        move_delta -= right;
    }
    if keys.pressed(KeyCode::KeyR) {
        rig.focus.y += speed * dt;
    }
    if keys.pressed(KeyCode::KeyF) {
        rig.focus.y -= speed * dt;
    }
    if move_delta.length_squared() > 0.0 {
        rig.focus += move_delta.normalize() * speed * dt;
    }

    // 旋转：Q/E 或鼠标左拖。
    if keys.pressed(KeyCode::KeyQ) {
        rig.yaw -= 1.6 * dt;
    }
    if keys.pressed(KeyCode::KeyE) {
        rig.yaw += 1.6 * dt;
    }
    if mouse.pressed(MouseButton::Left) {
        rig.yaw += motion.delta.x * 0.005;
        rig.pitch += motion.delta.y * 0.005;
    }
    // 平移：鼠标右拖。
    if mouse.pressed(MouseButton::Right) {
        let pan = rig.target_dist * 0.0016;
        rig.focus -= right * motion.delta.x * pan;
        // 前向拖动沿视线上投影的水平分量
        rig.focus -= fwd * motion.delta.y * pan;
    }

    // 缩放：滚轮。
    if scroll.delta.y.abs() > 1e-6 {
        rig.target_dist *= (-scroll.delta.y * 0.0012).exp();
    }

    rig.clamp_all();
}

/// 相机求解：碰撞钳制 + 平滑 + 写入 Transform。
pub fn camera_solve_system(
    mut rig: ResMut<CameraRigRes>,
    world: Res<WorldRes>,
    mut query: Query<&mut Transform, With<crate::render::MainCamera>>,
    time: Res<Time>,
) {
    let Ok(mut transform) = query.single_mut() else {
        return;
    };
    let Some(world) = world.0.as_ref() else {
        return;
    };
    let rig = &mut rig.0;
    let dt = time.delta_secs().min(0.1);
    let clamped = rig.collision_clamped_dist(world);
    // 一阶指数平滑（无振荡）。
    let alpha = 1.0 - (-dt * DIST_SMOOTHING).exp();
    rig.dist += (clamped - rig.dist) * alpha;
    rig.dist = rig.dist.clamp(1.5, DIST_MAX);
    let eye = rig.eye();
    transform.translation = eye;
    transform.look_at(rig.focus, Vec3::Y);
    debug_assert!(eye.is_finite(), "相机位置必须有限");
}

/// Bevy 资源包装。
#[derive(Resource, Default)]
pub struct CameraRigRes(pub CameraRig);

/// 世界资源包装（渲染/相机/拾取共享同一权威对象）。
#[derive(Resource, Default)]
pub struct WorldRes(pub Option<World>);

impl WorldRes {
    pub fn world(&self) -> &World {
        self.0.as_ref().expect("世界必须已加载")
    }
    pub fn world_mut(&mut self) -> &mut World {
        self.0.as_mut().expect("世界必须已加载")
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<CameraRigRes>();
    app.add_systems(
        Update,
        (camera_input_system, camera_solve_system.in_set(CameraSolve))
            .chain()
            .run_if(in_state(GameState::Ready)),
    );
    // 运行期间 rig 尚未初始化时（WorldRes 为空）跳过求解。
    app.add_systems(OnEnter(GameState::Ready), init_rig_default);
}

fn init_rig_default(mut rig: ResMut<CameraRigRes>, world: Res<WorldRes>) {
    let world = world.world();
    let center = Vec3::new(world.size.x as f32 / 2.0, 40.0, world.size.z as f32 / 2.0);
    rig.0 = CameraRig::new(
        (
            world.size.x as f32,
            world.size.y as f32,
            world.size.z as f32,
        ),
        center,
        0.8,
        0.9,
        60.0,
    );
}
