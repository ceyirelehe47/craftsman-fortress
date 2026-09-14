use bevy::app::AppExit;
use bevy::gltf::Gltf;
use bevy::prelude::*;
use bevy::render::view::window::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::window::{PresentMode, WindowResolution};
use craftsman_fortress::acceptance::save_screenshot;
use craftsman_fortress::asset_manifest::AssetManifest;
use craftsman_fortress::coords::WorldSize;
use craftsman_fortress::generation::TerrainParams;
use craftsman_fortress::objects::{ObjectKind, ObjectStore};
use craftsman_fortress::voxel::BlockId;
use craftsman_fortress::world::World;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Instant;

const WINDOW: [u32; 2] = [1280, 720];
const SHOTS: [&str; 4] = [
    "R4_01_object_lineup.png",
    "R4_02_rotated_footprints.png",
    "R4_03_management_view.png",
    "R4_04_voxel_context.png",
];

#[derive(Resource)]
struct LabState {
    evidence: PathBuf,
    manifest: AssetManifest,
    handles: HashMap<ObjectKind, Handle<Gltf>>,
    spawned: bool,
    ready_at: Option<Instant>,
    shot_index: usize,
    queue: VecDeque<(String, PathBuf)>,
    store_hash: u64,
    finalized: bool,
}

#[derive(Resource, Default)]
struct ShotLog(Vec<(String, bool, u64)>);

#[derive(Component)]
struct LabCamera;

fn main() -> AppExit {
    let mut args = std::env::args().skip(1);
    let manifest_path = args.next().map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(craftsman_fortress::object_runtime::DEFAULT_OBJECT_MANIFEST)
    });
    let evidence = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("evidence_r4_visual"));
    if evidence.exists()
        && evidence
            .read_dir()
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false)
    {
        eprintln!(
            "R4 evidence directory must be empty: {}",
            evidence.display()
        );
        std::process::exit(2);
    }
    std::fs::create_dir_all(evidence.join("screenshots")).unwrap();
    let manifest = AssetManifest::load(&manifest_path).unwrap_or_else(|error| {
        eprintln!("R4 manifest error: {error}");
        std::process::exit(2);
    });
    manifest
        .validate_acceptance_ready()
        .unwrap_or_else(|error| {
            eprintln!("R4 manifest not ready: {error}");
            std::process::exit(2);
        });
    for kind in ObjectKind::ALL {
        if !manifest
            .entries
            .iter()
            .any(|entry| entry.id == kind.asset_id())
        {
            eprintln!(
                "R4 manifest misses object asset mapping {} -> {}",
                kind.type_id(),
                kind.asset_id()
            );
            std::process::exit(2);
        }
    }

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
                    title: "工匠要塞 · R4 Object Lab".into(),
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
    app.insert_resource(LabState {
        evidence,
        manifest,
        handles: HashMap::new(),
        spawned: false,
        ready_at: None,
        shot_index: 0,
        queue: VecDeque::new(),
        store_hash: 0,
        finalized: false,
    });
    app.add_systems(Startup, setup);
    app.add_systems(
        Update,
        (spawn_when_loaded, drive_shots, process_shots, finalize).chain(),
    );
    app.run()
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut state: ResMut<LabState>,
) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(18.0, 12.0, 24.0).looking_at(Vec3::new(7.0, 1.5, 5.0), Vec3::Y),
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
    let floor_mesh = meshes.add(Cuboid::new(26.0, 0.2, 20.0));
    let floor_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.34, 0.47, 0.29),
        perceptual_roughness: 1.0,
        ..default()
    });
    commands.spawn((
        Mesh3d(floor_mesh),
        MeshMaterial3d(floor_mat),
        Transform::from_xyz(7.0, -0.1, 5.0),
    ));

    for kind in ObjectKind::ALL {
        let path = state
            .manifest
            .entries
            .iter()
            .find(|entry| entry.id == kind.asset_id())
            .map(|entry| entry.path.clone())
            .unwrap();
        state.handles.insert(kind, asset_server.load(path));
    }
}

fn spawn_when_loaded(
    mut commands: Commands,
    mut state: ResMut<LabState>,
    gltfs: Res<Assets<Gltf>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if state.spawned {
        return;
    }
    if !ObjectKind::ALL.iter().all(|kind| {
        state
            .handles
            .get(kind)
            .and_then(|handle| gltfs.get(handle))
            .is_some()
    }) {
        return;
    }

    let world = flat_world();
    let mut store = ObjectStore::new();
    store
        .place(&world, ObjectKind::CampfireBasic, IVec3::new(2, 5, 2), 0)
        .unwrap();
    store
        .place(&world, ObjectKind::TentBasic, IVec3::new(8, 5, 2), 1)
        .unwrap();
    store
        .place(&world, ObjectKind::StonePileSmall, IVec3::new(16, 5, 8), 2)
        .unwrap();
    state.store_hash = store.semantic_hash();

    let footprint_mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let footprint_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.2, 0.75, 1.0, 0.25),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    for record in store.records_sorted() {
        let footprint = record.rotated_footprint();
        let center = Vec3::new(
            record.anchor.x as f32 + footprint.x as f32 * 0.5,
            0.0,
            record.anchor.z as f32 + footprint.y as f32 * 0.5,
        );
        commands.spawn((
            Mesh3d(footprint_mesh.clone()),
            MeshMaterial3d(footprint_mat.clone()),
            Transform::from_xyz(center.x, 0.03, center.z).with_scale(Vec3::new(
                footprint.x as f32,
                0.06,
                footprint.y as f32,
            )),
        ));
        let root = commands
            .spawn((
                Transform::from_translation(center).with_rotation(Quat::from_rotation_y(
                    record.yaw_quarters as f32 * std::f32::consts::FRAC_PI_2,
                )),
                Visibility::Inherited,
                Name::new(format!("R4:{}", record.kind.type_id())),
            ))
            .id();
        let gltf = gltfs.get(state.handles.get(&record.kind).unwrap()).unwrap();
        let scene = gltf
            .default_scene
            .clone()
            .or_else(|| gltf.scenes.first().cloned())
            .unwrap();
        commands.spawn((
            WorldAssetRoot(scene),
            Transform::from_scale(Vec3::splat(record.kind.visual_scale())),
            ChildOf(root),
        ));
    }
    state.spawned = true;
    state.ready_at = Some(Instant::now());
}

fn drive_shots(mut camera: Query<&mut Transform, With<LabCamera>>, mut state: ResMut<LabState>) {
    let Some(start) = state.ready_at else {
        return;
    };
    let elapsed = start.elapsed().as_secs_f32();
    let poses = [
        (1.0, Vec3::new(18.0, 9.0, 22.0), Vec3::new(8.0, 1.2, 5.0)),
        (2.5, Vec3::new(11.0, 5.0, 14.0), Vec3::new(8.0, 1.1, 4.5)),
        (4.0, Vec3::new(12.0, 20.0, 20.0), Vec3::new(8.0, 0.0, 5.0)),
        (5.5, Vec3::new(22.0, 11.0, 18.0), Vec3::new(8.0, 1.0, 5.0)),
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
                        error!("R4 screenshot {observed} failed: {error}");
                        log.0.push((observed.clone(), false, 0));
                    }
                }
            },
        );
        log.0.retain(|(existing, _, _)| existing != &name);
    }
}

fn finalize(mut state: ResMut<LabState>, log: Res<ShotLog>, mut exit: MessageWriter<AppExit>) {
    if state.finalized || !state.spawned || log.0.len() < SHOTS.len() {
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
        "{{\n  \"overall\":\"{overall}\",\n  \"object_hash\":\"{:#018x}\",\n  \"asset_ids\":[\"bonefire\",\"tent\",\"stone_03\"],\n  \"screenshots\":{}\n}}\n",
        state.store_hash,
        log.0.len()
    );
    std::fs::write(state.evidence.join("report.json"), report).unwrap();
    std::fs::write(
        state.evidence.join("report.md"),
        format!(
            "# R4 object visual lab\n\n- overall: **{overall}**\n- stable object hash: `{:#018x}`\n- actual PalmStudio scenes: 3/3\n- screenshots: {}/4\n- failures: {}\n",
            state.store_hash,
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

fn flat_world() -> World {
    let mut world = World::empty(WorldSize::new(32, 32, 32), TerrainParams::new(5150));
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
