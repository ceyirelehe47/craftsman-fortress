//! 自由透视相机：焦点 + 轨道模型（任务书 4.5）。
//!
//! - Perspective 投影，围绕可移动"焦点"旋转 / 俯仰 / 缩放；
//! - 俯仰、距离、焦点范围全部受限且稳定；
//! - 相机不得停留于实体体素内：从焦点向相机方向做 DDA，命中则压缩轨道距离
//!   （可收缩至 0，眼位退到焦点）；一阶指数平滑收敛，无振荡（A08）；
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

    /// 期望眼位（用户/脚本目标距离版，不含碰撞与平滑）。
    /// 验收的"控制连续性"断言基于它——碰撞收缩是安全机制，其速度由几何需要决定。
    pub fn target_eye(&self) -> Vec3 {
        let mut rig = self.clone();
        rig.dist = rig.target_dist;
        rig.desired_eye()
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
            // 眼位必须留在命中面外侧 margin 处。命中面距焦点不足 margin
            // （含 t=0：焦点恰在墙面上——如边界钳制线 z=8 与谷壁平面重合）
            // 时收缩到 0：眼位退到焦点（空气），绝不允许被距离下限顶回实体。
            Some(hit) => (hit.t - COLLISION_MARGIN).max(0.0),
            None => self.target_dist,
        };
        limit.min(self.target_dist)
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
    // 平滑只作用于"放宽"方向：先向用户目标距离收敛，再被碰撞上限硬性钳住。
    // 收缩即时（防穿模优先，消除平滑超前导致的瞬态入模）；clamped 由几何决定、
    // 不依赖 dist，无反馈回路 => 不可能振荡（A08）。
    let alpha = 1.0 - (-dt * DIST_SMOOTHING).exp();
    rig.dist += (rig.target_dist - rig.dist) * alpha;
    rig.dist = rig.dist.min(clamped);
    // 下限 0：碰撞钳制允许距离收缩到 0（眼位退到焦点），求解层不得反弹。
    rig.dist = rig.dist.clamp(0.0, DIST_MAX);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generation::TerrainParams;
    use crate::voxel::BlockId;

    /// 空气世界 + 指定实体体素（不动 dirty 集，纯查询测试）。
    fn world_with(solid: &[IVec3]) -> World {
        let mut w = World::empty(
            crate::coords::WorldSize::new(64, 64, 64),
            TerrainParams::new(1),
        );
        w.fill_air_all_for_test();
        for v in solid {
            w.set_voxel(*v, BlockId::Stone).unwrap();
        }
        w
    }

    /// 焦点恰在墙面（z=8.0 整数格线，邻列 z=7 为 51m 实体柱）时，
    /// 碰撞钳制必须把距离收缩到 0（眼位退到焦点空气格），
    /// 而不是被距离下限顶进实体（A08 第一轮根因，run4 首例 (122,40,4)）。
    #[test]
    fn collision_focus_on_wall_plane_collapses_dist_to_zero() {
        let mut solid = Vec::new();
        // z=7 列：高度 40 的实体柱（焦点高度 34 朝 -z 必然命中）。
        for y in 0..40 {
            for x in 8..14 {
                solid.push(IVec3::new(x, y, 7));
            }
        }
        let world = world_with(&solid);
        let rig = CameraRig::new(
            (64.0, 64.0, 64.0),
            Vec3::new(11.0, 34.0, 8.0),
            3.5,  // yaw 朝 -z
            0.30, // 低角度
            8.0,
        );
        let d = rig.collision_clamped_dist(&world);
        assert_eq!(d, 0.0, "焦点贴墙面时距离应收缩到 0");
        let eye = {
            let mut r = rig.clone();
            r.dist = d;
            r.desired_eye()
        };
        let v = IVec3::new(
            eye.x.floor() as i32,
            eye.y.floor() as i32,
            eye.z.floor() as i32,
        );
        assert!(!world.voxel(v).is_solid(), "眼位 {eye:?} 不得在实体内");
    }

    /// 墙面距焦点 ~2.5m 且足够高：距离收缩到 hit.t - margin，眼位留在空气侧。
    #[test]
    fn collision_shrinks_to_margin_before_wall() {
        let mut solid = Vec::new();
        // x=7..14、y=8..14、z=4..7 的实体墙（射线含 -x 漂移与 +0.21 rad 抬升仍命中）。
        for y in 8..14 {
            for z in 4..7 {
                for x in 7..14 {
                    solid.push(IVec3::new(x, y, z));
                }
            }
        }
        let world = world_with(&solid);
        let rig = CameraRig::new(
            (64.0, 64.0, 64.0),
            Vec3::new(10.5, 10.5, 9.5),
            3.5,
            PITCH_MIN, // 微小抬升，保证非退化方向
            30.0,
        );
        let d = rig.collision_clamped_dist(&world);
        assert!(d < 30.0, "应发生碰撞收缩，实际 {d}");
        let eye = {
            let mut r = rig.clone();
            r.dist = d;
            r.desired_eye()
        };
        let v = IVec3::new(
            eye.x.floor() as i32,
            eye.y.floor() as i32,
            eye.z.floor() as i32,
        );
        assert!(
            !world.voxel(v).is_solid(),
            "收缩后眼位 {eye:?} 不得在实体内"
        );
    }

    /// 无命中：距离保持目标值不变。
    #[test]
    fn collision_no_hit_keeps_target_dist() {
        let world = world_with(&[]);
        let rig = CameraRig::new(
            (64.0, 64.0, 64.0),
            Vec3::new(32.0, 40.0, 32.0),
            0.8,
            0.9,
            60.0,
        );
        assert_eq!(rig.collision_clamped_dist(&world), 60.0);
    }
}
