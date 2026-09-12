//! 调度、诊断与可观测性（任务书 4.7）。
//!
//! - `FixedUpdate` 计数器验证 20 TPS 与渲染帧率分离（A10）；
//! - 屏幕调试面板报告 Seed、相机/焦点、Chunk 计数、队列、FPS 与帧时间；
//! - 结构化 CSV 指标（验收模式），机器可读；
//! - "可见但未准备 Chunk 数"关键诊断（A07，正式巡航恒为 0）；
//! - 进程 RSS 采样（A12）。

use crate::app_state::GameState;
use crate::camera::{CameraRigRes, WorldRes};
use crate::picking::CurrentPickRes;
use crate::render::ChunkMeshes;
use bevy::prelude::*;
use std::collections::VecDeque;
use std::io::Write;
use std::time::Instant;

/// 调试 HUD 文本标记。
#[derive(Component)]
pub struct DiagHudText;

/// 诊断状态。
#[derive(Resource, Default)]
pub struct DiagState {
    /// FixedUpdate 累计计数。
    pub fixed_ticks: u64,
    /// 是否统计帧时间（验收热身后开启）。
    pub collecting: bool,
    /// 帧时间环形缓冲（秒，仅 collecting 期间）。
    pub frame_times: VecDeque<f32>,
    /// FPS 指数平均。
    pub fps_ema: f32,
    /// 截图帧排除窗口（避免把读回开销计入性能统计）。
    pub exclude_until: Option<Instant>,
    /// CSV 指标文件。
    csv: Option<std::fs::File>,
    last_csv: Option<Instant>,
    last_hud: Option<Instant>,
    last_rss: Option<Instant>,
    pub rss_mb: f64,
    pub visible_unready: usize,
    sys: Option<sysinfo::System>,
    /// 运行期错误/警告计数（日志机器信号）。
    pub error_events: u64,
}

impl DiagState {
    /// 开启 CSV 输出。
    pub fn open_csv(&mut self, path: &std::path::Path) -> std::io::Result<()> {
        let mut f = std::fs::File::create(path)?;
        writeln!(f, "t_s,fps,frame_ms_ema,fixed_ticks,meshed,dirty,visible_unready,rss_mb,cam_x,cam_y,cam_z,focus_x,focus_y,focus_z")?;
        self.csv = Some(f);
        Ok(())
    }

    pub fn mark_capture(&mut self) {
        self.exclude_until = Some(Instant::now() + std::time::Duration::from_millis(250));
    }

    fn excluded(&self) -> bool {
        self.exclude_until.map(|t| Instant::now() < t).unwrap_or(false)
    }
}

/// TPS 计数（FixedUpdate，与渲染帧率分离的证据）。
fn fixed_tick_counter(mut diag: ResMut<DiagState>, time: Res<Time<bevy::time::Fixed>>) {
    let _ = time;
    diag.fixed_ticks += 1;
}

/// 帧收集 + HUD + CSV + RSS 采样。
#[allow(clippy::too_many_arguments)]
fn diagnostics_update(
    mut diag: ResMut<DiagState>,
    time: Res<Time>,
    world_res: Res<WorldRes>,
    rig: Res<CameraRigRes>,
    registry: Res<ChunkMeshes>,
    pick: Res<CurrentPickRes>,
    mut hud: Query<&mut Text, With<DiagHudText>>,
) {
    let dt = time.delta_secs();
    if dt > 0.0 {
        let fps = 1.0 / dt;
        diag.fps_ema = if diag.fps_ema == 0.0 { fps } else { diag.fps_ema * 0.9 + fps * 0.1 };
    }
    if diag.collecting && !diag.excluded() && dt > 0.0 {
        diag.frame_times.push_back(dt);
        if diag.frame_times.len() > 200_000 {
            diag.frame_times.pop_front();
        }
    }

    let now = Instant::now();
    // RSS 采样（5s 周期）。
    if diag.last_rss.map(|t| now.duration_since(t).as_secs_f64() >= 5.0).unwrap_or(true) {
        diag.last_rss = Some(now);
        sample_rss(&mut diag);
    }

    let Some(world) = world_res.0.as_ref() else {
        return;
    };

    // 可见未准备 Chunk：观察中心半径 RENDER_RADIUS 内未 Mesh 的 Chunk 数（应恒为 0）。
    if diag.last_csv.map(|t| now.duration_since(t).as_secs_f32() >= 1.0).unwrap_or(true) {
        diag.visible_unready = count_visible_unready(world, &registry, rig.0.focus, RENDER_RADIUS);
    }

    // HUD 4Hz。
    if diag.last_hud.map(|t| now.duration_since(t).as_secs_f32() >= 0.25).unwrap_or(true) {
        diag.last_hud = Some(now);
        let rig = &rig.0;
        let pick_s = pick
            .0
            .as_ref()
            .map(|h| format!("({},{},{}) 面 {:?}", h.voxel.x, h.voxel.y, h.voxel.z, h.face))
            .unwrap_or_else(|| "无".into());
        let text = format!(
            "Seed {} · 世界 {}×{}×{} · Chunk 16³ · TPS {:.1} (#{})\n\
             FPS {:.1} · 帧 {:.1}ms · RSS {:.0}MB\n\
             相机 ({:.1},{:.1},{:.1}) 焦点 ({:.1},{:.1},{:.1}) 距离 {:.1}m 俯仰 {:.0}°\n\
             Chunk 已生成 {}/{} · 已Mesh {} · 脏 {} · 可见未准备 {}\n\
             拾取: {}",
            world.params.seed,
            world.size.x,
            world.size.y,
            world.size.z,
            TPS,
            diag.fixed_ticks,
            diag.fps_ema,
            1000.0 / diag.fps_ema.max(1e-6),
            diag.rss_mb,
            rig.eye().x,
            rig.eye().y,
            rig.eye().z,
            rig.focus.x,
            rig.focus.y,
            rig.focus.z,
            rig.dist,
            rig.pitch.to_degrees(),
            world.generated_count(),
            world.total_chunks(),
            registry.meshed_count(),
            world.dirty_count(),
            diag.visible_unready,
            pick_s
        );
        if let Ok(mut hud_text) = hud.single_mut() {
            **hud_text = text;
        }
    }

    // CSV 1Hz。
    if diag.last_csv.map(|t| now.duration_since(t).as_secs_f32() >= 1.0).unwrap_or(true) {
        diag.last_csv = Some(now);
        let mut csv_line = None;
        if diag.csv.is_some() {
            let rig = &rig.0;
            let eye = rig.eye();
            csv_line = Some(format!(
                "{:.2},{:.2},{:.2},{},{},{},{},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1}",
                time.elapsed_secs(),
                diag.fps_ema,
                1000.0 / diag.fps_ema.max(1e-6),
                diag.fixed_ticks,
                registry.meshed_count(),
                world.dirty_count(),
                diag.visible_unready,
                diag.rss_mb,
                eye.x,
                eye.y,
                eye.z,
                rig.focus.x,
                rig.focus.y,
                rig.focus.z
            ));
        }
        if let (Some(csv), Some(line)) = (diag.csv.as_mut(), csv_line) {
            let _ = writeln!(csv, "{line}");
        }
    }
}

/// 目标模拟频率（任务书：固定 20 TPS）。
pub const TPS: f64 = 20.0;
/// 观察中心周围的渲染准备半径（米）。
pub const RENDER_RADIUS: f32 = 190.0;

fn count_visible_unready(
    world: &crate::world::World,
    registry: &ChunkMeshes,
    focus: Vec3,
    radius: f32,
) -> usize {
    let n = world.size.chunks();
    let mut unready = 0usize;
    let fx = focus.x / 16.0;
    let fz = focus.z / 16.0;
    let r_chunks = radius / 16.0;
    for cy in 0..n.y as i32 {
        for cz in 0..n.z as i32 {
            for cx in 0..n.x as i32 {
                let dx = cx as f32 + 0.5 - fx;
                let dz = cz as f32 + 0.5 - fz;
                if dx * dx + dz * dz > r_chunks * r_chunks {
                    continue;
                }
                if !registry.entries.contains_key(&bevy::math::IVec3::new(cx, cy, cz)) {
                    unready += 1;
                }
            }
        }
    }
    unready
}

fn sample_rss(diag: &mut DiagState) {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    if diag.sys.is_none() {
        diag.sys = Some(System::new());
    }
    let Some(sys) = diag.sys.as_mut() else { return };
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::everything());
    if let Some(proc) = sys.process(Pid::from_u32(std::process::id())) {
        diag.rss_mb = proc.memory() as f64 / (1024.0 * 1024.0);
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<DiagState>();
    app.add_systems(bevy::app::FixedUpdate, fixed_tick_counter);
    app.add_systems(OnEnter(GameState::Ready), on_ready_log);
    app.add_systems(
        Update,
        diagnostics_update.run_if(in_state(GameState::Ready)),
    );
}

fn on_ready_log(world_res: Res<WorldRes>, diag: Res<DiagState>) {
    let Some(world) = world_res.0.as_ref() else {
        return;
    };
    info!(
        "进入 Ready：seed={} 尺寸 {}×{}×{} chunks={} 初始 ticks={}",
        world.params.seed,
        world.size.x,
        world.size.y,
        world.size.z,
        world.total_chunks(),
        diag.fixed_ticks
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tps_constant_is_20() {
        assert!((TPS - 20.0).abs() < 1e-9);
    }
}
