//! 应用组装：插件、状态、窗口与系统接线。

use crate::acceptance::AcceptancePlugin;
use crate::app_state::GameState;
use crate::camera;
use crate::config::AppConfig;
use crate::coords::WorldSize;
use crate::diagnostics::{self, DiagHudText};
use crate::generation::TerrainParams;
use crate::meshing::DebugTint;
use crate::picking::{picking_highlight_system, CurrentPickRes, ScriptedRayRes};
use crate::render::{self, LoadingText};
use crate::world::World;
use bevy::prelude::*;
use bevy::text::FontSize;
use bevy::time::Fixed;
use bevy::window::{PresentMode, WindowResolution};

pub fn run() -> AppExit {
    let cfg = AppConfig::parse(std::env::args().skip(1)).unwrap_or_else(|e| {
        if !e.contains("--help") {
            eprintln!("参数错误: {e}");
        }
        std::process::exit(2);
    });
    let acceptance = cfg.acceptance;
    let mut app = App::new();

    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "工匠要塞 · 初版验证构建".into(),
                    // 0.19 起 WindowResolution::new 接受物理像素（u32）；启动时物理=逻辑。
                    resolution: WindowResolution::new(cfg.window[0] as u32, cfg.window[1] as u32)
                        .with_scale_factor_override(1.0),
                    present_mode: PresentMode::Fifo,
                    ..Default::default()
                }),
                ..Default::default()
            })
            .set(bevy::log::LogPlugin {
                level: bevy::log::Level::INFO,
                filter: "info,wgpu=error,naga=warn".into(),
                ..Default::default()
            }),
    );

    // 固定 20 TPS，与渲染帧率分离（固定时间基线）。
    app.insert_resource(Time::<Fixed>::from_hz(diagnostics::TPS));
    app.insert_resource(ClearColor(crate::render::sky_color()));
    app.insert_resource(UiScale(0.85));

    app.init_state::<GameState>();

    // 全局资源：配置、世界（空壳，Loading 阶段填充）、拾取。
    app.insert_resource(crate::camera::WorldRes(Some(World::empty(
        WorldSize::new(cfg.size.x, cfg.size.y, cfg.size.z),
        TerrainParams::new(cfg.seed),
    ))));
    app.insert_resource(CurrentPickRes(None));
    app.insert_resource(ScriptedRayRes(None));
    if cfg.tint_debug {
        app.insert_resource(crate::render::DebugTintRes(DebugTint::ChunkParity));
    }

    // 子模块插件。
    render::plugin(&mut app);
    camera::plugin(&mut app);
    diagnostics::plugin(&mut app);

    app.add_systems(Startup, spawn_ui_text);
    app.add_systems(OnEnter(GameState::Ready), hide_loading_text);
    app.add_systems(
        Update,
        picking_highlight_system.run_if(in_state(GameState::Ready)),
    );

    if acceptance {
        app.add_plugins(AcceptancePlugin::new(cfg.clone()));
    }

    info!(
        "启动：seed={} 尺寸={}×{}×{} 窗口={}×{} 验收模式={}",
        cfg.seed, cfg.size.x, cfg.size.y, cfg.size.z, cfg.window[0], cfg.window[1], acceptance
    );
    app.run()
}

/// 生成 UI：加载进度 + 调试 HUD + 帮助。
/// 应用内文本一律 ASCII：Bevy 默认字体无 CJK 字形（渲染为方块），
/// 中文 UI 待正式字体管线（决策记录"文本/字体"）。
fn spawn_ui_text(mut commands: Commands) {
    commands.spawn((
        Text::new("Initializing..."),
        TextFont {
            font_size: FontSize::Px(26.0),
            ..Default::default()
        },
        TextColor(Color::srgb(0.95, 0.96, 1.0)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Percent(40.0),
            left: Val::Percent(28.0),
            ..Default::default()
        },
        LoadingText,
    ));
    commands.spawn((
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(14.0),
            ..Default::default()
        },
        TextColor(Color::srgb(0.97, 0.98, 1.0)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            left: Val::Px(10.0),
            ..Default::default()
        },
        DiagHudText,
    ));
    commands.spawn((
        Text::new(
            "WASD move · R/F up/down · Q/E or LMB-drag rotate · RMB-drag pan · Wheel zoom · Shift fast · F3 chunk tint · F12 screenshot",
        ),
        TextFont {
            font_size: FontSize::Px(13.0),
            ..Default::default()
        },
        TextColor(Color::srgb(0.85, 0.9, 0.95)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(8.0),
            left: Val::Px(10.0),
            ..Default::default()
        },
    ));
}

/// Ready 进入时隐藏加载文本。
fn hide_loading_text(mut commands: Commands, query: Query<Entity, With<LoadingText>>) {
    if let Ok(entity) = query.single() {
        commands.entity(entity).despawn();
    }
}
