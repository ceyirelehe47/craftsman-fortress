use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::render::view::window::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::window::{PresentMode, WindowResolution};
use craftsman_fortress::acceptance::save_screenshot;
use craftsman_fortress::buildings::{BuildingKind, BuildingStore, PlacedBuilding, STORY_HEIGHT};
use craftsman_fortress::coords::WorldSize;
use craftsman_fortress::generation::TerrainParams;
use craftsman_fortress::objects::ObjectStore;
use craftsman_fortress::voxel::BlockId;
use craftsman_fortress::world::World;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Instant;

const WINDOW: [u32; 2] = [1280, 720];
const SHOTS: [&str; 4] = [
    "R5_01_component_catalog.png",
    "R5_02_wall_door_window.png",
    "R5_03_stair_and_upper_floor.png",
    "R5_04_structural_frame.png",
];

#[derive(Resource)]
struct LabState {
    evidence: PathBuf,
    ready_at: Instant,
    shot_index: usize,
    queue: VecDeque<(String, PathBuf)>,
    store_hash: u64,
    component_count: usize,
    finalized: bool,
}

#[derive(Resource, Default)]
struct ShotLog(Vec<(String, bool, u64)>);

struct LabAssets {
    cube: Handle<Mesh>,
    wood: Handle<StandardMaterial>,
    accent: Handle<StandardMaterial>,
    glass: Handle<StandardMaterial>,
    roof: Handle<StandardMaterial>,
}

#[derive(Resource)]
struct LabRecords(Vec<PlacedBuilding>);

#[derive(Component)]
struct LabCamera;

fn main() -> AppExit {
    let evidence = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("evidence_r5_visual"));
    if evidence.exists()
        && evidence
            .read_dir()
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false)
    {
        eprintln!(
            "R5 evidence directory must be empty: {}",
            evidence.display()
        );
        std::process::exit(2);
    }
    std::fs::create_dir_all(evidence.join("screenshots")).unwrap();

    let world = flat_world();
    let objects = ObjectStore::new();
    let store = visual_catalog(&world, &objects).unwrap_or_else(|error| {
        eprintln!("R5 visual catalog error: {error}");
        std::process::exit(2);
    });
    let store_hash = store.semantic_hash();
    let component_count = store.len();
    let records = store.records_sorted();

    let asset_root = std::env::current_dir().unwrap().join("assets");
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(bevy::asset::AssetPlugin {
                file_path: asset_root.to_string_lossy().to_string(),
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "工匠要塞 · R5 Building Lab".into(),
                    resolution: WindowResolution::new(WINDOW[0], WINDOW[1])
                        .with_scale_factor_override(1.0),
                    present_mode: PresentMode::Fifo,
                    ..default()
                }),
                ..default()
            })
            .set(bevy::log::LogPlugin {
                level: bevy::log::Level::INFO,
                filter: "info,wgpu=error,naga=warn".into(),
                ..default()
            }),
    );
    app.insert_resource(ClearColor(Color::srgb(0.52, 0.68, 0.82)));
    app.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: 700.0,
        ..default()
    });
    app.insert_resource(ShotLog::default());
    app.insert_resource(LabRecords(records));
    app.insert_resource(LabState {
        evidence,
        ready_at: Instant::now(),
        shot_index: 0,
        queue: VecDeque::new(),
        store_hash,
        component_count,
        finalized: false,
    });
    app.add_systems(Startup, setup);
    app.add_systems(Update, (drive_shots, process_shots, finalize).chain());
    app.run()
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    records: Res<LabRecords>,
) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(22.0, 14.0, 28.0).looking_at(Vec3::new(9.0, 6.0, 5.0), Vec3::Y),
        LabCamera,
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 10_000.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.9, -0.7, 0.0)),
    ));

    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let ground_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.34, 0.47, 0.29),
        perceptual_roughness: 1.0,
        ..default()
    });
    let wood = materials.add(StandardMaterial {
        base_color: Color::srgb(0.54, 0.34, 0.18),
        perceptual_roughness: 0.9,
        ..default()
    });
    let accent = materials.add(StandardMaterial {
        base_color: Color::srgb(0.73, 0.55, 0.31),
        perceptual_roughness: 0.85,
        ..default()
    });
    let glass = materials.add(StandardMaterial {
        base_color: Color::srgba(0.42, 0.74, 0.92, 0.38),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.1,
        ..default()
    });
    let roof = materials.add(StandardMaterial {
        base_color: Color::srgb(0.31, 0.22, 0.20),
        perceptual_roughness: 0.95,
        ..default()
    });
    let assets = LabAssets {
        cube: cube.clone(),
        wood,
        accent,
        glass,
        roof,
    };
    commands.spawn((
        Mesh3d(cube),
        MeshMaterial3d(ground_material),
        Transform::from_xyz(9.0, 4.8, 5.0).with_scale(Vec3::new(26.0, 0.2, 14.0)),
    ));

    for record in records.0.iter().copied() {
        let rotation = match record.kind {
            BuildingKind::FloorBasic | BuildingKind::RoofFlatBasic | BuildingKind::ColumnBasic => {
                Quat::IDENTITY
            }
            _ => Quat::from_rotation_y(-(record.yaw_quarters as f32) * std::f32::consts::FRAC_PI_2),
        };
        let root = commands
            .spawn((
                Transform::from_translation(record.anchor.as_vec3()).with_rotation(rotation),
                Visibility::Inherited,
                Name::new(format!("R5:{}", record.kind.type_id())),
            ))
            .id();
        spawn_component(&mut commands, root, record.kind, &assets);
    }
}

fn spawn_component(commands: &mut Commands, root: Entity, kind: BuildingKind, assets: &LabAssets) {
    match kind {
        BuildingKind::FloorBasic => spawn_box(
            commands,
            root,
            assets,
            assets.wood.clone(),
            Vec3::new(0.5, -0.06, 0.5),
            Vec3::new(1.0, 0.12, 1.0),
        ),
        BuildingKind::RoofFlatBasic => spawn_box(
            commands,
            root,
            assets,
            assets.roof.clone(),
            Vec3::new(0.5, -0.09, 0.5),
            Vec3::new(1.0, 0.18, 1.0),
        ),
        BuildingKind::WallBasic => spawn_box(
            commands,
            root,
            assets,
            assets.wood.clone(),
            Vec3::new(0.5, 1.5, 0.0),
            Vec3::new(1.0, 3.0, 0.12),
        ),
        BuildingKind::DoorwayBasic => {
            for (center, size) in [
                (Vec3::new(0.08, 1.5, 0.0), Vec3::new(0.16, 3.0, 0.14)),
                (Vec3::new(0.92, 1.5, 0.0), Vec3::new(0.16, 3.0, 0.14)),
                (Vec3::new(0.5, 2.82, 0.0), Vec3::new(0.84, 0.36, 0.14)),
            ] {
                spawn_box(commands, root, assets, assets.accent.clone(), center, size);
            }
        }
        BuildingKind::WindowBasic => {
            for (center, size) in [
                (Vec3::new(0.08, 1.5, 0.0), Vec3::new(0.16, 3.0, 0.14)),
                (Vec3::new(0.92, 1.5, 0.0), Vec3::new(0.16, 3.0, 0.14)),
                (Vec3::new(0.5, 0.48, 0.0), Vec3::new(0.84, 0.24, 0.14)),
                (Vec3::new(0.5, 2.52, 0.0), Vec3::new(0.84, 0.24, 0.14)),
            ] {
                spawn_box(commands, root, assets, assets.accent.clone(), center, size);
            }
            spawn_box(
                commands,
                root,
                assets,
                assets.glass.clone(),
                Vec3::new(0.5, 1.5, 0.0),
                Vec3::new(0.68, 1.68, 0.04),
            );
        }
        BuildingKind::ColumnBasic => spawn_box(
            commands,
            root,
            assets,
            assets.accent.clone(),
            Vec3::new(0.0, 1.5, 0.0),
            Vec3::new(0.18, 3.0, 0.18),
        ),
        BuildingKind::BeamBasic => spawn_box(
            commands,
            root,
            assets,
            assets.accent.clone(),
            Vec3::new(0.5, -0.09, 0.0),
            Vec3::new(1.0, 0.18, 0.18),
        ),
        BuildingKind::StairBasic => {
            for step in 0..STORY_HEIGHT {
                let height = step as f32 + 1.0;
                spawn_box(
                    commands,
                    root,
                    assets,
                    assets.wood.clone(),
                    Vec3::new(step as f32 + 0.5, height * 0.5, 0.5),
                    Vec3::new(1.0, height, 1.0),
                );
            }
        }
    }
}

fn spawn_box(
    commands: &mut Commands,
    root: Entity,
    assets: &LabAssets,
    material: Handle<StandardMaterial>,
    center: Vec3,
    size: Vec3,
) {
    commands.spawn((
        Mesh3d(assets.cube.clone()),
        MeshMaterial3d(material),
        Transform::from_translation(center).with_scale(size),
        ChildOf(root),
    ));
}

fn drive_shots(mut camera: Query<&mut Transform, With<LabCamera>>, mut state: ResMut<LabState>) {
    let elapsed = state.ready_at.elapsed().as_secs_f32();
    // 参考补丁首拍 1.0s 在首帧管线（窗口+着色器编译）完成前触发，第一张
    // 截到清屏色（单色图统计被 validate_png 拒绝）。全部推迟 1.5s、间隔
    // 不变；首拍同时从 28m 拉近到 ~16m 正对构件条带——远景全景天空占比
    // 过大使 stddev 跌破 8.0（见 docs/DECISIONS_R5.md R5-13.7）。
    let poses = [
        (2.5, Vec3::new(10.0, 11.0, 18.0), Vec3::new(9.5, 6.0, 3.0)),
        (4.0, Vec3::new(8.5, 8.5, 13.5), Vec3::new(3.0, 6.0, 3.0)),
        (5.5, Vec3::new(21.0, 11.0, 15.0), Vec3::new(14.0, 6.5, 3.0)),
        (7.0, Vec3::new(13.0, 12.0, 15.0), Vec3::new(8.0, 6.5, 3.0)),
    ];
    if state.shot_index < poses.len() && elapsed >= poses[state.shot_index].0 {
        let (_, eye, target) = poses[state.shot_index];
        if let Ok(mut transform) = camera.single_mut() {
            *transform = Transform::from_translation(eye).looking_at(target, Vec3::Y);
        }
        let name = SHOTS[state.shot_index].to_string();
        let path = state.evidence.join("screenshots").join(&name);
        state.queue.push_back((name, path));
        state.shot_index += 1;
    }
}

fn process_shots(mut commands: Commands, mut state: ResMut<LabState>, mut log: ResMut<ShotLog>) {
    while let Some((name, path)) = state.queue.pop_front() {
        let observed = name.clone();
        commands.spawn(Screenshot::primary_window()).observe(
            move |trigger: On<ScreenshotCaptured>, mut log: ResMut<ShotLog>| {
                let image: Image = std::ops::Deref::deref(trigger.event()).clone();
                match save_screenshot(&image, &path, false) {
                    Ok((_, _, bytes)) => log.0.push((observed.clone(), true, bytes)),
                    Err(error) => {
                        error!("R5 screenshot {observed} failed: {error}");
                        log.0.push((observed.clone(), false, 0));
                    }
                }
            },
        );
        log.0.retain(|(existing, _, _)| existing != &name);
    }
}

fn finalize(mut state: ResMut<LabState>, log: Res<ShotLog>, mut exit: MessageWriter<AppExit>) {
    if state.finalized || log.0.len() < SHOTS.len() {
        return;
    }
    let mut failures = Vec::new();
    for expected in SHOTS {
        let Some((_, ok, bytes)) = log.0.iter().find(|(name, _, _)| name == expected) else {
            failures.push(format!("missing screenshot {expected}"));
            continue;
        };
        if !ok || *bytes <= 10_000 {
            failures.push(format!("invalid screenshot {expected}"));
            continue;
        }
        if let Err(error) = validate_png(&state.evidence.join("screenshots").join(expected)) {
            failures.push(format!("{expected}: {error}"));
        }
    }
    let overall = if failures.is_empty() { "PASS" } else { "FAIL" };
    let report = format!(
        "{{\n  \"overall\":\"{overall}\",\n  \"building_hash\":\"{:#018x}\",\n  \"component_count\":{},\n  \"building_types\":8,\n  \"screenshots\":{}\n}}\n",
        state.store_hash,
        state.component_count,
        log.0.len()
    );
    std::fs::write(state.evidence.join("report.json"), report).unwrap();
    std::fs::write(
        state.evidence.join("report.md"),
        format!(
            "# R5 modular-building visual lab\n\n- overall: **{overall}**\n- stable building hash: `{:#018x}`\n- stable component types: 8/8\n- components: {}\n- screenshots: {}/4\n- failures: {}\n",
            state.store_hash,
            state.component_count,
            log.0.len(),
            if failures.is_empty() {
                "none".into()
            } else {
                failures.join("; ")
            }
        ),
    )
    .unwrap();
    state.finalized = true;
    exit.write(if failures.is_empty() {
        AppExit::Success
    } else {
        AppExit::error()
    });
}

fn validate_png(path: &Path) -> Result<(), String> {
    let image = image::ImageReader::open(path)
        .map_err(|error| error.to_string())?
        .decode()
        .map_err(|error| error.to_string())?
        .to_rgba8();
    if image.dimensions() != (WINDOW[0], WINDOW[1]) {
        return Err(format!("wrong dimensions: {:?}", image.dimensions()));
    }
    let mut colors = std::collections::HashSet::new();
    let mut sum = 0.0f64;
    let mut sum_sq = 0.0f64;
    for pixel in image.pixels() {
        colors.insert((pixel[0] >> 4, pixel[1] >> 4, pixel[2] >> 4));
        let value = 0.2126 * pixel[0] as f64 + 0.7152 * pixel[1] as f64 + 0.0722 * pixel[2] as f64;
        sum += value;
        sum_sq += value * value;
    }
    let n = (image.width() as u64 * image.height() as u64) as f64;
    let mean = sum / n;
    let stddev = (sum_sq / n - mean * mean).max(0.0).sqrt();
    if !(8.0..=247.0).contains(&mean) || stddev < 8.0 || colors.len() < 64 {
        return Err(format!(
            "implausible image statistics mean={mean:.2} stddev={stddev:.2} colors={}",
            colors.len()
        ));
    }
    Ok(())
}

fn visual_catalog(
    world: &World,
    objects: &ObjectStore,
) -> Result<BuildingStore, Box<dyn std::error::Error>> {
    let mut store = BuildingStore::new();
    store.place(
        world,
        objects,
        BuildingKind::FloorBasic,
        IVec3::new(2, 5, 2),
        0,
    )?;
    store.place(
        world,
        objects,
        BuildingKind::WallBasic,
        IVec3::new(2, 5, 2),
        0,
    )?;
    store.place(
        world,
        objects,
        BuildingKind::DoorwayBasic,
        IVec3::new(3, 5, 2),
        1,
    )?;
    store.place(
        world,
        objects,
        BuildingKind::WindowBasic,
        IVec3::new(2, 5, 3),
        0,
    )?;
    store.place(
        world,
        objects,
        BuildingKind::WallBasic,
        IVec3::new(2, 5, 2),
        1,
    )?;
    store.place(
        world,
        objects,
        BuildingKind::FloorBasic,
        IVec3::new(2, 8, 2),
        0,
    )?;

    for point in [
        IVec3::new(7, 5, 2),
        IVec3::new(8, 5, 2),
        IVec3::new(7, 5, 3),
        IVec3::new(8, 5, 3),
    ] {
        store.place(world, objects, BuildingKind::ColumnBasic, point, 0)?;
    }
    store.place(
        world,
        objects,
        BuildingKind::BeamBasic,
        IVec3::new(7, 8, 2),
        0,
    )?;
    store.place(
        world,
        objects,
        BuildingKind::BeamBasic,
        IVec3::new(7, 8, 3),
        0,
    )?;
    store.place(
        world,
        objects,
        BuildingKind::RoofFlatBasic,
        IVec3::new(7, 8, 2),
        0,
    )?;

    store.place(
        world,
        objects,
        BuildingKind::FloorBasic,
        IVec3::new(12, 5, 2),
        0,
    )?;
    for point in [
        IVec3::new(15, 5, 2),
        IVec3::new(16, 5, 2),
        IVec3::new(15, 5, 3),
        IVec3::new(16, 5, 3),
    ] {
        store.place(world, objects, BuildingKind::ColumnBasic, point, 0)?;
    }
    store.place(
        world,
        objects,
        BuildingKind::FloorBasic,
        IVec3::new(15, 8, 2),
        0,
    )?;
    store.place(
        world,
        objects,
        BuildingKind::StairBasic,
        IVec3::new(12, 5, 2),
        0,
    )?;
    Ok(store)
}

fn flat_world() -> World {
    let mut world = World::empty(WorldSize::new(32, 32, 32), TerrainParams::new(7171));
    world.fill_air_all_for_test();
    for z in 0..32 {
        for x in 0..32 {
            world
                .set_voxel(IVec3::new(x, 4, z), BlockId::Stone)
                .unwrap();
        }
    }
    world.clear_all_dirty_for_test();
    world
}
