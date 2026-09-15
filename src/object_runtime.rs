//! R4 对象 ECS 表现与交互。
//!
//! `ObjectStore` 是权威数据；ECS Entity 仅为可重建表现。第三方 GLB 缺失或尚未加载时
//! 使用确定性占地占位体，存档永远只写稳定对象类型与空间状态。

use crate::app_state::GameState;
use crate::asset_manifest::AssetManifest;
use crate::building_runtime::BuildingWorldRes;
use crate::camera::WorldRes;
use crate::objects::{ObjectId, ObjectKind, ObjectStore, PlacedObject};
use crate::picking::{CurrentPickRes, CurrentRayRes};
use bevy::gltf::Gltf;
use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;
use bevy::text::FontSize;
use std::collections::HashMap;
use std::path::PathBuf;

pub const DEFAULT_OBJECT_MANIFEST: &str =
    "assets/vendor_local/voxel_survival_pack/v1.0/free_sample/selection.tsv";

#[derive(Resource, Debug)]
pub struct ObjectWorldRes {
    pub store: ObjectStore,
    pub dirty: bool,
    pub last_generation: Option<u64>,
    pub last_message: String,
}

impl ObjectWorldRes {
    pub fn new(store: ObjectStore, loaded_generation: Option<u64>) -> Self {
        let last_message = loaded_generation
            .map(|generation| format!("Loaded object generation {generation}"))
            .unwrap_or_else(|| "Object layer ready".into());
        Self {
            store,
            dirty: false,
            last_generation: loaded_generation,
            last_message,
        }
    }
}

#[derive(Resource)]
struct ObjectUiState {
    kind: ObjectKind,
    yaw_quarters: u8,
    selected: Option<ObjectId>,
    moving: Option<ObjectId>,
}

#[derive(Resource)]
struct ObjectManifestPath(PathBuf);

#[derive(Resource, Default)]
struct ObjectAssetCatalog {
    handles: HashMap<ObjectKind, Handle<Gltf>>,
    manifest_loaded: bool,
}

#[derive(Resource)]
struct PlaceholderAssets {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

#[derive(Resource)]
struct ObjectVisualState {
    last_revision: u64,
    last_ready_count: usize,
    roots: Vec<Entity>,
}

#[derive(Component)]
struct ObjectVisualRoot;

#[derive(Component)]
struct ObjectStatusText;

pub fn plugin(
    app: &mut App,
    store: ObjectStore,
    loaded_generation: Option<u64>,
    manifest_path: PathBuf,
) {
    app.insert_resource(ObjectWorldRes::new(store, loaded_generation));
    app.insert_resource(ObjectUiState {
        kind: ObjectKind::CampfireBasic,
        yaw_quarters: 0,
        selected: None,
        moving: None,
    });
    app.insert_resource(ObjectManifestPath(manifest_path));
    app.insert_resource(ObjectAssetCatalog::default());
    app.insert_resource(ObjectVisualState {
        last_revision: u64::MAX,
        last_ready_count: usize::MAX,
        roots: Vec::new(),
    });
    app.add_systems(Startup, setup_object_runtime);
    app.add_systems(
        Update,
        (
            object_input_system,
            object_gizmo_system,
            sync_object_visuals,
            update_object_status,
        )
            .chain()
            .run_if(in_state(GameState::Ready)),
    );
}

fn setup_object_runtime(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    manifest_path: Res<ObjectManifestPath>,
    mut catalog: ResMut<ObjectAssetCatalog>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.25, 0.75, 0.95, 0.55),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.95,
        ..default()
    });
    commands.insert_resource(PlaceholderAssets { mesh, material });

    match AssetManifest::load(&manifest_path.0) {
        Ok(manifest) => {
            for kind in ObjectKind::ALL {
                if let Some(spec) = manifest
                    .entries
                    .iter()
                    .find(|entry| entry.id == kind.asset_id())
                {
                    catalog
                        .handles
                        .insert(kind, asset_server.load(spec.path.clone()));
                } else {
                    warn!(
                        "R4 object type {} has no R3 manifest asset id {}",
                        kind.type_id(),
                        kind.asset_id()
                    );
                }
            }
            catalog.manifest_loaded = true;
        }
        Err(error) => {
            warn!(
                "R4 local asset manifest unavailable (using placeholders): {}: {error}",
                manifest_path.0.display()
            );
        }
    }

    commands.spawn((
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(13.0),
            ..default()
        },
        TextColor(Color::srgb(0.95, 0.9, 0.72)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(49.0),
            left: Val::Px(10.0),
            ..default()
        },
        ObjectStatusText,
    ));
}

fn object_input_system(
    keys: Res<ButtonInput<KeyCode>>,
    pick: Res<CurrentPickRes>,
    current_ray: Res<CurrentRayRes>,
    world_res: Res<WorldRes>,
    buildings: Option<Res<BuildingWorldRes>>,
    mut objects: ResMut<ObjectWorldRes>,
    mut ui: ResMut<ObjectUiState>,
) {
    let Some(world) = world_res.0.as_ref() else {
        return;
    };

    if keys.just_pressed(KeyCode::Digit1) {
        ui.kind = ObjectKind::CampfireBasic;
        objects.last_message = "Object type: campfire_basic".into();
    }
    if keys.just_pressed(KeyCode::Digit2) {
        ui.kind = ObjectKind::TentBasic;
        objects.last_message = "Object type: tent_basic".into();
    }
    if keys.just_pressed(KeyCode::Digit3) {
        ui.kind = ObjectKind::StonePileSmall;
        objects.last_message = "Object type: stone_pile_small".into();
    }
    if keys.just_pressed(KeyCode::KeyG) {
        ui.yaw_quarters = (ui.yaw_quarters + 1) % 4;
        objects.last_message = format!("Object yaw: {}°", ui.yaw_quarters as u16 * 90);
    }
    if keys.just_pressed(KeyCode::Escape) {
        ui.moving = None;
        objects.last_message = "Object move cancelled".into();
    }

    if keys.just_pressed(KeyCode::KeyO) {
        ui.selected = current_ray.0.and_then(|ray| {
            let terrain_t = pick.0.map(|hit| hit.t).unwrap_or(400.0);
            let building_t = buildings
                .as_ref()
                .and_then(|buildings| buildings.store.pick(&ray, 400.0).map(|(_, t)| t))
                .unwrap_or(400.0);
            objects
                .store
                .pick(&ray, terrain_t.min(building_t) + 0.05)
                .map(|(id, _)| id)
        });
        objects.last_message = ui
            .selected
            .map(|id| format!("Selected object {id}"))
            .unwrap_or_else(|| "No object under cursor".into());
    }

    if keys.just_pressed(KeyCode::KeyM) {
        if let Some(id) = ui.selected {
            ui.moving = Some(id);
            if let Some(record) = objects.store.get(id) {
                ui.kind = record.kind;
                ui.yaw_quarters = record.yaw_quarters;
            }
            objects.last_message = format!("Move object {id}: choose new ground and press P");
        }
    }

    if keys.just_pressed(KeyCode::KeyX) {
        if let Some(id) = ui.selected {
            if objects.store.remove(id).is_some() {
                objects.dirty = true;
                objects.last_message = format!("Deleted object {id}");
                ui.selected = None;
                ui.moving = None;
            }
        }
    }

    if keys.just_pressed(KeyCode::KeyP) {
        let Some(hit) = pick.0 else {
            objects.last_message = "Place object: no terrain selected".into();
            return;
        };
        let anchor = hit.place;
        if let Some(id) = ui.moving {
            let Some(old) = objects.store.get(id).copied() else {
                objects.last_message = format!("Move blocked: object {id} is missing");
                return;
            };
            let candidate = PlacedObject {
                anchor,
                yaw_quarters: ui.yaw_quarters,
                ..old
            };
            let building_error = buildings
                .as_ref()
                .and_then(|buildings| buildings.store.validate_object_record(candidate).err());
            if let Some(error) = building_error {
                objects.last_message = format!("Move blocked by building: {error}");
            } else {
                match objects
                    .store
                    .move_object(world, id, anchor, ui.yaw_quarters)
                {
                    Ok(()) => {
                        objects.dirty = true;
                        objects.last_message = format!("Moved object {id} to {anchor}");
                        ui.selected = Some(id);
                        ui.moving = None;
                    }
                    Err(error) => objects.last_message = format!("Move blocked: {error}"),
                }
            }
        } else {
            let candidate = PlacedObject {
                id: ObjectId(u64::MAX),
                kind: ui.kind,
                anchor,
                yaw_quarters: ui.yaw_quarters,
            };
            let building_error = buildings
                .as_ref()
                .and_then(|buildings| buildings.store.validate_object_record(candidate).err());
            if let Some(error) = building_error {
                objects.last_message = format!("Place blocked by building: {error}");
            } else {
                match objects.store.place(world, ui.kind, anchor, ui.yaw_quarters) {
                    Ok(id) => {
                        objects.dirty = true;
                        objects.last_message =
                            format!("Placed {} as object {id}", ui.kind.type_id());
                        ui.selected = Some(id);
                    }
                    Err(error) => objects.last_message = format!("Place blocked: {error}"),
                }
            }
        }
    }
}

fn object_gizmo_system(
    pick: Res<CurrentPickRes>,
    world_res: Res<WorldRes>,
    buildings: Option<Res<BuildingWorldRes>>,
    objects: Res<ObjectWorldRes>,
    ui: Res<ObjectUiState>,
    mut gizmos: Gizmos,
) {
    let Some(world) = world_res.0.as_ref() else {
        return;
    };
    if let Some(id) = ui.selected {
        if let Some(record) = objects.store.get(id).copied() {
            draw_record_gizmo(&mut gizmos, record, Color::srgb(1.0, 0.65, 0.1));
        }
    }
    let Some(hit) = pick.0 else {
        return;
    };
    let ignore = ui.moving;
    let preview = PlacedObject {
        id: ignore.unwrap_or(ObjectId(u64::MAX)),
        kind: ui.kind,
        anchor: hit.place,
        yaw_quarters: ui.yaw_quarters,
    };
    let valid = objects
        .store
        .validate_candidate(world, ui.kind, hit.place, ui.yaw_quarters, ignore)
        .is_ok()
        && buildings
            .as_ref()
            .map(|buildings| buildings.store.validate_object_record(preview).is_ok())
            .unwrap_or(true);
    draw_record_gizmo(
        &mut gizmos,
        preview,
        if valid {
            Color::srgb(0.2, 1.0, 0.35)
        } else {
            Color::srgb(1.0, 0.2, 0.2)
        },
    );
}

fn draw_record_gizmo(gizmos: &mut Gizmos, record: PlacedObject, color: Color) {
    let footprint = record.rotated_footprint();
    let size = Vec3::new(
        footprint.x as f32,
        record.kind.clearance() as f32,
        footprint.y as f32,
    );
    let center = record.anchor.as_vec3() + size * 0.5;
    gizmos.cube(
        Transform::from_translation(center).with_scale(size * 1.01),
        color,
    );
}

fn sync_object_visuals(
    mut commands: Commands,
    objects: Res<ObjectWorldRes>,
    catalog: Res<ObjectAssetCatalog>,
    gltfs: Res<Assets<Gltf>>,
    placeholders: Option<Res<PlaceholderAssets>>,
    mut state: ResMut<ObjectVisualState>,
) {
    let ready_count = catalog
        .handles
        .values()
        .filter(|handle| gltfs.get(*handle).is_some())
        .count();
    if state.last_revision == objects.store.revision() && state.last_ready_count == ready_count {
        return;
    }
    for entity in state.roots.drain(..) {
        commands.entity(entity).despawn();
    }

    for record in objects.store.records_sorted() {
        let footprint = record.rotated_footprint();
        let center = Vec3::new(
            record.anchor.x as f32 + footprint.x as f32 * 0.5,
            record.anchor.y as f32,
            record.anchor.z as f32 + footprint.y as f32 * 0.5,
        );
        let root = commands
            .spawn((
                Transform::from_translation(center).with_rotation(Quat::from_rotation_y(
                    record.yaw_quarters as f32 * std::f32::consts::FRAC_PI_2,
                )),
                Visibility::Inherited,
                ObjectVisualRoot,
                Name::new(format!("Object:{}:{}", record.id, record.kind.type_id())),
            ))
            .id();
        let mut actual_scene = false;
        if let Some(handle) = catalog.handles.get(&record.kind) {
            if let Some(gltf) = gltfs.get(handle) {
                if let Some(scene) = gltf
                    .default_scene
                    .clone()
                    .or_else(|| gltf.scenes.first().cloned())
                {
                    commands.spawn((
                        WorldAssetRoot(scene),
                        Transform::from_scale(Vec3::splat(record.kind.visual_scale())),
                        ChildOf(root),
                    ));
                    actual_scene = true;
                }
            }
        }
        if !actual_scene {
            if let Some(placeholders) = placeholders.as_ref() {
                let size = Vec3::new(
                    footprint.x as f32,
                    record.kind.clearance() as f32,
                    footprint.y as f32,
                );
                commands.spawn((
                    Mesh3d(placeholders.mesh.clone()),
                    MeshMaterial3d(placeholders.material.clone()),
                    Transform::from_xyz(0.0, size.y * 0.5, 0.0).with_scale(size * 0.92),
                    ChildOf(root),
                ));
            }
        }
        state.roots.push(root);
    }
    state.last_revision = objects.store.revision();
    state.last_ready_count = ready_count;
}

fn update_object_status(
    objects: Res<ObjectWorldRes>,
    ui: Res<ObjectUiState>,
    mut text: Query<&mut Text, With<ObjectStatusText>>,
) {
    let Ok(mut text) = text.single_mut() else {
        return;
    };
    let state = if objects.dirty { "DIRTY" } else { "SAVED" };
    let generation = objects
        .last_generation
        .map(|generation| generation.to_string())
        .unwrap_or_else(|| "-".into());
    let selected = ui
        .selected
        .map(|id| id.to_string())
        .unwrap_or_else(|| "-".into());
    **text = format!(
        "Objects {state} · count {} · gen {generation} · type {} · yaw {}° · selected {selected} · 1/2/3 type · G rotate · P place · O select · M move · X delete\n{}",
        objects.store.len(),
        ui.kind.type_id(),
        ui.yaw_quarters as u16 * 90,
        objects.last_message
    );
}
