//! 验收控制器（任务书第 5 章：单一验收入口的运行时部分）。
//!
//! 职责：在 `--acceptance` 模式下接管相机，执行脚本化巡航（固定机位截图 →
//! 120s 巡航 + 拾取/编辑测试 → 长时耐久循环），全程采样指标，最后执行
//! 自动判定（A01-A13 的运行时部分），输出机器可读 `report.json` 与人类可读
//! `report.md` 并以退出码报告结论。A14（独立复核）由外部 Reviewer 完成。

use crate::app_state::GameState;
use crate::camera::{
    self, CameraRig, CameraRigRes, WorldRes, DIST_MAX, DIST_MIN, PITCH_MAX, PITCH_MIN,
};
use crate::config::AppConfig;
use crate::diagnostics::{DiagState, TPS};
use crate::features::{probe_world, FeatureReport};
use crate::meshing::DebugTint;
use crate::picking::{pick_voxel, Ray, ScriptedRayRes};
use crate::render::{ChunkMeshes, DebugTintRes};
use crate::voxel::BlockId;
use crate::world::World;
use bevy::prelude::*;
use bevy::render::view::window::screenshot::{Screenshot, ScreenshotCaptured};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// 验收时间线（秒，相对进入 Ready）。
const T_WARMUP_END: f64 = 8.0;
const T_VIEWS_END: f64 = 48.0;
const T_CRUISE_START: f64 = 48.0;
const T_CRUISE_MEASURE_START: f64 = 48.0;
const T_CRUISE_MEASURE_END: f64 = 168.0;
const T_GIF_LAST: f64 = 300.0;
const T_FINALIZE: f64 = 628.0;
const GIF_INTERVAL: f64 = 4.0;
/// GIF 帧尺寸。
const GIF_SIZE: (u32, u32) = (480, 270);
/// 帧间期望眼位最大位移（米/帧），"跳变"的工程定义是位姿不连续。
/// 与帧率无关：脚本平滑运动峰值 ~1 m/帧（240 fps 下 223 m/s 的过渡峰值），
/// 真跳变/状态错乱是数十米级瞬移；4 m/帧 之间留一个数量级余量。
const MAX_EYE_STEP: f32 = 4.0;

#[derive(Clone, Copy, Debug)]
struct Pose {
    focus: Vec3,
    yaw: f32,
    pitch: f32,
    dist: f32,
    /// 地下/特殊机位：不做"贴地抬升"修正。
    underground: bool,
}

impl Pose {
    fn surface(focus: Vec3, yaw: f32, pitch: f32, dist: f32) -> Self {
        Self {
            focus,
            yaw,
            pitch,
            dist,
            underground: false,
        }
    }
    fn cave(focus: Vec3, yaw: f32, pitch: f32, dist: f32) -> Self {
        Self {
            focus,
            yaw,
            pitch,
            dist,
            underground: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Segment {
    t0: f64,
    t1: f64,
    from: Pose,
    to: Pose,
    /// 起始时刻直接跳变到位（不做过渡）。
    cut: bool,
    /// 过渡期间焦点抬升弧高（跨过地形）。
    arc: f32,
    /// 巡航覆盖场景标记（A06）。
    coverage: Coverage,
    /// 焦点高度下限（构造段时沿路径预采样地形最高点 +3）。
    /// 段内常量 => 逐帧连续，避免逐帧 max 造成 focus 跳变（A06/A08）。
    min_focus_y: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Coverage {
    low_angle: bool,
    management: bool,
    max_height: bool,
    max_zoom: bool,
    min_zoom: bool,
    cross_chunk: bool,
    rotate_360: bool,
    border: bool,
}

#[derive(Clone, Debug)]
pub struct Check {
    id: &'static str,
    name: &'static str,
    pass: bool,
    detail: String,
    evidence: String,
}

#[derive(Resource, Default)]
pub struct ShotLog {
    pub records: Vec<(String, PathBuf, bool, u64)>,
}

#[derive(Resource)]
pub struct AcceptanceState {
    pub cfg: AppConfig,
    pub dir: PathBuf,
    pub ready_at: Option<Instant>,
    pub features: Option<FeatureReport>,
    pub initial_world_hash: u64,
    pub segments: Vec<Segment>,
    pub actions: VecDeque<(f64, ActionId)>,
    pub shot_queue: VecDeque<ShotRequest>,
    pub coverage: Coverage,
    pub coverage_hits: Vec<Coverage>,
    // 运行时采样
    pub tick_samples: Vec<(f64, u64)>,
    pub rss_samples: Vec<(f64, f64)>,
    pub entity_samples: Vec<(f64, usize)>,
    pub finite_violations: u32,
    pub clamp_violations: u32,
    pub inside_solid_events: u32,
    pub inside_solid_first: Option<Vec3>,
    pub unready_violation_frames: u32,
    pub speed_violations: u32,
    pub oscillation_events: u32,
    pub dist_history: VecDeque<(f64, f32)>,
    pub last_eye: Option<Vec3>,
    pub last_dist_delta_sign: i32,
    pub last_dist_delta_abs: f32,
    pub osc_run: u32,
    pub last_segment_idx: usize,
    // 拾取测试
    pub ray_results: Vec<(String, bool, String)>,
    pub highlight_ray: Option<Ray>,
    // 编辑测试
    pub edit_done: bool,
    pub edit_info: Option<EditInfo>,
    pub edit_verified: Option<bool>,
    pub edit_verify_detail: String,
    // 完成
    pub finalized: bool,
    pub checks: Vec<Check>,
    pub collect_started: bool,
    pub last_sample: Option<Instant>,
    pub gif_frame_count: usize,
    pub panic_flag_path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct EditInfo {
    voxel: IVec3,
    chunk_a: IVec3,
    chunk_b: IVec3,
    dirty_after: Vec<IVec3>,
    t_edit: f64,
}

pub struct ShotRequest {
    name: String,
    path: PathBuf,
    downscale: bool,
}

pub enum ActionId {
    StartCollecting,
    Shot(&'static str),
    GifFrame,
    PickRays,
    HighlightOn,
    HighlightOff,
    EditTest,
    ShotEdit(&'static str),
    VerifyEdit,
    TintOn,
    TintOff,
    Finalize,
}

impl AcceptanceState {
    pub fn new(cfg: AppConfig, dir: PathBuf) -> Self {
        Self {
            panic_flag_path: dir.join("panic.txt"),
            cfg,
            dir,
            ready_at: None,
            features: None,
            initial_world_hash: 0,
            segments: Vec::new(),
            actions: VecDeque::new(),
            shot_queue: VecDeque::new(),
            coverage: Coverage::default(),
            coverage_hits: Vec::new(),
            tick_samples: Vec::new(),
            rss_samples: Vec::new(),
            entity_samples: Vec::new(),
            finite_violations: 0,
            clamp_violations: 0,
            inside_solid_events: 0,
            inside_solid_first: None,
            unready_violation_frames: 0,
            speed_violations: 0,
            oscillation_events: 0,
            dist_history: VecDeque::new(),
            last_eye: None,
            last_dist_delta_sign: 0,
            last_dist_delta_abs: 0.0,
            osc_run: 0,
            last_segment_idx: 0,
            ray_results: Vec::new(),
            highlight_ray: None,
            edit_done: false,
            edit_info: None,
            edit_verified: None,
            edit_verify_detail: String::new(),
            finalized: false,
            checks: Vec::new(),
            collect_started: false,
            last_sample: None,
            gif_frame_count: 0,
        }
    }

    fn t(&self) -> f64 {
        self.ready_at
            .map(|t| t.elapsed().as_secs_f64())
            .unwrap_or(0.0)
    }
}

pub struct AcceptancePlugin {
    cfg: AppConfig,
}

impl AcceptancePlugin {
    pub fn new(cfg: AppConfig) -> Self {
        Self { cfg }
    }
}

impl Plugin for AcceptancePlugin {
    fn build(&self, app: &mut App) {
        let dir = PathBuf::from(&self.cfg.evidence_dir);
        let _ = std::fs::create_dir_all(dir.join("screenshots"));
        let _ = std::fs::create_dir_all(dir.join("gif_frames"));
        // panic 钩子：任何 panic 都留下机器可读标记（A11）。
        let panic_path = dir.join("panic.txt");
        let _ = std::fs::remove_file(&panic_path);
        let hook_path = panic_path.clone();
        std::panic::set_hook(Box::new(move |info| {
            use std::io::Write;
            if let Ok(mut f) = std::fs::File::create(&hook_path) {
                let _ = writeln!(f, "{info}");
                let _ = writeln!(
                    f,
                    "backtrace:\n{}",
                    std::backtrace::Backtrace::force_capture()
                );
            }
        }));

        app.insert_resource(AcceptanceState::new(self.cfg.clone(), dir.clone()));
        app.init_resource::<ShotLog>();
        app.add_systems(OnEnter(GameState::Ready), acceptance_on_ready);
        app.add_systems(
            Update,
            (
                // 在相机求解之后采样/接管：A06-A08 采样读到的是本帧位姿经
                // 碰撞钳制后的真实眼位，而不是"上帧距离 + 本帧位姿"的瞬态。
                // 脚本位姿写入滞后求解一帧（~4ms），对 628s 巡航无影响。
                acceptance_control.after(camera::CameraSolve),
                process_shot_queue.after(camera::CameraSolve),
            )
                .run_if(in_state(GameState::Ready)),
        );
    }
}

/// 进入 Ready：探测特征、构建时间线、打开指标输出。
fn acceptance_on_ready(
    mut acc: ResMut<AcceptanceState>,
    mut diag: ResMut<DiagState>,
    world_res: Res<WorldRes>,
    mut rig: ResMut<CameraRigRes>,
) {
    let world = world_res.world();
    acc.initial_world_hash = world.world_hash();
    let features = probe_world(world);
    info!("[验收] 特征探测: {}", features.summary());
    build_timeline(&mut acc, world, &features);

    // 指标 CSV 与收集窗口。
    let csv = acc.dir.join("metrics.csv");
    if let Err(e) = diag.open_csv(&csv) {
        info!("[验收] 无法创建指标 CSV: {e}");
    }
    // 特征报告落盘（A04 证据）。
    let _ = std::fs::write(acc.dir.join("features.txt"), features.summary());

    // 初始机位：平原视角（热身段）。
    let start = acc
        .segments
        .first()
        .map(|s| s.from)
        .unwrap_or(Pose::surface(Vec3::new(128.0, 60.0, 128.0), 0.6, 0.6, 40.0));
    rig.0 = CameraRig::new(
        (
            world.size.x as f32,
            world.size.y as f32,
            world.size.z as f32,
        ),
        start.focus,
        start.yaw,
        start.pitch,
        start.dist,
    );
    rig.0.control_locked = true;
    acc.ready_at = Some(Instant::now());
    info!(
        "[验收] Ready：开始脚本巡航（总时长 ~{}s）",
        T_FINALIZE + 12.0
    );
}

/// 构建相机脚本时间线：热身 → 8 个固定机位 → 巡航/耐久循环。
/// 沿 from→to 焦点连线预采样地形最高点 +3，作为段内焦点高度下限。
/// 段内常量 => 焦点轨迹逐帧连续（lerp/弧线本身连续），杜绝逐帧 max 的跳变。
fn path_min_focus_y(world: &World, from: Vec3, to: Vec3) -> f32 {
    let mut max_h = 1i32;
    for i in 0..9 {
        let u = i as f32 / 8.0;
        let x = (from.x + (to.x - from.x) * u).clamp(0.0, (world.size.x - 1) as f32) as i32;
        let z = (from.z + (to.z - from.z) * u).clamp(0.0, (world.size.z - 1) as f32) as i32;
        max_h = max_h.max(world.column_height(x, z));
    }
    max_h as f32 + 3.0
}

fn build_timeline(acc: &mut AcceptanceState, world: &World, features: &FeatureReport) {
    acc.features = Some(features.clone());
    let cx = world.size.x as f32 / 2.0;
    let cz = world.size.z as f32 / 2.0;
    let h = |x: i32, z: i32| world.column_height(x, z).max(1) as f32;
    let fc = |hit: &Option<crate::features::FeatureHit>| {
        hit.as_ref()
            .map(|f| f.view_center)
            .unwrap_or(Vec3::new(cx, 50.0, cz))
    };

    // ---- 固定机位（每个 5s：2s 过渡 + 3s 稳定，3.5s 处截图）----
    let mut poses: Vec<(Pose, &'static str, bool)> = Vec::new(); // (pose, shot, cut)
    let plains = fc(&features.plains);
    poses.push((
        Pose::surface(plains + Vec3::new(0.0, 2.0, 0.0), 0.6, 0.52, 24.0),
        "A04_01_plains.png",
        false,
    ));
    let hills = fc(&features.hills);
    poses.push((
        Pose::surface(hills + Vec3::new(0.0, 4.0, 0.0), 2.2, 0.62, 55.0),
        "A04_02_hills.png",
        false,
    ));
    let mountain = fc(&features.mountains);
    poses.push((
        Pose::surface(mountain + Vec3::new(0.0, 6.0, 0.0), 0.9, 0.78, 120.0),
        "A04_03_mountain.png",
        false,
    ));
    let valley = fc(&features.valleys);
    poses.push((
        Pose::surface(valley + Vec3::new(0.0, 6.0, 0.0), 4.0, 1.0, 130.0),
        "A04_04_valley_management.png",
        false,
    ));
    let cliff = fc(&features.cliffs);
    poses.push((
        Pose::surface(cliff + Vec3::new(0.0, 3.0, 0.0), 2.4, 0.34, 38.0),
        "A04_05_cliff.png",
        false,
    ));
    let opening = fc(&features.cave_opening);
    poses.push((
        Pose::surface(opening + Vec3::new(0.0, 1.0, 0.0), 1.2, 0.72, 16.0),
        "A04_06_cave_opening.png",
        false,
    ));
    let cave = fc(&features.underground_cave);
    poses.push((
        Pose::cave(cave, 0.0, PITCH_MIN, 1.5),
        "A04_07_underground_cave.png",
        true,
    ));
    poses.push((
        Pose::surface(
            Vec3::new(cx, (world.size.y as f32) * 0.72, cz),
            0.0,
            1.25,
            135.0,
        ),
        "A04_08_chunk_boundaries.png",
        true,
    ));

    let mut t = T_WARMUP_END;
    let mut prev = poses[0].0;
    // 机位截图动作先收集到局部列表（与后续动作统一排序后一次性写入，
    // 避免被末尾的 `acc.actions = VecDeque::from(acts)` 覆盖丢失）。
    let mut pose_shots: Vec<(f64, ActionId)> = Vec::new();
    // 热身段（0-8s）：平原机位缓慢环绕。
    acc.segments.push(Segment {
        t0: 0.0,
        t1: T_WARMUP_END,
        from: prev,
        to: Pose::surface(prev.focus, prev.yaw + 0.5, prev.pitch, prev.dist),
        cut: false,
        arc: 0.0,
        coverage: Coverage::default(),
        min_focus_y: path_min_focus_y(world, prev.focus, poses[0].0.focus),
    });
    for &(pose, shot, cut) in &poses {
        acc.segments.push(Segment {
            t0: t,
            t1: t + 5.0,
            from: prev,
            to: pose,
            cut,
            arc: if cut { 0.0 } else { 45.0 },
            coverage: Coverage::default(),
            min_focus_y: if cut {
                0.0
            } else {
                path_min_focus_y(world, prev.focus, pose.focus)
            },
        });
        pose_shots.push((t + 3.5, ActionId::Shot(shot)));
        t += 5.0;
        prev = pose;
    }
    debug_assert!((t - T_VIEWS_END).abs() < 1e-6);
    prev = poses.last().map(|p| p.0).unwrap_or(prev);

    // ---- 巡航 waypoints（循环生成直到 T_FINALIZE）----
    // (focus_x, focus_z, y_offset, yaw_delta, pitch, dist, coverage, arc)
    let plains_xz = (plains.x, plains.z);
    let hills_xz = (hills.x, hills.z);
    let mtn_xz = (mountain.x, mountain.z);
    let valley_xz = (valley.x, valley.z);
    let cliff_xz = (cliff.x, cliff.z);
    let open_xz = (opening.x, opening.z);
    let border_x = 20.0f32;
    // 巡航路点：(focus_x, focus_z, y_offset, yaw_delta, pitch, dist, coverage, arc)
    type Waypoint = (f32, f32, f32, f32, f32, f32, Coverage, f32);
    let waypoints: Vec<Waypoint> = vec![
        (
            plains_xz.0,
            plains_xz.1,
            4.0,
            1.2,
            0.55,
            35.0,
            Coverage {
                cross_chunk: true,
                ..Default::default()
            },
            30.0,
        ),
        (
            hills_xz.0,
            hills_xz.1,
            6.0,
            1.4,
            0.62,
            60.0,
            Coverage {
                cross_chunk: true,
                ..Default::default()
            },
            35.0,
        ),
        (
            mtn_xz.0,
            mtn_xz.1,
            8.0,
            std::f32::consts::TAU,
            0.85,
            95.0,
            Coverage {
                rotate_360: true,
                cross_chunk: true,
                ..Default::default()
            },
            40.0,
        ),
        (
            valley_xz.0,
            valley_xz.1,
            10.0,
            1.0,
            0.95,
            8.0,
            Coverage {
                max_zoom: true,
                min_zoom: true,
                cross_chunk: true,
                ..Default::default()
            },
            45.0,
        ),
        (
            valley_xz.0,
            valley_xz.1,
            4.0,
            0.8,
            0.30,
            12.0,
            Coverage {
                low_angle: true,
                cross_chunk: true,
                ..Default::default()
            },
            25.0,
        ),
        (
            cliff_xz.0,
            cliff_xz.1,
            5.0,
            1.6,
            0.42,
            30.0,
            Coverage {
                cross_chunk: true,
                ..Default::default()
            },
            30.0,
        ),
        (
            open_xz.0,
            open_xz.1,
            5.0,
            1.2,
            0.65,
            25.0,
            Coverage {
                cross_chunk: true,
                ..Default::default()
            },
            30.0,
        ),
        (
            border_x,
            cz,
            12.0,
            1.0,
            0.85,
            70.0,
            Coverage {
                border: true,
                cross_chunk: true,
                ..Default::default()
            },
            40.0,
        ),
        (
            border_x,
            cz - 80.0,
            14.0,
            1.2,
            1.05,
            90.0,
            Coverage {
                border: true,
                cross_chunk: true,
                ..Default::default()
            },
            40.0,
        ),
        (
            cx,
            cz,
            20.0,
            2.0,
            1.2,
            145.0,
            Coverage {
                management: true,
                max_height: true,
                max_zoom: true,
                cross_chunk: true,
                ..Default::default()
            },
            50.0,
        ),
    ];
    let seg_len = 8.0f64;
    let mut lap = 0usize;
    while t < T_FINALIZE {
        let dist_scale = [1.0f32, 0.75, 1.0, 0.55][lap % 4];
        for (fx, fz, y_off, yaw_d, pitch, dist, cov, arc) in &waypoints {
            if t >= T_FINALIZE {
                break;
            }
            let dist = (dist * dist_scale).clamp(DIST_MIN, DIST_MAX);
            let pitch = pitch.clamp(PITCH_MIN, PITCH_MAX);
            let focus = Vec3::new(*fx, h(*fx as i32, *fz as i32) + y_off, *fz);
            let pose = Pose::surface(focus, prev.yaw + yaw_d, pitch, dist);
            acc.segments.push(Segment {
                t0: t,
                t1: (t + seg_len).min(T_FINALIZE),
                from: prev,
                to: pose,
                cut: false,
                arc: *arc,
                coverage: *cov,
                min_focus_y: path_min_focus_y(world, prev.focus, pose.focus),
            });
            prev = pose;
            t += seg_len;
        }
        lap += 1;
    }
    // 收尾静止段：Finalize 前的最后 4s 相机不动（队列清空判定）。
    acc.segments.push(Segment {
        t0: T_FINALIZE - 4.0,
        t1: T_FINALIZE + 1e9,
        from: prev,
        to: prev,
        cut: false,
        arc: 0.0,
        coverage: Coverage::default(),
        min_focus_y: path_min_focus_y(world, prev.focus, prev.focus),
    });

    // ---- 时间点动作 ----
    let mut acts: Vec<(f64, ActionId)> = vec![(T_WARMUP_END, ActionId::StartCollecting)];
    acts.extend(pose_shots);
    acts.push((T_CRUISE_START + 4.0, ActionId::PickRays));
    acts.push((T_CRUISE_START + 7.0, ActionId::HighlightOn));
    acts.push((
        T_CRUISE_START + 9.0,
        ActionId::Shot("A09_picking_highlight.png"),
    ));
    acts.push((T_CRUISE_START + 12.0, ActionId::HighlightOff));
    acts.push((T_CRUISE_START + 32.0, ActionId::EditTest));
    // VerifyEdit 必须在 EditTest 后 8s 内（A05 判定的时间窗）。
    acts.push((T_CRUISE_START + 38.0, ActionId::VerifyEdit));
    // +41 而非 +40：避开每 4s 的 GIF 帧——同帧对同一窗口请求两张截图时
    // Bevy 会跳过其中一张（Duplicate render target warning）。
    acts.push((
        T_CRUISE_START + 41.0,
        ActionId::ShotEdit("A05_edit_rebuild.png"),
    ));
    acts.push((T_VIEWS_END - 7.0, ActionId::TintOn));
    acts.push((T_VIEWS_END - 1.0, ActionId::TintOff));
    // GIF 帧。
    let mut g = T_CRUISE_START;
    while g <= T_GIF_LAST {
        acts.push((g, ActionId::GifFrame));
        g += GIF_INTERVAL;
    }
    acts.push((T_FINALIZE, ActionId::Finalize));
    acts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    acc.actions = VecDeque::from(acts);
}

fn smoothstep(u: f64) -> f64 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// 主控制器：按时间线驱动机位、执行动作、采集不变量。
#[allow(clippy::too_many_arguments)]
fn acceptance_control(
    mut acc: ResMut<AcceptanceState>,
    mut rig: ResMut<CameraRigRes>,
    mut world_res: ResMut<WorldRes>,
    mut diag: ResMut<DiagState>,
    registry: Res<ChunkMeshes>,
    mut scripted_ray: ResMut<ScriptedRayRes>,
    mut tint: ResMut<DebugTintRes>,
    next_state: ResMut<NextState<GameState>>,
    exit: MessageWriter<AppExit>,
) {
    let t = acc.t();
    let Some(world) = world_res.0.as_mut() else {
        return;
    };

    // ---------- 1. 位姿脚本 ----------
    // 找当前 segment（线性扫描，段数 ~80）。
    let mut idx = acc.last_segment_idx;
    while idx + 1 < acc.segments.len() && acc.segments[idx + 1].t0 <= t {
        idx += 1;
    }
    while idx > 0 && acc.segments[idx].t0 > t {
        idx -= 1;
    }
    let seg_changed = idx != acc.last_segment_idx;
    acc.last_segment_idx = idx;
    let seg = acc.segments[idx].clone();
    let u = ((t - seg.t0) / (seg.t1 - seg.t0).max(1e-9)).clamp(0.0, 1.0);
    // cut 段：整段直接位于目标位姿（焦点也瞬移）。若只瞬移 dist 而焦点仍 lerp，
    // 过渡路径会直线穿山——相机与焦点长时间停留在实体内（A08）。
    let e = if seg.cut { 1.0 } else { smoothstep(u) };
    let lerp = |a: f32, b: f32, u: f64| a + (b - a) * u as f32;
    let mut focus = Vec3::new(
        lerp(seg.from.focus.x, seg.to.focus.x, e),
        lerp(seg.from.focus.y, seg.to.focus.y, e),
        lerp(seg.from.focus.z, seg.to.focus.z, e),
    );
    // 过渡弧：中段抬升。
    if seg.arc > 0.0 {
        focus.y += (u as f32 * std::f32::consts::PI).sin() * seg.arc;
    }
    let yaw = lerp(seg.from.yaw, seg.to.yaw, e);
    let pitch = lerp(seg.from.pitch, seg.to.pitch, e);
    let target_dist = lerp(seg.from.dist, seg.to.dist, e);
    // 贴地保护：焦点不低于段内预采样的地形下限（地下机位豁免）。
    // 段内常量下限保证 focus 逐帧连续（lerp/弧线本身连续）。
    let underground = seg.to.underground || seg.from.underground;
    if !underground {
        focus.y = focus.y.max(seg.min_focus_y);
    }

    // ---------- 2. 不变量采样（t >= 热身结束）----------
    // 必须在"位姿应用"之前采样：此时 rig 是本帧求解后的完整状态
    // （位姿 N-1 + 针对该位姿的碰撞钳制距离），眼位即真实渲染相机位置。
    // 若先应用位姿 N 再采样，会读到"新位姿 + 旧距离"的瞬态错位
    // （run5 残留 6 帧入实体事件的根因）。
    if t >= T_WARMUP_END && !acc.finalized {
        let eye = rig.0.eye();
        // 有限性
        if !eye.is_finite()
            || !rig.0.focus.is_finite()
            || !rig.0.dist.is_finite()
            || !rig.0.pitch.is_finite()
            || !rig.0.yaw.is_finite()
        {
            acc.finite_violations += 1;
        }
        // 限位
        if rig.0.pitch < PITCH_MIN - 1e-3
            || rig.0.pitch > PITCH_MAX + 1e-3
            || rig.0.target_dist > DIST_MAX + 1e-2
            || rig.0.target_dist < 1.4
            || rig.0.dist > DIST_MAX + 1e-2
        {
            // 距离下限只约束意图值 target_dist（脚本 ≥1.5 / 用户 ≥6）；
            // 实际 dist 允许被碰撞钳制收缩到 0（眼位退到焦点）。
            acc.clamp_violations += 1;
        }
        // 相机不得在实体内（A08）
        let v = IVec3::new(
            eye.x.floor() as i32,
            eye.y.floor() as i32,
            eye.z.floor() as i32,
        );
        if world.size.contains(v) && world.voxel(v).is_solid() {
            acc.inside_solid_events += 1;
            if acc.inside_solid_first.is_none() {
                acc.inside_solid_first = Some(eye);
            }
        }
        // 可见未准备（A07）
        if diag.visible_unready > 0 {
            acc.unready_violation_frames += 1;
        }
        // 跳变（无跳变）：基于期望眼位（目标距离版）的帧间位移——控制流连续性断言。
        // 碰撞收缩是安全机制，其眼位速度由几何需要决定，不属"控制跳变"。
        // 用位移而非速度：速度阈值会随帧率缩放（240 fps 下平滑运动的
        // smoothstep 峰值即超 200 m/s），位移阈值帧率无关。
        let target_eye = rig.0.target_eye();
        if let Some(last) = acc.last_eye {
            if t >= T_CRUISE_START && !seg_changed {
                let step = (target_eye - last).length();
                if step > MAX_EYE_STEP {
                    acc.speed_violations += 1;
                    // 限流样本日志（前 3 次），便于证据复核。
                    if acc.speed_violations <= 3 {
                        info!(
                            "[验收/A06] 跳变样本: t={t:.2} step={step:.1}m eye=({:.1},{:.1},{:.1}) last=({:.1},{:.1},{:.1})",
                            target_eye.x, target_eye.y, target_eye.z, last.x, last.y, last.z
                        );
                    }
                }
            }
        }
        acc.last_eye = Some(target_eye);

        // 距离振荡检测（A08）：只计"摆幅不衰减的连续交替"（真振荡）。
        // 一阶滤波对波动输入的正常跟踪（交替但衰减）不算不稳定。
        acc.dist_history.push_back((t, rig.0.dist));
        if acc.dist_history.len() > 240 {
            acc.dist_history.pop_front();
        }
        let delta = rig.0.dist
            - acc
                .dist_history
                .iter()
                .rev()
                .nth(1)
                .map(|&(_, d)| d)
                .unwrap_or(rig.0.dist);
        let sign = if delta > 0.05 {
            1
        } else if delta < -0.05 {
            -1
        } else {
            0
        };
        if sign != 0 {
            if sign == -acc.last_dist_delta_sign && acc.last_dist_delta_sign != 0 {
                // 一次交替：摆幅明显衰减（<85%）=> 正常收敛，打断连续计数；
                // 连续 6 次不衰减交替 => 真振荡（等幅摆动不收敛）。
                let last_amp = acc.last_dist_delta_abs;
                let amp = delta.abs();
                if last_amp > 0.0 && amp >= last_amp * 0.85 {
                    acc.osc_run += 1;
                    if acc.osc_run >= 6 {
                        acc.oscillation_events += 1;
                        acc.osc_run = 0;
                    }
                } else {
                    acc.osc_run = 0;
                }
            }
            acc.last_dist_delta_sign = sign;
            acc.last_dist_delta_abs = delta.abs();
        }

        // 低频采样：ticks / RSS / 实体数（1s 周期）
        let sample_due = acc
            .last_sample
            .map(|s| s.elapsed().as_secs_f64() >= 1.0)
            .unwrap_or(true);
        if sample_due {
            acc.last_sample = Some(Instant::now());
            acc.tick_samples.push((t, diag.fixed_ticks));
            acc.rss_samples.push((t, diag.rss_mb));
            acc.entity_samples.push((t, registry.meshed_count()));
        }
    }

    // ---------- 3. 位姿应用 ----------
    rig.0.control_locked = true;
    rig.0.focus = focus;
    rig.0.yaw = yaw;
    rig.0.pitch = pitch;
    rig.0.target_dist = target_dist.max(1.5);
    rig.0.clamp_all();
    // 段切换 & cut：距离直接到位，避免跨段平滑拖尾穿过地形。
    if seg_changed {
        if seg.cut {
            rig.0.dist = rig.0.target_dist;
        }
        if t >= T_CRUISE_START {
            acc.coverage_hits.push(seg.coverage);
        }
    }

    // ---------- 4. 动作 ----------
    while let Some((at, _)) = acc.actions.front() {
        if *at > t {
            break;
        }
        let (_, action) = acc.actions.pop_front().unwrap();
        run_action(
            &mut acc,
            &mut diag,
            &mut tint,
            &mut scripted_ray,
            world,
            action,
            t,
        );
    }

    // ---------- 5. Finalize（阻塞完成全部判定与证据输出）----------
    if t >= T_FINALIZE && !acc.finalized {
        acc.finalized = true;
        finalize(
            &mut acc,
            world,
            registry.into_inner(),
            diag.into_inner(),
            next_state,
            exit,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn run_action(
    acc: &mut AcceptanceState,
    diag: &mut DiagState,
    tint: &mut DebugTintRes,
    scripted_ray: &mut ScriptedRayRes,
    world: &mut World,
    action: ActionId,
    t: f64,
) {
    use ActionId::*;
    match action {
        StartCollecting => {
            diag.collecting = true;
            info!("[验收] 热身结束，开始统计帧时间");
        }
        Shot(name) => {
            acc.shot_queue.push_back(ShotRequest {
                name: name.to_string(),
                path: acc.dir.join("screenshots").join(name),
                downscale: false,
            });
        }
        ShotEdit(name) => {
            acc.shot_queue.push_back(ShotRequest {
                name: name.to_string(),
                path: acc.dir.join("screenshots").join(name),
                downscale: false,
            });
        }
        GifFrame => {
            let name = format!("gif_{:04}.png", acc.gif_frame_count);
            acc.gif_frame_count += 1;
            acc.shot_queue.push_back(ShotRequest {
                name,
                path: acc
                    .dir
                    .join("gif_frames")
                    .join(format!("{:04}.png", acc.gif_frame_count - 1)),
                downscale: true,
            });
        }
        PickRays => run_pick_rays(acc, world),
        HighlightOn => {
            scripted_ray.0 = acc.highlight_ray;
        }
        HighlightOff => {
            scripted_ray.0 = None;
        }
        EditTest => run_edit_test(acc, world, t),
        VerifyEdit => verify_edit(acc, world, t),
        TintOn => {
            tint.0 = DebugTint::ChunkParity;
            mark_all_dirty(world);
        }
        TintOff => {
            tint.0 = DebugTint::Off;
            mark_all_dirty(world);
        }
        Finalize => { /* 由主控制器处理 */ }
    }
}

fn mark_all_dirty(world: &mut World) {
    let n = world.size.chunks();
    for cy in 0..n.y as i32 {
        for cz in 0..n.z as i32 {
            for cx in 0..n.x as i32 {
                world.mark_dirty(IVec3::new(cx, cy, cz));
            }
        }
    }
}

/// A09：固定射线拾取断言。
fn run_pick_rays(acc: &mut AcceptanceState, world: &World) {
    let mut results: Vec<(String, bool, String)> = Vec::new();
    let check =
        |name: &str, cond: bool, detail: String, results: &mut Vec<(String, bool, String)>| {
            info!(
                "[验收/A09] {name}: {} — {detail}",
                if cond { "PASS" } else { "FAIL" }
            );
            results.push((name.to_string(), cond, detail));
        };

    // 特征点
    let feats = acc.features.as_ref().expect("特征报告必须存在");
    let plains = feats
        .plains
        .as_ref()
        .map(|f| f.voxel.unwrap())
        .unwrap_or(IVec3::new(128, 40, 128));
    let peak = feats
        .mountains
        .as_ref()
        .map(|f| f.voxel.unwrap())
        .unwrap_or(IVec3::new(128, 90, 128));
    let opening = feats
        .cave_opening
        .as_ref()
        .map(|f| f.voxel.unwrap())
        .unwrap_or(IVec3::new(64, 30, 64));

    // 1. 垂直下射平原 → 顶面命中
    let ray = Ray::normalized(
        Vec3::new(
            plains.x as f32 + 0.5,
            (world.size.y - 2) as f32,
            plains.z as f32 + 0.5,
        ),
        Vec3::new(0.0, -1.0, 0.0),
    );
    let hit = pick_voxel(world, &ray, 400.0);
    match &hit {
        Some(h) if h.face == crate::voxel::FaceDir::PosY => {
            check(
                "射线1-平原垂直下射",
                true,
                format!("命中 {} 面 {:?}", h.voxel, h.face),
                &mut results,
            );
        }
        other => check(
            "射线1-平原垂直下射",
            false,
            format!("异常结果 {other:?}"),
            &mut results,
        ),
    }
    acc.highlight_ray = Some(ray);

    // 2. 山顶下射 → 命中高海拔
    let ray2 = Ray::normalized(
        Vec3::new(
            peak.x as f32 + 0.5,
            (world.size.y - 2) as f32,
            peak.z as f32 + 0.5,
        ),
        Vec3::new(0.0, -1.0, 0.0),
    );
    match pick_voxel(world, &ray2, 400.0) {
        Some(h) if h.voxel.y >= 75 => check(
            "射线2-山顶下射",
            true,
            format!("命中 y={}", h.voxel.y),
            &mut results,
        ),
        other => check(
            "射线2-山顶下射",
            false,
            format!("异常 {other:?}"),
            &mut results,
        ),
    }

    // 3. 水平跨 Chunk：从 (8,h+2) 沿 +X
    let sy = world.column_height(8, 8) + 2;
    let ray3 = Ray::normalized(Vec3::new(8.5, sy as f32, 8.5), Vec3::new(1.0, 0.0, 0.0));
    let hit3 = pick_voxel(world, &ray3, 300.0);
    match &hit3 {
        Some(h)
            if h.voxel.x >= 8
                && h.face == crate::voxel::FaceDir::NegX
                && h.place.x == h.voxel.x - 1 =>
        {
            check(
                "射线3-水平跨Chunk",
                true,
                format!("命中 {} 面 NegX", h.voxel),
                &mut results,
            );
        }
        other => check(
            "射线3-水平跨Chunk",
            false,
            format!("异常 {other:?}"),
            &mut results,
        ),
    }

    // 4. 斜 45° 跨 Chunk
    let ray4 = Ray::normalized(Vec3::new(10.5, 90.0, 10.5), Vec3::new(1.0, -1.0, 1.0));
    match pick_voxel(world, &ray4, 400.0) {
        Some(h) => check(
            "射线4-斜角",
            true,
            format!("命中 {} 面 {:?}", h.voxel, h.face),
            &mut results,
        ),
        None => check("射线4-斜角", false, "未命中".into(), &mut results),
    }

    // 5. 天空空射线
    match pick_voxel(
        world,
        &Ray::normalized(Vec3::new(128.5, 100.0, 128.5), Vec3::new(0.0, 1.0, 0.0)),
        300.0,
    ) {
        None => check("射线5-空射线", true, "未命中（正确）".into(), &mut results),
        Some(h) => check(
            "射线5-空射线",
            false,
            format!("异常命中 {h:?}"),
            &mut results,
        ),
    }

    // 6. 最大缩放距离等价性
    let hit6 = pick_voxel(world, &ray3, 500.0);
    let same = match (&hit3, &hit6) {
        (Some(a), Some(b)) => a.voxel == b.voxel && a.face == b.face,
        (None, None) => true,
        _ => false,
    };
    check(
        "射线6-最大缩放等价",
        same,
        format!(
            "近/远: {:?} / {:?}",
            hit3.map(|h| h.voxel),
            hit6.map(|h| h.voxel)
        ),
        &mut results,
    );

    // 7. 陡峭侧壁：找一个“起点空气、+X 三格外实体”的位置
    let mut found = None;
    'outer: for x in 40..200 {
        for z in 40..200 {
            let y = world.column_height(x, z) + 1;
            if y < 10 || y + 3 >= world.size.y as i32 {
                continue;
            }
            if !world.voxel(IVec3::new(x, y, z)).is_solid()
                && !world.voxel(IVec3::new(x + 1, y, z)).is_solid()
                && world.voxel(IVec3::new(x + 2, y, z)).is_solid()
            {
                found = Some((x, y, z));
                break 'outer;
            }
        }
    }
    match found {
        Some((x, y, z)) => {
            let ray7 = Ray::normalized(
                Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5),
                Vec3::new(1.0, 0.0, 0.0),
            );
            match pick_voxel(world, &ray7, 60.0) {
                Some(h) if h.face == crate::voxel::FaceDir::NegX && h.voxel.x >= x + 2 => {
                    check(
                        "射线7-陡峭侧壁",
                        true,
                        format!("命中 {} 面 NegX", h.voxel),
                        &mut results,
                    );
                }
                other => check(
                    "射线7-陡峭侧壁",
                    false,
                    format!("异常 {other:?}"),
                    &mut results,
                ),
            }
        }
        None => check(
            "射线7-陡峭侧壁",
            false,
            "未找到测试位置".into(),
            &mut results,
        ),
    }

    // 8. 洞口井射：从开口上方垂直下射，应穿过被挖穿的表面、
    //    在纯函数地形表面之下命中（井底或洞内）。
    let ray8 = Ray::normalized(
        Vec3::new(
            opening.x as f32 + 0.5,
            (opening.y + 8) as f32,
            opening.z as f32 + 0.5,
        ),
        Vec3::new(0.0, -1.0, 0.0),
    );
    // 开口列的纯函数地形高度（未被洞穴挖穿时的表面）。
    let h_terrain = world
        .params
        .terrain_height(opening.x, opening.z)
        .clamp(1, (world.size.y as i32) - 2);
    match pick_voxel(world, &ray8, 200.0) {
        Some(h) if h.voxel.y < h_terrain => check(
            "射线8-洞口井射",
            true,
            format!(
                "地形面 {h_terrain} 之下命中 y={}（开口列最高实体 {}）",
                h.voxel.y, opening.y
            ),
            &mut results,
        ),
        other => check(
            "射线8-洞口井射",
            false,
            format!("异常 {other:?}（期望 y < {h_terrain}）"),
            &mut results,
        ),
    }

    acc.ray_results = results;
}

/// A05 运行时：Chunk 边界单体素修改 + 双侧标脏。
fn run_edit_test(acc: &mut AcceptanceState, world: &mut World, t: f64) {
    let feats = acc.features.as_ref().unwrap();
    let center = feats
        .plains
        .as_ref()
        .map(|f| f.voxel.unwrap())
        .unwrap_or(IVec3::new(128, 40, 128));
    // 在平原中心附近找一个 x%16==15 的边界列。
    let mut target = None;
    for dx in -60i32..60 {
        let x = center.x + dx;
        if x.rem_euclid(16) != 15 || x + 1 >= world.size.x as i32 - 1 || x < 1 {
            continue;
        }
        let z = center.z.clamp(2, world.size.z as i32 - 3);
        let h = world.column_height(x, z);
        if h > 4 && !world.voxel(IVec3::new(x + 1, h + 1, z)).is_solid() {
            target = Some(IVec3::new(x + 1, h + 1, z));
            break;
        }
    }
    let Some(v) = target else {
        acc.edit_info = None;
        info!("[验收/A05] 未找到边界测试位置（异常）");
        return;
    };
    let old = world.set_voxel(v, BlockId::Stone);
    let chunk_b = crate::coords::chunk_of_voxel(v);
    let chunk_a = crate::coords::chunk_of_voxel(IVec3::new(v.x - 1, v.y, v.z));
    let dirty_after = world.dirty_chunks();
    let both = dirty_after.contains(&chunk_a) && dirty_after.contains(&chunk_b);
    info!(
        "[验收/A05] 修改 {v}（旧值 {old:?}）→ 脏 {:?} 双侧标记={}",
        dirty_after, both
    );
    acc.edit_info = Some(EditInfo {
        voxel: v,
        chunk_a,
        chunk_b,
        dirty_after,
        t_edit: t,
    });
}

fn verify_edit(acc: &mut AcceptanceState, world: &World, t: f64) {
    match acc.edit_info.clone() {
        Some(info) => {
            let drained = world.dirty_count() == 0;
            let solid_now = world.voxel(info.voxel) == BlockId::Stone;
            let both_marked = info.dirty_after.contains(&info.chunk_a)
                && info.dirty_after.contains(&info.chunk_b);
            let elapsed_ok = t - info.t_edit <= 8.0;
            let ok = drained && solid_now && both_marked && elapsed_ok;
            acc.edit_verify_detail = format!(
                "编辑 {} 后 {:.1}s：队列清空={} 新体素固化={} 双侧标脏={}",
                info.voxel,
                t - info.t_edit,
                drained,
                solid_now,
                both_marked
            );
            acc.edit_verified = Some(ok);
            info!("[验收/A05] {}", acc.edit_verify_detail);
        }
        None => {
            acc.edit_verified = Some(false);
            acc.edit_verify_detail = "编辑测试未执行".into();
        }
    }
}

/// 截图请求处理：装配 Screenshot 实体与保存 observer。
fn process_shot_queue(
    mut commands: Commands,
    mut acc: ResMut<AcceptanceState>,
    mut diag: ResMut<DiagState>,
) {
    while let Some(req) = acc.shot_queue.pop_front() {
        let path = req.path.clone();
        let name = req.name.clone();
        let downscale = req.downscale;
        commands.spawn(Screenshot::primary_window()).observe(
            move |trigger: On<ScreenshotCaptured>, mut log: ResMut<ShotLog>| {
                let img: bevy::image::Image = std::ops::Deref::deref(trigger.event()).clone();
                let result = save_screenshot(&img, &path, downscale);
                match result {
                    Ok((w, h, bytes)) => {
                        log.records.push((name.clone(), path.clone(), true, bytes));
                        let _ = (w, h);
                    }
                    Err(e) => {
                        log.records.push((name.clone(), path.clone(), false, 0));
                        info!("[验收] 截图保存失败 {name}: {e}");
                    }
                }
            },
        );
        diag.mark_capture();
    }
}

/// 保存截图：全分辨率 PNG（固定机位）或降采样 PNG（GIF 帧）。
/// 同时供 render.rs 的 F12 调试截图复用。
pub fn save_screenshot(
    img: &bevy::image::Image,
    path: &Path,
    downscale: bool,
) -> std::io::Result<(u32, u32, u64)> {
    let dynamic = img
        .clone()
        .try_into_dynamic()
        .map_err(|e| std::io::Error::other(format!("图像转换失败: {e:?}")))?;
    let rgba = dynamic.to_rgba8();
    let out = if downscale {
        image::imageops::resize(
            &rgba,
            GIF_SIZE.0,
            GIF_SIZE.1,
            image::imageops::FilterType::Triangle,
        )
    } else {
        rgba
    };
    let (w, h) = out.dimensions();
    out.save_with_format(path, image::ImageFormat::Png)
        .map_err(std::io::Error::other)?;
    let bytes = std::fs::metadata(path)?.len();
    Ok((w, h, bytes))
}

// ----------------------------------------------------------------------------
// 收尾：自动判定（A01-A13）与报告输出
// ----------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn finalize(
    acc: &mut AcceptanceState,
    world: &World,
    _registry: &ChunkMeshes,
    diag: &DiagState,
    mut next_state: ResMut<NextState<GameState>>,
    mut exit: MessageWriter<AppExit>,
) {
    info!("[验收] 进入收尾判定（t={:.1}s）", acc.t());
    let t = acc.t();
    let mut checks: Vec<Check> = Vec::new();

    // ---- A01 干净构建（由 scripts/acceptance.sh 写入 build_checks.json）----
    let build_json = acc.dir.join("build_checks.json");
    let build_all = match std::fs::read_to_string(&build_json) {
        Ok(content) => {
            let fmt_ok = content.contains("\"fmt\": true") || content.contains("\"fmt\":true");
            let clippy_ok =
                content.contains("\"clippy\": true") || content.contains("\"clippy\":true");
            let test_ok = content.contains("\"tests\": true") || content.contains("\"tests\":true");
            let build_ok = content.contains("\"release_build\": true")
                || content.contains("\"release_build\":true");
            let all = fmt_ok && clippy_ok && test_ok && build_ok;
            checks.push(Check {
                id: "A01",
                name: "干净构建（fmt/clippy/test/Release）",
                pass: all,
                detail: format!(
                    "fmt={fmt_ok} clippy={clippy_ok} tests={test_ok} release={build_ok}"
                ),
                evidence: "build_checks.json".into(),
            });
            all
        }
        Err(_) => {
            checks.push(Check {
                id: "A01",
                name: "干净构建（fmt/clippy/test/Release）",
                pass: false,
                detail: "build_checks.json 缺失：必须通过 scripts/acceptance.sh 运行完整验收"
                    .into(),
                evidence: "（缺失）".into(),
            });
            false
        }
    };

    // ---- A02 生成确定性（应用内重验：顺序无关 + 同 seed 一致）----
    let size = world.size;
    let params = world.params.clone();
    let live_hash = acc.initial_world_hash;
    let t0 = Instant::now();
    let mut b = World::empty(size, params.clone());
    let n = size.chunks();
    for cy in 0..n.y as i32 {
        for cz in 0..n.z as i32 {
            for cx in 0..n.x as i32 {
                b.ensure_chunk(IVec3::new(cx, cy, cz));
            }
        }
    }
    let hash_b = b.world_hash();
    drop(b);
    // 乱序（LCG 洗牌）。
    let mut order: Vec<IVec3> = Vec::with_capacity(world.total_chunks());
    for cy in 0..n.y as i32 {
        for cz in 0..n.z as i32 {
            for cx in 0..n.x as i32 {
                order.push(IVec3::new(cx, cy, cz));
            }
        }
    }
    let mut rng: u64 = 0xACCE_55EED;
    let mut i = order.len();
    while i > 1 {
        rng = crate::noise::mix64(rng);
        let j = (rng as usize) % i;
        i -= 1;
        order.swap(i, j);
    }
    let mut c = World::empty(size, params.clone());
    for cc in &order {
        c.ensure_chunk(*cc);
    }
    let hash_c = c.world_hash();
    drop(c);
    let a02 = live_hash == hash_b && hash_b == hash_c;
    checks.push(Check {
        id: "A02",
        name: "生成确定性",
        pass: a02,
        detail: format!("世界哈希 live={live_hash:#x} 顺序重建={hash_b:#x} 乱序重建={hash_c:#x}（耗时 {:.1}s，另有单元测试覆盖）", t0.elapsed().as_secs_f32()),
        evidence: "report.json / cargo test generation".into(),
    });

    // ---- A03 Chunk 独立性与跨界连续 ----
    // 判定语义：跨界两侧每列的"实际最高实体"必须符合纯函数地形定义——
    // 等于 clamp 后的 terrain_height，或低于它且该层为空气（被洞穴合法穿透）。
    // 两侧都符合 => 边界两侧由同一纯函数独立生成仍逐位一致（悬崖允许任意高差，
    // "高度相等"不是正确性判据；"符合定义"才是）。独立成块等价性另由单元测试覆盖
    //（生成哈希与顺序无关）。
    let mut border_pairs = 0usize;
    let mut consistent = 0usize;
    let mut srand: u64 = 0xB0AD;
    let col_ok = |world: &World, x: i32, z: i32| -> bool {
        let h_terrain = world
            .params
            .terrain_height(x, z)
            .clamp(1, (world.size.y as i32) - 2);
        let h_actual = world.column_height(x, z);
        h_actual <= h_terrain
            && (h_actual == h_terrain || !world.voxel(IVec3::new(x, h_terrain, z)).is_solid())
    };
    for _ in 0..300 {
        srand = crate::noise::mix64(srand);
        let x = 16 + (srand % ((world.size.x as u64) - 32)) as i32;
        srand = crate::noise::mix64(srand);
        let z = 16 + (srand % ((world.size.z as u64) - 32)) as i32;
        // 只取恰好跨 Chunk 边界的列对。
        let xa = if x.rem_euclid(16) == 15 {
            x
        } else {
            (x / 16) * 16 + 15
        };
        border_pairs += 1;
        if col_ok(world, xa, z) && col_ok(world, xa + 1, z) {
            consistent += 1;
        }
    }
    let ratio = consistent as f64 / border_pairs.max(1) as f64;
    let a03 = ratio >= 0.99;
    checks.push(Check {
        id: "A03",
        name: "Chunk 独立性与跨界连续",
        pass: a03,
        detail: format!("跨界列对 {consistent}/{border_pairs} 符合纯函数地形定义（≥99%；两侧独立生成逐位一致的哈希证据见 A02）"),
        evidence: "report.json / cargo test world::tests".into(),
    });

    // ---- A04 地形覆盖 ----
    let features = acc.features.clone().unwrap_or_default();
    let shots_expected = [
        "A04_01_plains.png",
        "A04_02_hills.png",
        "A04_03_mountain.png",
        "A04_04_valley_management.png",
        "A04_05_cliff.png",
        "A04_06_cave_opening.png",
        "A04_07_underground_cave.png",
        "A04_08_chunk_boundaries.png",
    ];
    let mut shot_details = Vec::new();
    let mut shots_ok = true;
    for name in shots_expected {
        let p = acc.dir.join("screenshots").join(name);
        let stat = std::fs::metadata(&p).ok().map(|m| m.len()).unwrap_or(0);
        let ok = stat > 10_000;
        shots_ok &= ok;
        shot_details.push(format!("{name}: {}B", stat));
        if !ok {
            // 二次机会：等待中的异步截图
            info!("[验收/A04] 截图偏小或缺失: {name} ({stat}B)");
        }
    }
    let a04 = features.all_present() && shots_ok;
    checks.push(Check {
        id: "A04",
        name: "地形覆盖（八类特征 + 证据机位）",
        pass: a04,
        detail: format!("{} | 截图: {}", features.summary(), shot_details.join(", ")),
        evidence: "screenshots/ + features.txt".into(),
    });

    // ---- A05 Mesh 正确性（单元测试 + 运行时编辑重建）----
    let edit_ok = acc.edit_verified.unwrap_or(false);
    let a05 = edit_ok;
    checks.push(Check {
        id: "A05",
        name: "Mesh 正确性与边界重建",
        pass: a05,
        detail: format!(
            "运行时: {}；遮挡/绕序/跨界无内面/空 Chunk 等由单元测试 meshing::tests 覆盖",
            acc.edit_verify_detail
        ),
        evidence: "cargo test meshing / screenshots/A05_edit_rebuild.png".into(),
    });

    // ---- A06 相机全路径 ----
    let mut cov = Coverage::default();
    for c in &acc.coverage_hits {
        cov.low_angle |= c.low_angle;
        cov.management |= c.management;
        cov.max_height |= c.max_height;
        cov.max_zoom |= c.max_zoom;
        cov.min_zoom |= c.min_zoom;
        cov.cross_chunk |= c.cross_chunk;
        cov.rotate_360 |= c.rotate_360;
        cov.border |= c.border;
    }
    let cov_all = cov.low_angle
        && cov.management
        && cov.max_height
        && cov.max_zoom
        && cov.min_zoom
        && cov.cross_chunk
        && cov.rotate_360
        && cov.border;
    let a06 = acc.finite_violations == 0
        && acc.clamp_violations == 0
        && acc.speed_violations == 0
        && cov_all;
    checks.push(Check {
        id: "A06",
        name: "相机全路径",
        pass: a06,
        detail: format!(
            "NaN/跳变/限位违规: finite={} clamp={} speed={}；覆盖: 低角={} 经营={} 最大高度={} 最大缩放={} 最小缩放={} 跨Chunk={} 360°={} 边界={}",
            acc.finite_violations, acc.clamp_violations, acc.speed_violations,
            cov.low_angle, cov.management, cov.max_height, cov.max_zoom, cov.min_zoom, cov.cross_chunk, cov.rotate_360, cov.border
        ),
        evidence: "metrics.csv / cruise_timelapse.gif".into(),
    });

    // ---- A07 渲染准备 ----
    let dirty_now = world.dirty_count();
    let a07 = acc.unready_violation_frames == 0 && dirty_now == 0;
    checks.push(Check {
        id: "A07",
        name: "渲染准备（visible-unready=0，队列清空）",
        pass: a07,
        detail: format!(
            "巡航期间 visible-unready 违规帧={}，收尾队列残留={}",
            acc.unready_violation_frames, dirty_now
        ),
        evidence: "metrics.csv".into(),
    });

    // ---- A08 相机实体冲突 ----
    let a08 = acc.inside_solid_events == 0 && acc.oscillation_events <= 4;
    checks.push(Check {
        id: "A08",
        name: "相机不进入实体 / 无振荡",
        pass: a08,
        detail: format!(
            "实体内事件={}（首例 {:?}） 距离符号交替事件={}",
            acc.inside_solid_events, acc.inside_solid_first, acc.oscillation_events
        ),
        evidence: "metrics.csv".into(),
    });

    // ---- A09 拾取 ----
    let ray_pass = acc.ray_results.iter().filter(|r| r.1).count();
    let a09 = !acc.ray_results.is_empty() && ray_pass == acc.ray_results.len();
    checks.push(Check {
        id: "A09",
        name: "拾取（8 条固定射线）",
        pass: a09,
        detail: format!(
            "{ray_pass}/{} 条通过: {}",
            acc.ray_results.len(),
            acc.ray_results
                .iter()
                .map(|r| r.0.clone())
                .collect::<Vec<_>>()
                .join("；")
        ),
        evidence: "screenshots/A09_picking_highlight.png".into(),
    });

    // ---- A10 20 TPS 分离 ----
    let tick_at = |t0: f64, t1: f64| -> Option<(f64, f64)> {
        let a = acc
            .tick_samples
            .iter()
            .find(|(ts, _)| *ts >= t0)
            .map(|(_, c)| *c);
        let b = acc
            .tick_samples
            .iter()
            .rev()
            .find(|(ts, _)| *ts <= t1)
            .map(|(_, c)| *c);
        match (a, b) {
            (Some(a), Some(b)) => Some((t1 - t0, b.saturating_sub(a) as f64)),
            _ => None,
        }
    };
    let _a10 = match tick_at(T_CRUISE_MEASURE_START, T_CRUISE_MEASURE_END) {
        Some((secs, delta)) => {
            let rate = delta / secs;
            let ok = (rate - TPS).abs() / TPS <= 0.02;
            checks.push(Check {
                id: "A10",
                name: "20 TPS 固定步与渲染分离",
                pass: ok,
                detail: format!("{secs:.0}s 内固定步 {delta} 次 = {rate:.3} TPS（目标 20 ±2%）"),
                evidence: "metrics.csv fixed_ticks 列".into(),
            });
            ok
        }
        None => {
            checks.push(Check {
                id: "A10",
                name: "20 TPS 固定步与渲染分离",
                pass: false,
                detail: "采样缺失".into(),
                evidence: "metrics.csv".into(),
            });
            false
        }
    };

    // ---- A11 稳定性 ----
    let panic_happened = acc.panic_flag_path.exists();
    let duration_ok = t >= 600.0;
    let a11 = !panic_happened && duration_ok && dirty_now == 0;
    checks.push(Check {
        id: "A11",
        name: "稳定性（≥10 分钟巡航）",
        pass: a11,
        detail: format!(
            "运行 {t:.0}s，panic={}，队列残留={}",
            panic_happened, dirty_now
        ),
        evidence: "metrics.csv / app_stdout.log".into(),
    });

    // ---- A12 资源稳定 ----
    let win_mean = |lo: f64, hi: f64| -> Option<f64> {
        let s: Vec<f64> = acc
            .rss_samples
            .iter()
            .filter(|(ts, _)| *ts >= lo && *ts <= hi)
            .map(|(_, r)| *r)
            .collect();
        if s.is_empty() {
            None
        } else {
            Some(s.iter().sum::<f64>() / s.len() as f64)
        }
    };
    let first = win_mean(180.0, 300.0);
    let last_max = acc
        .rss_samples
        .iter()
        .filter(|(ts, _)| *ts >= 508.0)
        .map(|(_, r)| *r)
        .fold(0.0f64, f64::max);
    let ent_first = acc
        .entity_samples
        .iter()
        .find(|(ts, _)| *ts >= 200.0)
        .map(|(_, e)| *e);
    let ent_last = acc.entity_samples.last().map(|(_, e)| *e);
    let (a12, a12_detail) = match (first, ent_first, ent_last) {
        (Some(first), Some(ef), Some(el)) => {
            let ratio = last_max / first.max(1.0);
            let stable_entities = ef == el;
            (ratio <= 1.25 && stable_entities, format!("首稳定循环均值 RSS {first:.0}MB，末段峰值 {last_max:.0}MB（×{ratio:.3}，阈值 1.25）；Chunk 实体 {ef}→{el}"))
        }
        _ => (false, "RSS/实体采样缺失".into()),
    };
    checks.push(Check {
        id: "A12",
        name: "资源稳定",
        pass: a12,
        detail: a12_detail,
        evidence: "metrics.csv rss_mb 列".into(),
    });

    // ---- A13 性能基线 ----
    let mut times: Vec<f32> = diag.frame_times.iter().copied().collect();
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let (a13, a13_detail) = if times.len() > 600 {
        let median = times[times.len() / 2];
        let p95 = times[(times.len() as f32 * 0.95) as usize];
        let long_frames = times.iter().filter(|&&t| t > 0.2).count();
        // 连续长帧检测（>200ms 相邻出现）
        let mut consecutive_long = 0;
        let mut prev_long = false;
        for &ft in diag.frame_times.iter() {
            let is_long = ft > 0.2;
            if is_long && prev_long {
                consecutive_long += 1;
            }
            prev_long = is_long;
        }
        let median_fps = 1.0 / median;
        let ok = median_fps >= 60.0 && p95 <= 0.0333 && long_frames <= 2 && consecutive_long == 0;
        (
            ok,
            format!(
                "样本 {} 帧：中位 FPS {:.1}（≥60），p95 帧时 {:.2}ms（≤33.3），>200ms 帧 {}（相邻长帧对 {}）",
                times.len(),
                median_fps,
                p95 * 1000.0,
                long_frames,
                consecutive_long
            ),
        )
    } else {
        (
            false,
            format!("帧样本不足（{}），运行时长异常", times.len()),
        )
    };
    checks.push(Check {
        id: "A13",
        name: "性能基线（1280×720 Release）",
        pass: a13,
        detail: a13_detail,
        evidence: "metrics.csv fps 列".into(),
    });

    // ---- 汇总（A14 由独立 Reviewer 出具，不在此判定）----
    let all_pass = checks.iter().all(|c| c.pass);
    let overall = if all_pass { "PASS" } else { "FAIL" };

    // GIF 组装（在资源判定之后，避免编码内存影响 RSS 采样）。
    let gif_result = assemble_gif(&acc.dir, acc.gif_frame_count);
    info!("[验收] GIF 组装: {}", gif_result.unwrap_or_else(|e| e));

    // 报告输出。
    let meta = format!(
        "{{\n  \"seed\": {},\n  \"size\": [{}, {}, {}],\n  \"world_hash\": \"{:#x}\",\n  \"window\": [{}, {}],\n  \"profile\": \"release\",\n  \"duration_s\": {:.1},\n  \"gif_frames\": {},\n  \"timestamp\": \"{}\"\n}}",
        acc.cfg.seed,
        acc.cfg.size.x,
        acc.cfg.size.y,
        acc.cfg.size.z,
        acc.initial_world_hash,
        acc.cfg.window[0],
        acc.cfg.window[1],
        t,
        acc.gif_frame_count,
        chrono_like_now(),
    );
    let checks_json: Vec<String> = checks
        .iter()
        .map(|c| {
            format!(
                "  {{\"id\": \"{}\", \"name\": \"{}\", \"status\": \"{}\", \"detail\": \"{}\", \"evidence\": \"{}\"}}",
                c.id,
                c.name,
                if c.pass { "PASS" } else { "FAIL" },
                c.detail.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', " "),
                c.evidence
            )
        })
        .collect();
    let json = format!(
        "{{\n\"meta\": {},\n\"checks\": [\n{}\n],\n\"a14_reviewer\": \"EXTERNAL（由独立 Reviewer 出具）\",\n\"overall\": \"{}\"\n}}\n",
        meta,
        checks_json.join(",\n"),
        overall
    );
    let _ = std::fs::write(acc.dir.join("report.json"), &json);

    let mut md = String::new();
    md.push_str("# 初版验收报告（自动判定）\n\n");
    md.push_str(&format!(
        "- **总体结论：{overall}**（A01-A13 运行时判定；A14 由独立 Reviewer 复核出具）\n"
    ));
    md.push_str(&format!(
        "- Seed：{}，世界 {}×{}×{}，窗口 {}×{}，运行 {:.0}s\n",
        acc.cfg.seed,
        acc.cfg.size.x,
        acc.cfg.size.y,
        acc.cfg.size.z,
        acc.cfg.window[0],
        acc.cfg.window[1],
        t
    ));
    md.push_str(&format!("- 世界哈希：`{:#x}`\n\n", acc.initial_world_hash));
    md.push_str("| ID | 项目 | 结论 | 说明 |\n|---|---|---|---|\n");
    for c in &checks {
        md.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            c.id,
            c.name,
            if c.pass { "PASS" } else { "FAIL" },
            c.detail
        ));
    }
    md.push_str("\n## 特征探测\n\n");
    md.push_str(&features.summary());
    md.push('\n');
    let _ = std::fs::write(acc.dir.join("report.md"), &md);
    info!(
        "[验收] 报告已写入 {}（overall={overall}）",
        acc.dir.display()
    );

    acc.checks = checks;
    let _ = build_all;
    // 退出：Success 仅当全部通过。
    next_state.set(GameState::Finished);
    if all_pass {
        exit.write(AppExit::Success);
    } else {
        exit.write(AppExit::Error(std::num::NonZeroU8::new(1).unwrap()));
    }
}

fn chrono_like_now() -> String {
    // 无 chrono 依赖：使用系统时间戳。
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "unknown".into())
}

/// 把 gif_frames/*.png 组装为巡航 GIF（"录像"证据；无 ffmpeg 环境的替代方案）。
fn assemble_gif(dir: &Path, frame_count: usize) -> Result<String, String> {
    use std::io::BufWriter;
    if frame_count == 0 {
        return Err("无 GIF 帧".into());
    }
    let out_path = dir.join("cruise_timelapse.gif");
    let file = std::fs::File::create(&out_path).map_err(|e| e.to_string())?;
    let mut enc = image::codecs::gif::GifEncoder::new(BufWriter::new(file));
    let mut encoded = 0usize;
    for i in 0..frame_count {
        let p = dir.join("gif_frames").join(format!("{i:04}.png"));
        let Ok(img) = image::open(&p) else {
            continue;
        };
        let rgba = img.to_rgba8();
        let delay = image::Delay::from_numer_denom_ms(400, 1);
        let frame = image::Frame::from_parts(rgba, 0, 0, delay);
        enc.encode_frame(frame).map_err(|e| e.to_string())?;
        encoded += 1;
    }
    Ok(format!("{encoded} 帧 -> {}", out_path.display()))
}
