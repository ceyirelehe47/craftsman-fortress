use bevy::app::AppExit;
use bevy::camera::primitives::{Aabb, MeshAabb};
use bevy::gltf::{Gltf, GltfMesh, GltfNode};
use bevy::prelude::*;
use bevy::render::view::window::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::window::{PresentMode, WindowResolution};
use craftsman_fortress::acceptance::save_screenshot;
use craftsman_fortress::asset_manifest::{AssetManifest, AssetSpec, NormalizationPlan};
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Instant;

const WINDOW: [u32; 2] = [1280, 720];
const SCREENSHOTS: [&str; 5] = [
    "R3_01_scale_lineup.png",
    "R3_02_close_front.png",
    "R3_03_side_profile.png",
    "R3_04_management_view.png",
    "R3_05_chunk_grid_context.png",
];

#[derive(Clone, Debug)]
struct LabArgs {
    manifest_path: PathBuf,
    evidence_dir: PathBuf,
}

impl LabArgs {
    fn parse() -> Result<Self, String> {
        let mut manifest_path = None;
        let mut evidence_dir = None;
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--manifest" => {
                    manifest_path = Some(PathBuf::from(
                        args.next().ok_or("--manifest requires a path")?,
                    ));
                }
                "--evidence" => {
                    evidence_dir = Some(PathBuf::from(
                        args.next().ok_or("--evidence requires a path")?,
                    ));
                }
                "--help" | "-h" => {
                    println!(
                        "r3_asset_lab --manifest assets/.../selection.tsv --evidence evidence_dir"
                    );
                    return Err("help requested".into());
                }
                other => return Err(format!("unknown argument: {other}")),
            }
        }
        Ok(Self {
            manifest_path: manifest_path.ok_or("missing --manifest")?,
            evidence_dir: evidence_dir.ok_or("missing --evidence")?,
        })
    }
}

#[derive(Clone, Debug)]
struct Bounds {
    min: Vec3,
    max: Vec3,
}

impl Bounds {
    fn empty() -> Self {
        Self {
            min: Vec3::splat(f32::INFINITY),
            max: Vec3::splat(f32::NEG_INFINITY),
        }
    }

    fn include(&mut self, point: Vec3) {
        self.min = self.min.min(point);
        self.max = self.max.max(point);
    }

    fn valid(&self) -> bool {
        self.min.is_finite()
            && self.max.is_finite()
            && self.max.x > self.min.x
            && self.max.y > self.min.y
            && self.max.z > self.min.z
    }

    fn size(&self) -> Vec3 {
        self.max - self.min
    }
}

#[derive(Clone, Debug)]
struct RuntimeAsset {
    spec: AssetSpec,
    handle: Handle<Gltf>,
    scene: Option<Handle<WorldAsset>>,
    root: Option<Entity>,
    raw: Option<Bounds>,
    normalized: Option<Bounds>,
    scale: f32,
    mesh_count: usize,
    material_count: usize,
    animation_count: usize,
    error: Option<String>,
}

#[derive(Resource)]
struct LabState {
    args: LabArgs,
    manifest: AssetManifest,
    assets: Vec<RuntimeAsset>,
    started_at: Instant,
    ready_at: Option<Instant>,
    shot_index: usize,
    shot_queue: VecDeque<(String, PathBuf)>,
    baseline_counts: Option<ResourceCounts>,
    lifecycle_counts: Vec<(String, ResourceCounts)>,
    temp_roots: Vec<Entity>,
    lifecycle_cycle: usize,
    lifecycle_spawned: bool,
    lifecycle_waiting_for_settle: bool,
    lifecycle_settle_frames: u16,
    finalized: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ResourceCounts {
    meshes: usize,
    materials: usize,
    images: usize,
    entities: usize,
}

#[derive(Component)]
struct LabCamera;

#[derive(Component)]
struct LabModelRoot;

#[derive(Component)]
struct TempLifecycleRoot;

#[derive(Resource, Default)]
struct ShotLog(Vec<(String, bool, u64)>);

fn main() -> AppExit {
    let args = LabArgs::parse().unwrap_or_else(|error| {
        eprintln!("R3 asset lab argument error: {error}");
        std::process::exit(2);
    });
    if args.evidence_dir.exists()
        && args
            .evidence_dir
            .read_dir()
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false)
    {
        eprintln!(
            "R3 evidence directory must be empty: {}",
            args.evidence_dir.display()
        );
        std::process::exit(2);
    }
    std::fs::create_dir_all(args.evidence_dir.join("screenshots")).unwrap_or_else(|error| {
        eprintln!("cannot create evidence directory: {error}");
        std::process::exit(2);
    });

    let manifest = AssetManifest::load(&args.manifest_path).unwrap_or_else(|error| {
        eprintln!("R3 manifest error: {error}");
        std::process::exit(2);
    });
    manifest
        .validate_acceptance_ready()
        .unwrap_or_else(|error| {
            eprintln!("R3 manifest is not acceptance-ready: {error}");
            std::process::exit(2);
        });
    manifest
        .validate_files(Path::new("assets"))
        .unwrap_or_else(|error| {
            eprintln!("R3 local asset check failed: {error}");
            std::process::exit(2);
        });
    let manifest_snapshot = args.evidence_dir.join("manifest_snapshot.tsv");
    std::fs::copy(&args.manifest_path, &manifest_snapshot).unwrap_or_else(|error| {
        eprintln!("cannot copy manifest snapshot: {error}");
        std::process::exit(2);
    });

    let mut app = App::new();
    // Bevy 默认以可执行文件所在目录为资产根（target/release/examples/），
    // 与验收脚本"仓库根目录为 CWD、相对路径 assets/..."的约定不符。
    // 这里把资产根钉在 <CWD>/assets，与 validate_files 的解析基准保持一致。
    let asset_root = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("assets");
    app.add_plugins(
        DefaultPlugins
            .set(bevy::asset::AssetPlugin {
                file_path: asset_root.to_string_lossy().to_string(),
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "工匠要塞 · R3 Asset Lab".into(),
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
    app.insert_resource(ClearColor(Color::srgb(0.55, 0.69, 0.82)));
    app.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: 700.0,
        ..default()
    });
    app.insert_resource(ShotLog::default());
    app.insert_resource(LabState {
        args,
        manifest,
        assets: Vec::new(),
        started_at: Instant::now(),
        ready_at: None,
        shot_index: 0,
        shot_queue: VecDeque::new(),
        baseline_counts: None,
        lifecycle_counts: Vec::new(),
        temp_roots: Vec::new(),
        lifecycle_cycle: 0,
        lifecycle_spawned: false,
        lifecycle_waiting_for_settle: false,
        lifecycle_settle_frames: 0,
        finalized: false,
    });
    app.add_systems(Startup, setup);
    app.add_systems(
        Update,
        (
            prepare_loaded_assets,
            drive_camera_and_lifecycle,
            process_shot_queue,
            finalize_when_ready,
        )
            .chain(),
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
        Transform::from_xyz(16.0, 10.0, 24.0).looking_at(Vec3::new(8.0, 2.0, 0.0), Vec3::Y),
        LabCamera,
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 10_000.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.9, -0.8, 0.0)),
    ));

    let floor_mesh = meshes.add(Cuboid::new(40.0, 0.1, 28.0));
    let floor_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.36, 0.47, 0.31),
        perceptual_roughness: 1.0,
        ..default()
    });
    commands.spawn((
        Mesh3d(floor_mesh),
        MeshMaterial3d(floor_material),
        Transform::from_xyz(8.0, -0.05, 0.0),
    ));

    let meter_mesh = meshes.add(Cuboid::new(0.08, 1.0, 0.08));
    let meter_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.95, 0.18, 0.12),
        emissive: LinearRgba::new(0.2, 0.0, 0.0, 1.0),
        ..default()
    });
    for i in 0..=5 {
        commands.spawn((
            Mesh3d(meter_mesh.clone()),
            MeshMaterial3d(meter_material.clone()),
            Transform::from_xyz(-2.0, 0.5 + i as f32, 0.0),
        ));
    }

    state.assets = state
        .manifest
        .entries
        .iter()
        .cloned()
        .map(|spec| RuntimeAsset {
            handle: asset_server.load(spec.path.clone()),
            spec,
            scene: None,
            root: None,
            raw: None,
            normalized: None,
            scale: 1.0,
            mesh_count: 0,
            material_count: 0,
            animation_count: 0,
            error: None,
        })
        .collect();
    info!("R3 asset lab queued {} GLB files", state.assets.len());
}

fn prepare_loaded_assets(
    mut commands: Commands,
    mut state: ResMut<LabState>,
    gltfs: Res<Assets<Gltf>>,
    nodes: Res<Assets<GltfNode>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    meshes: Res<Assets<Mesh>>,
) {
    if state.ready_at.is_some() {
        return;
    }
    let columns = 4usize;
    for (index, runtime) in state.assets.iter_mut().enumerate() {
        if runtime.root.is_some() || runtime.error.is_some() {
            continue;
        }
        let Some(gltf) = gltfs.get(&runtime.handle) else {
            continue;
        };
        let scene = gltf
            .default_scene
            .clone()
            .or_else(|| gltf.scenes.first().cloned());
        let Some(scene) = scene else {
            runtime.error = Some("GLB has no scene".into());
            continue;
        };
        let Some(raw) = gltf_bounds(gltf, &nodes, &gltf_meshes, &meshes) else {
            continue;
        };
        let plan = match NormalizationPlan::from_bounds(
            raw.min.to_array(),
            raw.max.to_array(),
            runtime.spec.target_height_m,
        ) {
            Ok(plan) => plan,
            Err(error) => {
                runtime.error = Some(error.to_string());
                continue;
            }
        };
        let row = index / columns;
        let col = index % columns;
        let position = Vec3::new(col as f32 * 5.0, plan.ground_offset_y, -(row as f32) * 6.0);
        let rotation = Quat::from_rotation_y(runtime.spec.yaw_deg.to_radians());
        let root = commands
            .spawn((
                Transform {
                    translation: position,
                    rotation,
                    scale: Vec3::splat(plan.scale),
                },
                Visibility::Inherited,
                LabModelRoot,
                Name::new(format!("R3:{}", runtime.spec.id)),
            ))
            .id();
        commands.spawn((WorldAssetRoot(scene.clone()), ChildOf(root)));

        let normalized = transform_bounds(&raw, position, rotation, plan.scale);
        runtime.scene = Some(scene);
        runtime.root = Some(root);
        runtime.raw = Some(raw);
        runtime.normalized = Some(normalized);
        runtime.scale = plan.scale;
        runtime.mesh_count = gltf.meshes.len();
        runtime.material_count = gltf.materials.len();
        runtime.animation_count = gltf.animations.len();
    }

    if state.started_at.elapsed().as_secs_f32() > 60.0 {
        for runtime in &mut state.assets {
            if runtime.root.is_none() && runtime.error.is_none() {
                runtime.error = Some("GLB did not finish loading within 60 seconds".into());
            }
        }
    }
    let complete = state
        .assets
        .iter()
        .all(|asset| asset.root.is_some() || asset.error.is_some());
    if complete {
        state.ready_at = Some(Instant::now());
        info!("R3 all assets reached a terminal load state");
    }
}

fn gltf_bounds(
    gltf: &Gltf,
    nodes: &Assets<GltfNode>,
    gltf_meshes: &Assets<GltfMesh>,
    meshes: &Assets<Mesh>,
) -> Option<Bounds> {
    let mut child_ids = HashSet::new();
    for handle in &gltf.nodes {
        let node = nodes.get(handle)?;
        for child in &node.children {
            child_ids.insert(child.id());
        }
    }
    let roots: Vec<_> = gltf
        .nodes
        .iter()
        .filter(|handle| !child_ids.contains(&handle.id()))
        .cloned()
        .collect();
    let roots = if roots.is_empty() {
        gltf.nodes.clone()
    } else {
        roots
    };
    let mut bounds = Bounds::empty();
    let mut visited = HashSet::new();
    for root in roots {
        visit_node(
            &root,
            Transform::IDENTITY,
            nodes,
            gltf_meshes,
            meshes,
            &mut visited,
            &mut bounds,
        )?;
    }
    bounds.valid().then_some(bounds)
}

fn visit_node(
    handle: &Handle<GltfNode>,
    parent: Transform,
    nodes: &Assets<GltfNode>,
    gltf_meshes: &Assets<GltfMesh>,
    meshes: &Assets<Mesh>,
    visited: &mut HashSet<AssetId<GltfNode>>,
    bounds: &mut Bounds,
) -> Option<()> {
    if !visited.insert(handle.id()) {
        return Some(());
    }
    let node = nodes.get(handle)?;
    let world = parent.mul_transform(node.transform);
    if let Some(mesh_handle) = node.mesh.as_ref() {
        let gltf_mesh = gltf_meshes.get(mesh_handle)?;
        for primitive in &gltf_mesh.primitives {
            let mesh = meshes.get(&primitive.mesh)?;
            let aabb = mesh.compute_aabb()?;
            include_aabb(bounds, &aabb, world);
        }
    }
    for child in &node.children {
        visit_node(child, world, nodes, gltf_meshes, meshes, visited, bounds)?;
    }
    Some(())
}

fn include_aabb(bounds: &mut Bounds, aabb: &Aabb, transform: Transform) {
    let min: Vec3 = aabb.min().into();
    let max: Vec3 = aabb.max().into();
    for x in [min.x, max.x] {
        for y in [min.y, max.y] {
            for z in [min.z, max.z] {
                bounds.include(transform.transform_point(Vec3::new(x, y, z)));
            }
        }
    }
}

fn transform_bounds(raw: &Bounds, translation: Vec3, rotation: Quat, scale: f32) -> Bounds {
    let mut out = Bounds::empty();
    for x in [raw.min.x, raw.max.x] {
        for y in [raw.min.y, raw.max.y] {
            for z in [raw.min.z, raw.max.z] {
                out.include(translation + rotation * (Vec3::new(x, y, z) * scale));
            }
        }
    }
    out
}

fn drive_camera_and_lifecycle(
    mut commands: Commands,
    mut state: ResMut<LabState>,
    mut camera: Query<&mut Transform, With<LabCamera>>,
    meshes: Res<Assets<Mesh>>,
    materials: Res<Assets<StandardMaterial>>,
    images: Res<Assets<Image>>,
    entities: Query<Entity>,
) {
    let Some(ready_at) = state.ready_at else {
        return;
    };
    let elapsed = ready_at.elapsed().as_secs_f32();
    let Ok(mut camera_transform) = camera.single_mut() else {
        return;
    };

    let shots = [
        (2.0, Vec3::new(12.0, 7.0, 22.0), Vec3::new(7.0, 2.0, -3.0)),
        (3.5, Vec3::new(4.0, 3.0, 10.0), Vec3::new(4.0, 1.5, 0.0)),
        (5.0, Vec3::new(22.0, 5.0, 2.0), Vec3::new(7.0, 1.5, -3.0)),
        (6.5, Vec3::new(10.0, 20.0, 18.0), Vec3::new(7.0, 0.0, -4.0)),
        (8.0, Vec3::new(20.0, 12.0, 24.0), Vec3::new(8.0, 1.0, -4.0)),
    ];
    if state.shot_index < shots.len() && elapsed >= shots[state.shot_index].0 {
        let (_, eye, target) = shots[state.shot_index];
        *camera_transform = Transform::from_translation(eye).looking_at(target, Vec3::Y);
        let name = SCREENSHOTS[state.shot_index].to_string();
        let path = state.args.evidence_dir.join("screenshots").join(&name);
        state.shot_queue.push_back((name, path));
        state.shot_index += 1;
    }

    if elapsed >= 9.0 && state.baseline_counts.is_none() {
        let counts = ResourceCounts {
            meshes: meshes.len(),
            materials: materials.len(),
            images: images.len(),
            entities: entities.iter().count(),
        };
        state.baseline_counts = Some(counts);
        state.lifecycle_counts.push(("baseline".into(), counts));
    }

    if elapsed >= 10.0 && state.lifecycle_cycle < 5 {
        let counts = ResourceCounts {
            meshes: meshes.len(),
            materials: materials.len(),
            images: images.len(),
            entities: entities.iter().count(),
        };
        if state.lifecycle_waiting_for_settle {
            state.lifecycle_settle_frames = state.lifecycle_settle_frames.saturating_add(1);
            let baseline = state.baseline_counts.unwrap_or_default();
            let returned = counts.meshes == baseline.meshes
                && counts.materials == baseline.materials
                && counts.images == baseline.images
                && counts.entities <= baseline.entities + 2;
            if returned || state.lifecycle_settle_frames >= 120 {
                let cycle = state.lifecycle_cycle + 1;
                state
                    .lifecycle_counts
                    .push((format!("cycle_{cycle}"), counts));
                state.lifecycle_cycle = cycle;
                state.lifecycle_waiting_for_settle = false;
                state.lifecycle_settle_frames = 0;
            }
        } else if !state.lifecycle_spawned {
            let scenes: Vec<_> = state
                .assets
                .iter()
                .filter_map(|asset| asset.scene.clone())
                .collect();
            for repeat in 0..3 {
                for (index, scene) in scenes.iter().enumerate() {
                    let root = commands
                        .spawn((
                            Transform::from_xyz(
                                100.0 + index as f32 * 3.0,
                                0.0,
                                repeat as f32 * 3.0,
                            ),
                            Visibility::Hidden,
                            TempLifecycleRoot,
                        ))
                        .id();
                    commands.spawn((WorldAssetRoot(scene.clone()), ChildOf(root)));
                    state.temp_roots.push(root);
                }
            }
            state.lifecycle_spawned = true;
        } else if elapsed >= 10.75 + state.lifecycle_cycle as f32 * 1.5 {
            for root in state.temp_roots.drain(..) {
                commands.entity(root).despawn();
            }
            state.lifecycle_spawned = false;
            state.lifecycle_waiting_for_settle = true;
            state.lifecycle_settle_frames = 0;
        }
    }
}

fn process_shot_queue(
    mut commands: Commands,
    mut state: ResMut<LabState>,
    mut log: ResMut<ShotLog>,
) {
    while let Some((name, path)) = state.shot_queue.pop_front() {
        let observed_name = name.clone();
        commands.spawn(Screenshot::primary_window()).observe(
            move |trigger: On<ScreenshotCaptured>, mut log: ResMut<ShotLog>| {
                let image: Image = std::ops::Deref::deref(trigger.event()).clone();
                let result = save_screenshot(&image, &path, false);
                match result {
                    Ok((_, _, bytes)) => log.0.push((observed_name.clone(), true, bytes)),
                    Err(error) => {
                        error!("R3 screenshot {} failed: {error}", observed_name);
                        log.0.push((observed_name.clone(), false, 0));
                    }
                }
            },
        );
        log.0.retain(|(existing, _, _)| existing != &name);
    }
}

fn finalize_when_ready(
    mut state: ResMut<LabState>,
    log: Res<ShotLog>,
    meshes: Res<Assets<Mesh>>,
    materials: Res<Assets<StandardMaterial>>,
    images: Res<Assets<Image>>,
    entities: Query<Entity>,
    mut exit: MessageWriter<AppExit>,
) {
    if state.finalized {
        return;
    }
    let Some(ready_at) = state.ready_at else {
        return;
    };
    if ready_at.elapsed().as_secs_f32() < 18.0
        || state.lifecycle_cycle < 5
        || log.0.len() < SCREENSHOTS.len()
    {
        return;
    }

    let final_counts = ResourceCounts {
        meshes: meshes.len(),
        materials: materials.len(),
        images: images.len(),
        entities: entities.iter().count(),
    };
    state.lifecycle_counts.push(("final".into(), final_counts));
    let baseline = state.baseline_counts.unwrap_or_default();

    let mut failures = Vec::new();
    for asset in &state.assets {
        if let Some(error) = asset.error.as_ref() {
            failures.push(format!("{}: {error}", asset.spec.id));
            continue;
        }
        let Some(normalized) = asset.normalized.as_ref() else {
            failures.push(format!("{}: missing normalized bounds", asset.spec.id));
            continue;
        };
        let height = normalized.size().y;
        let tolerance = (asset.spec.target_height_m * 0.02).max(0.02);
        if (height - asset.spec.target_height_m).abs() > tolerance {
            failures.push(format!(
                "{}: height {} differs from target {}",
                asset.spec.id, height, asset.spec.target_height_m
            ));
        }
        if normalized.min.y.abs() > 0.02 {
            failures.push(format!(
                "{}: grounded min_y={} exceeds tolerance",
                asset.spec.id, normalized.min.y
            ));
        }
    }
    for expected in SCREENSHOTS {
        match log.0.iter().find(|(name, _, _)| name == expected) {
            Some((_, true, bytes)) if *bytes > 10_000 => {
                let path = state.args.evidence_dir.join("screenshots").join(expected);
                if let Err(error) = validate_screenshot_file(&path) {
                    failures.push(format!("screenshot {expected}: {error}"));
                }
            }
            _ => failures.push(format!("missing or invalid screenshot: {expected}")),
        }
    }
    if final_counts.meshes != baseline.meshes
        || final_counts.materials != baseline.materials
        || final_counts.images != baseline.images
    {
        failures.push(format!(
            "asset resources changed after lifecycle: baseline={baseline:?}, final={final_counts:?}"
        ));
    }
    if final_counts.entities > baseline.entities + 2 {
        failures.push(format!(
            "entity count did not return to baseline: baseline={}, final={}",
            baseline.entities, final_counts.entities
        ));
    }
    let cycle_rows: Vec<_> = state
        .lifecycle_counts
        .iter()
        .filter(|(phase, _)| phase.starts_with("cycle_"))
        .collect();
    if cycle_rows.len() != 5 {
        failures.push(format!(
            "expected 5 settled lifecycle rows, got {}",
            cycle_rows.len()
        ));
    }
    for (phase, counts) in cycle_rows {
        if counts.meshes != baseline.meshes
            || counts.materials != baseline.materials
            || counts.images != baseline.images
            || counts.entities > baseline.entities + 2
        {
            failures.push(format!(
                "{phase} did not settle to baseline: baseline={baseline:?}, actual={counts:?}"
            ));
        }
    }

    let pass = failures.is_empty();
    if let Err(error) = write_reports(&state, &log, baseline, final_counts, &failures) {
        error!("R3 report write failed: {error}");
        exit.write(AppExit::error());
    } else if pass {
        info!("R3 asset lab PASS");
        exit.write(AppExit::Success);
    } else {
        error!("R3 asset lab FAIL: {}", failures.join(" | "));
        exit.write(AppExit::error());
    }
    state.finalized = true;
}

fn validate_screenshot_file(path: &Path) -> Result<(), String> {
    let image = image::ImageReader::open(path)
        .map_err(|error| format!("cannot open PNG: {error}"))?
        .decode()
        .map_err(|error| format!("cannot decode PNG: {error}"))?
        .to_rgba8();
    if image.dimensions() != (WINDOW[0], WINDOW[1]) {
        return Err(format!(
            "wrong dimensions {:?}, expected {:?}",
            image.dimensions(),
            WINDOW
        ));
    }
    let mut sum = 0.0f64;
    let mut sum_sq = 0.0f64;
    let mut quantized = HashSet::new();
    let pixels = image.width() as u64 * image.height() as u64;
    for pixel in image.pixels() {
        let luma = 0.2126 * pixel[0] as f64 + 0.7152 * pixel[1] as f64 + 0.0722 * pixel[2] as f64;
        sum += luma;
        sum_sq += luma * luma;
        quantized.insert((pixel[0] >> 4, pixel[1] >> 4, pixel[2] >> 4));
    }
    let count = pixels as f64;
    let mean = sum / count;
    let variance = (sum_sq / count - mean * mean).max(0.0);
    if !(8.0..=247.0).contains(&mean) {
        return Err(format!("mean luminance is not plausible: {mean:.2}"));
    }
    if variance.sqrt() < 8.0 {
        return Err(format!(
            "image is nearly uniform: stddev={:.2}",
            variance.sqrt()
        ));
    }
    if quantized.len() < 64 {
        return Err(format!("too few quantized colors: {}", quantized.len()));
    }
    Ok(())
}

fn write_reports(
    state: &LabState,
    log: &ShotLog,
    baseline: ResourceCounts,
    final_counts: ResourceCounts,
    failures: &[String],
) -> std::io::Result<()> {
    let mut asset_json = String::new();
    for (index, asset) in state.assets.iter().enumerate() {
        if index > 0 {
            asset_json.push_str(",\n");
        }
        let raw = asset.raw.as_ref();
        let normalized = asset.normalized.as_ref();
        asset_json.push_str(&format!(
            "    {{\"id\":\"{}\",\"path\":\"{}\",\"category\":\"{}\",\"target_height_m\":{:.6},\"yaw_deg\":{:.6},\"scale\":{:.9},\"raw_min\":{},\"raw_max\":{},\"final_min\":{},\"final_max\":{},\"meshes\":{},\"materials\":{},\"animations\":{},\"error\":{}}}",
            json_escape(&asset.spec.id),
            json_escape(&asset.spec.path),
            asset.spec.category.as_str(),
            asset.spec.target_height_m,
            asset.spec.yaw_deg,
            asset.scale,
            vec_json(raw.map(|b| b.min)),
            vec_json(raw.map(|b| b.max)),
            vec_json(normalized.map(|b| b.min)),
            vec_json(normalized.map(|b| b.max)),
            asset.mesh_count,
            asset.material_count,
            asset.animation_count,
            asset
                .error
                .as_ref()
                .map(|e| format!("\"{}\"", json_escape(e)))
                .unwrap_or_else(|| "null".into())
        ));
    }
    let screenshot_json = log
        .0
        .iter()
        .map(|(name, ok, bytes)| {
            format!(
                "{{\"name\":\"{}\",\"ok\":{},\"bytes\":{}}}",
                json_escape(name),
                ok,
                bytes
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let failure_json = failures
        .iter()
        .map(|failure| format!("\"{}\"", json_escape(failure)))
        .collect::<Vec<_>>()
        .join(",");
    let overall = if failures.is_empty() { "PASS" } else { "FAIL" };
    let transform_fingerprint = transform_fingerprint(&state.assets);
    let report = format!(
        "{{\n  \"overall\":\"{overall}\",\n  \"manifest_hash\":\"{:#018x}\",\n  \"transform_fingerprint\":\"{transform_fingerprint:#018x}\",\n  \"entry_count\":{},\n  \"assets\":[\n{asset_json}\n  ],\n  \"screenshots\":[{screenshot_json}],\n  \"resource_baseline\":{},\n  \"resource_final\":{},\n  \"failures\":[{failure_json}]\n}}\n",
        state.manifest.canonical_hash(),
        state.assets.len(),
        counts_json(baseline),
        counts_json(final_counts)
    );
    std::fs::write(state.args.evidence_dir.join("asset_report.json"), report)?;

    let mut csv = String::from("phase,meshes,materials,images,entities\n");
    for (phase, counts) in &state.lifecycle_counts {
        csv.push_str(&format!(
            "{},{},{},{},{}\n",
            phase, counts.meshes, counts.materials, counts.images, counts.entities
        ));
    }
    std::fs::write(state.args.evidence_dir.join("asset_lifecycle.csv"), csv)?;
    std::fs::write(
        state.args.evidence_dir.join("report.md"),
        format!(
            "# R3 asset lab\n\n- overall: **{overall}**\n- selected assets: {}\n- manifest hash: `{:#018x}`\n- screenshots: {}\n- lifecycle baseline: `{:?}`\n- lifecycle final: `{:?}`\n- failures: {}\n",
            state.assets.len(),
            state.manifest.canonical_hash(),
            log.0.len(),
            baseline,
            final_counts,
            if failures.is_empty() {
                "none".into()
            } else {
                failures.join("; ")
            }
        ),
    )?;
    Ok(())
}

fn transform_fingerprint(assets: &[RuntimeAsset]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut feed = |text: &str| {
        for byte in text.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    for asset in assets {
        feed(&asset.spec.id);
        feed(&format!("{:.9}", asset.scale));
        if let Some(bounds) = asset.normalized.as_ref() {
            feed(&format!(
                "{:.6},{:.6},{:.6}|{:.6},{:.6},{:.6}",
                bounds.min.x, bounds.min.y, bounds.min.z, bounds.max.x, bounds.max.y, bounds.max.z
            ));
        }
    }
    hash
}

fn vec_json(value: Option<Vec3>) -> String {
    value
        .map(|v| format!("[{:.6},{:.6},{:.6}]", v.x, v.y, v.z))
        .unwrap_or_else(|| "null".into())
}

fn counts_json(value: ResourceCounts) -> String {
    format!(
        "{{\"meshes\":{},\"materials\":{},\"images\":{},\"entities\":{}}}",
        value.meshes, value.materials, value.images, value.entities
    )
}

fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}
