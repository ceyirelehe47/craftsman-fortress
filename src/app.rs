//! 应用组装：插件、状态、窗口与系统接线。

use crate::acceptance::AcceptancePlugin;
use crate::app_state::GameState;
use crate::camera;
use crate::config::AppConfig;
use crate::coords::WorldSize;
use crate::diagnostics::{self, DiagHudText};
use crate::generation::TerrainParams;
use crate::meshing::DebugTint;
use crate::picking::{picking_highlight_system, CurrentPickRes, CurrentRayRes, ScriptedRayRes};
use crate::render::{self, LoadingText};
use crate::world::World;
use bevy::prelude::*;
use bevy::text::FontSize;
use bevy::time::Fixed;
use bevy::window::{PresentMode, WindowResolution};
use std::path::Path;

pub fn run() -> AppExit {
    let mut cfg = AppConfig::parse(std::env::args().skip(1)).unwrap_or_else(|e| {
        if !e.contains("--help") {
            eprintln!("参数错误: {e}");
        }
        std::process::exit(2);
    });
    let acceptance = cfg.acceptance;

    let mut loaded_generation = None;
    let world = if let Some(load_path) = cfg.load_path.clone() {
        match crate::persistence::load_latest(Path::new(&load_path)) {
            Ok(loaded) => {
                cfg.seed = loaded.world.params.seed;
                cfg.size = loaded.world.size;
                loaded_generation = Some(loaded.meta.generation);
                info!(
                    "加载存档：{} generation={} edits={} hash={:#x}",
                    loaded.slot_path.display(),
                    loaded.meta.generation,
                    loaded.meta.edit_count,
                    loaded.meta.world_semantic_hash
                );
                loaded.world
            }
            Err(e) => {
                eprintln!("存档加载失败（{load_path}）：{e}");
                std::process::exit(3);
            }
        }
    } else {
        World::empty(
            WorldSize::new(cfg.size.x, cfg.size.y, cfg.size.z),
            TerrainParams::new(cfg.seed),
        )
    };

    let object_save_path =
        crate::object_persistence::object_logical_path(Path::new(&cfg.save_path));
    let (object_store, loaded_object_generation) = if let Some(load_path) = cfg.load_path.as_ref() {
        let object_load_path = crate::object_persistence::object_logical_path(Path::new(load_path));
        match crate::object_persistence::load_latest(&object_load_path, &world) {
            Ok(Some(loaded)) => {
                info!(
                    "加载对象存档：{} generation={} objects={} hash={:#x}",
                    loaded.slot_path.display(),
                    loaded.meta.generation,
                    loaded.meta.object_count,
                    loaded.meta.object_semantic_hash
                );
                (loaded.store, Some(loaded.meta.generation))
            }
            Ok(None) => (crate::objects::ObjectStore::new(), None),
            Err(error) => {
                eprintln!(
                    "对象存档加载失败（{}）：{error}",
                    object_load_path.display()
                );
                std::process::exit(4);
            }
        }
    } else {
        (crate::objects::ObjectStore::new(), None)
    };

    let building_save_path =
        crate::building_persistence::building_logical_path(Path::new(&cfg.save_path));
    let (building_store, loaded_building_generation) = if let Some(load_path) =
        cfg.load_path.as_ref()
    {
        let building_load_path =
            crate::building_persistence::building_logical_path(Path::new(load_path));
        match crate::building_persistence::load_latest(&building_load_path, &world, &object_store) {
            Ok(Some(loaded)) => {
                info!(
                    "加载建筑存档：{} generation={} components={} hash={:#x}",
                    loaded.slot_path.display(),
                    loaded.meta.generation,
                    loaded.meta.component_count,
                    loaded.meta.building_semantic_hash
                );
                (loaded.store, Some(loaded.meta.generation))
            }
            Ok(None) => (crate::buildings::BuildingStore::new(), None),
            Err(error) => {
                eprintln!(
                    "建筑存档加载失败（{}）：{error}",
                    building_load_path.display()
                );
                std::process::exit(5);
            }
        }
    } else {
        (crate::buildings::BuildingStore::new(), None)
    };

    let asset_root = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("assets");
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(bevy::asset::AssetPlugin {
                file_path: asset_root.to_string_lossy().to_string(),
                ..Default::default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "工匠要塞 · 初版验证构建".into(),
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

    app.insert_resource(Time::<Fixed>::from_hz(diagnostics::TPS));
    app.insert_resource(ClearColor(crate::render::sky_color()));
    app.insert_resource(UiScale(0.85));
    app.init_state::<GameState>();

    app.insert_resource(crate::camera::WorldRes(Some(world)));
    app.insert_resource(CurrentPickRes(None));
    app.insert_resource(CurrentRayRes(None));
    app.insert_resource(ScriptedRayRes(None));
    if cfg.tint_debug {
        app.insert_resource(crate::render::DebugTintRes(DebugTint::ChunkParity));
    }

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
    } else {
        let manifest_path = std::env::var("R3_MANIFEST")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                std::path::PathBuf::from(crate::object_runtime::DEFAULT_OBJECT_MANIFEST)
            });
        crate::object_runtime::plugin(
            &mut app,
            object_store,
            loaded_object_generation,
            manifest_path,
        );
        crate::building_runtime::plugin(&mut app, building_store, loaded_building_generation);
        crate::editing::plugin(
            &mut app,
            std::path::PathBuf::from(&cfg.save_path),
            object_save_path.clone(),
            building_save_path.clone(),
            loaded_generation,
        );
    }

    info!(
        "启动：seed={} 尺寸={}×{}×{} 窗口={}×{} 验收模式={} 存档路径={} 对象路径={} 建筑路径={} 加载={:?}",
        cfg.seed,
        cfg.size.x,
        cfg.size.y,
        cfg.size.z,
        cfg.window[0],
        cfg.window[1],
        acceptance,
        cfg.save_path,
        object_save_path.display(),
        building_save_path.display(),
        cfg.load_path
    );
    app.run()
}

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
            "WASD move · R/F up/down · Q/E or LMB-drag rotate · RMB-drag pan · Wheel zoom · Delete/B terrain · 1/2/3 object · G/P/O/M/X object · F1-F4/F6-F9 building · H/J/K/L/Backspace building · PageUp/PageDown level · F5 save",
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

fn hide_loading_text(mut commands: Commands, query: Query<Entity, With<LoadingText>>) {
    if let Ok(entity) = query.single() {
        commands.entity(entity).despawn();
    }
}
