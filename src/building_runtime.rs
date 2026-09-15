//! R5 建筑构件的 ECS 表现与交互。
//!
//! `BuildingStore` 是权威数据；ECS Entity 和程序化 Mesh 仅为可重建表现。

use crate::app_state::GameState;
use crate::buildings::{
    visual_boxes, BuildingId, BuildingKind, BuildingStore, PlacedBuilding, STORY_HEIGHT,
};
use crate::camera::WorldRes;
use crate::object_runtime::ObjectWorldRes;
use crate::picking::{CurrentPickRes, CurrentRayRes};
use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;
use bevy::text::FontSize;

#[derive(Resource, Debug)]
pub struct BuildingWorldRes {
    pub store: BuildingStore,
    pub dirty: bool,
    pub last_generation: Option<u64>,
    pub last_message: String,
}

impl BuildingWorldRes {
    pub fn new(store: BuildingStore, loaded_generation: Option<u64>) -> Self {
        let last_message = loaded_generation
            .map(|generation| format!("Loaded building generation {generation}"))
            .unwrap_or_else(|| "Building layer ready".into());
        Self {
            store,
            dirty: false,
            last_generation: loaded_generation,
            last_message,
        }
    }
}

#[derive(Resource)]
struct BuildingUiState {
    kind: BuildingKind,
    yaw_quarters: u8,
    selected: Option<BuildingId>,
    moving: Option<BuildingId>,
    level_offset: i32,
}

#[derive(Resource)]
struct BuildingAssets {
    cube: Handle<Mesh>,
    wood: Handle<StandardMaterial>,
    accent: Handle<StandardMaterial>,
    glass: Handle<StandardMaterial>,
    roof: Handle<StandardMaterial>,
}

#[derive(Resource)]
struct BuildingVisualState {
    last_revision: u64,
    roots: Vec<Entity>,
}

#[derive(Component)]
struct BuildingVisualRoot;

#[derive(Component)]
struct BuildingStatusText;

pub fn plugin(app: &mut App, store: BuildingStore, loaded_generation: Option<u64>) {
    app.insert_resource(BuildingWorldRes::new(store, loaded_generation));
    app.insert_resource(BuildingUiState {
        kind: BuildingKind::FloorBasic,
        yaw_quarters: 0,
        selected: None,
        moving: None,
        level_offset: 0,
    });
    app.insert_resource(BuildingVisualState {
        last_revision: u64::MAX,
        roots: Vec::new(),
    });
    app.add_systems(Startup, setup_building_runtime);
    app.add_systems(
        Update,
        (
            building_input_system,
            building_gizmo_system,
            sync_building_visuals,
            update_building_status,
        )
            .chain()
            .run_if(in_state(GameState::Ready)),
    );
}

fn setup_building_runtime(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
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
    commands.insert_resource(BuildingAssets {
        cube,
        wood,
        accent,
        glass,
        roof,
    });
    commands.spawn((
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(13.0),
            ..default()
        },
        TextColor(Color::srgb(0.86, 0.96, 0.82)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(70.0),
            left: Val::Px(10.0),
            ..default()
        },
        BuildingStatusText,
    ));
}

fn building_input_system(
    keys: Res<ButtonInput<KeyCode>>,
    pick: Res<CurrentPickRes>,
    current_ray: Res<CurrentRayRes>,
    world_res: Res<WorldRes>,
    objects: Res<ObjectWorldRes>,
    mut buildings: ResMut<BuildingWorldRes>,
    mut ui: ResMut<BuildingUiState>,
) {
    let Some(world) = world_res.0.as_ref() else {
        return;
    };

    for (key, kind) in [
        (KeyCode::F1, BuildingKind::FloorBasic),
        (KeyCode::F2, BuildingKind::WallBasic),
        (KeyCode::F3, BuildingKind::DoorwayBasic),
        (KeyCode::F4, BuildingKind::WindowBasic),
        (KeyCode::F6, BuildingKind::StairBasic),
        (KeyCode::F7, BuildingKind::RoofFlatBasic),
        (KeyCode::F8, BuildingKind::ColumnBasic),
        (KeyCode::F9, BuildingKind::BeamBasic),
    ] {
        if keys.just_pressed(key) {
            ui.kind = kind;
            buildings.last_message = format!("Building type: {}", kind.type_id());
        }
    }
    if keys.just_pressed(KeyCode::KeyH) {
        ui.yaw_quarters = (ui.yaw_quarters + 1) % 4;
        buildings.last_message = format!("Building yaw: {}°", ui.yaw_quarters as u16 * 90);
    }
    if keys.just_pressed(KeyCode::PageUp) {
        ui.level_offset = (ui.level_offset + STORY_HEIGHT).min(STORY_HEIGHT * 8);
        buildings.last_message = format!("Build level offset: {}m", ui.level_offset);
    }
    if keys.just_pressed(KeyCode::PageDown) {
        ui.level_offset = (ui.level_offset - STORY_HEIGHT).max(-STORY_HEIGHT * 8);
        buildings.last_message = format!("Build level offset: {}m", ui.level_offset);
    }
    if keys.just_pressed(KeyCode::Escape) {
        ui.moving = None;
        buildings.last_message = "Building move cancelled".into();
    }

    if keys.just_pressed(KeyCode::KeyK) {
        ui.selected = current_ray.0.and_then(|ray| {
            let terrain_t = pick.0.map(|hit| hit.t).unwrap_or(400.0);
            let object_t = objects
                .store
                .pick(&ray, 400.0)
                .map(|(_, t)| t)
                .unwrap_or(400.0);
            buildings
                .store
                .pick(&ray, terrain_t.min(object_t) + 0.05)
                .map(|(id, _)| id)
        });
        buildings.last_message = ui
            .selected
            .map(|id| format!("Selected building {id}"))
            .unwrap_or_else(|| "No building under cursor".into());
    }

    if keys.just_pressed(KeyCode::KeyL) {
        if let Some(id) = ui.selected {
            ui.moving = Some(id);
            if let Some(record) = buildings.store.get(id) {
                ui.kind = record.kind;
                ui.yaw_quarters = record.yaw_quarters;
            }
            buildings.last_message = format!("Move building {id}: choose anchor and press J");
        }
    }

    if keys.just_pressed(KeyCode::Backspace) {
        if let Some(id) = ui.selected {
            match buildings.store.remove(world, &objects.store, id) {
                Ok(_) => {
                    buildings.dirty = true;
                    buildings.last_message = format!("Deleted building {id}");
                    ui.selected = None;
                    ui.moving = None;
                }
                Err(error) => {
                    buildings.last_message = format!("Delete blocked: {error}");
                }
            }
        }
    }

    if keys.just_pressed(KeyCode::KeyJ) {
        let Some(hit) = pick.0 else {
            buildings.last_message = "Place building: no terrain selected".into();
            return;
        };
        let anchor = hit.place + IVec3::Y * ui.level_offset;
        if let Some(id) = ui.moving {
            match buildings
                .store
                .move_component(world, &objects.store, id, anchor, ui.yaw_quarters)
            {
                Ok(()) => {
                    buildings.dirty = true;
                    buildings.last_message = format!("Moved building {id} to {anchor}");
                    ui.selected = Some(id);
                    ui.moving = None;
                }
                Err(error) => buildings.last_message = format!("Move blocked: {error}"),
            }
        } else {
            match buildings
                .store
                .place(world, &objects.store, ui.kind, anchor, ui.yaw_quarters)
            {
                Ok(id) => {
                    buildings.dirty = true;
                    buildings.last_message =
                        format!("Placed {} as building {id}", ui.kind.type_id());
                    ui.selected = Some(id);
                }
                Err(error) => buildings.last_message = format!("Place blocked: {error}"),
            }
        }
    }
}

fn building_gizmo_system(
    pick: Res<CurrentPickRes>,
    world_res: Res<WorldRes>,
    objects: Res<ObjectWorldRes>,
    buildings: Res<BuildingWorldRes>,
    ui: Res<BuildingUiState>,
    mut gizmos: Gizmos,
) {
    let Some(world) = world_res.0.as_ref() else {
        return;
    };
    if let Some(id) = ui.selected {
        if let Some(record) = buildings.store.get(id).copied() {
            draw_record_gizmo(&mut gizmos, record, Color::srgb(1.0, 0.65, 0.1));
        }
    }
    let Some(hit) = pick.0 else {
        return;
    };
    let anchor = hit.place + IVec3::Y * ui.level_offset;
    let valid = buildings
        .store
        .validate_candidate(
            world,
            &objects.store,
            ui.kind,
            anchor,
            ui.yaw_quarters,
            ui.moving,
        )
        .is_ok();
    let preview = PlacedBuilding {
        id: ui.moving.unwrap_or(BuildingId(u64::MAX)),
        kind: ui.kind,
        anchor,
        yaw_quarters: ui.yaw_quarters,
    };
    draw_record_gizmo(
        &mut gizmos,
        preview,
        if valid {
            Color::srgb(0.25, 1.0, 0.35)
        } else {
            Color::srgb(1.0, 0.2, 0.2)
        },
    );
}

fn draw_record_gizmo(gizmos: &mut Gizmos, record: PlacedBuilding, color: Color) {
    for (min, max) in visual_boxes(record) {
        let size = max - min;
        let center = (min + max) * 0.5;
        gizmos.cube(
            Transform::from_translation(center).with_scale(size * 1.02),
            color,
        );
    }
}

fn sync_building_visuals(
    mut commands: Commands,
    buildings: Res<BuildingWorldRes>,
    assets: Option<Res<BuildingAssets>>,
    mut state: ResMut<BuildingVisualState>,
) {
    let Some(assets) = assets else {
        return;
    };
    if state.last_revision == buildings.store.revision() {
        return;
    }
    for entity in state.roots.drain(..) {
        commands.entity(entity).despawn();
    }
    for record in buildings.store.records_sorted() {
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
                BuildingVisualRoot,
                Name::new(format!("Building:{}:{}", record.id, record.kind.type_id())),
            ))
            .id();
        spawn_component_children(&mut commands, root, record.kind, &assets);
        state.roots.push(root);
    }
    state.last_revision = buildings.store.revision();
}

fn spawn_component_children(
    commands: &mut Commands,
    root: Entity,
    kind: BuildingKind,
    assets: &BuildingAssets,
) {
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
            spawn_box(
                commands,
                root,
                assets,
                assets.accent.clone(),
                Vec3::new(0.08, 1.5, 0.0),
                Vec3::new(0.16, 3.0, 0.14),
            );
            spawn_box(
                commands,
                root,
                assets,
                assets.accent.clone(),
                Vec3::new(0.92, 1.5, 0.0),
                Vec3::new(0.16, 3.0, 0.14),
            );
            spawn_box(
                commands,
                root,
                assets,
                assets.accent.clone(),
                Vec3::new(0.5, 2.82, 0.0),
                Vec3::new(0.84, 0.36, 0.14),
            );
        }
        BuildingKind::WindowBasic => {
            for center in [Vec3::new(0.08, 1.5, 0.0), Vec3::new(0.92, 1.5, 0.0)] {
                spawn_box(
                    commands,
                    root,
                    assets,
                    assets.accent.clone(),
                    center,
                    Vec3::new(0.16, 3.0, 0.14),
                );
            }
            for center in [Vec3::new(0.5, 0.48, 0.0), Vec3::new(0.5, 2.52, 0.0)] {
                spawn_box(
                    commands,
                    root,
                    assets,
                    assets.accent.clone(),
                    center,
                    Vec3::new(0.84, 0.24, 0.14),
                );
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
    assets: &BuildingAssets,
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

fn update_building_status(
    buildings: Res<BuildingWorldRes>,
    ui: Res<BuildingUiState>,
    mut text: Query<&mut Text, With<BuildingStatusText>>,
) {
    let Ok(mut text) = text.single_mut() else {
        return;
    };
    let state = if buildings.dirty { "DIRTY" } else { "SAVED" };
    let generation = buildings
        .last_generation
        .map(|generation| generation.to_string())
        .unwrap_or_else(|| "-".into());
    let selected = ui
        .selected
        .map(|id| id.to_string())
        .unwrap_or_else(|| "-".into());
    **text = format!(
        "Buildings {state} · count {} · gen {generation} · type {} · yaw {}° · level {:+}m · selected {selected} · F1-F4/F6-F9 type · H rotate · J place · K select · L move · Backspace delete\n{}",
        buildings.store.len(),
        ui.kind.type_id(),
        ui.yaw_quarters as u16 * 90,
        ui.level_offset,
        buildings.last_message
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coords::WorldSize;
    use crate::generation::TerrainParams;
    use crate::objects::ObjectStore;
    use crate::picking::{PickHit, Ray};
    use crate::voxel::{BlockId, FaceDir};
    use crate::world::World;

    fn flat_world() -> World {
        let mut world = World::empty(WorldSize::new(32, 32, 32), TerrainParams::new(88));
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

    fn terrain_hit(x: i32, z: i32) -> PickHit {
        PickHit {
            voxel: IVec3::new(x, 4, z),
            face: FaceDir::PosY,
            place: IVec3::new(x, 5, z),
            t: 5.0,
            point: Vec3::new(x as f32 + 0.5, 5.0, z as f32 + 0.5),
        }
    }

    fn press(app: &mut App, key: KeyCode) {
        let mut input = ButtonInput::<KeyCode>::default();
        input.press(key);
        app.insert_resource(input);
        app.update();
    }

    #[test]
    fn keyboard_session_place_select_move_cancel_rotate_and_delete() {
        let mut app = App::new();
        app.insert_resource(ButtonInput::<KeyCode>::default());
        app.insert_resource(CurrentPickRes(Some(terrain_hit(5, 5))));
        app.insert_resource(CurrentRayRes(Some(Ray::normalized(
            Vec3::new(5.5, 10.0, 5.5),
            -Vec3::Y,
        ))));
        app.insert_resource(WorldRes(Some(flat_world())));
        app.insert_resource(ObjectWorldRes::new(ObjectStore::new(), None));
        app.insert_resource(BuildingWorldRes::new(BuildingStore::new(), None));
        app.insert_resource(BuildingUiState {
            kind: BuildingKind::FloorBasic,
            yaw_quarters: 0,
            selected: None,
            moving: None,
            level_offset: 0,
        });
        app.add_systems(Update, building_input_system);

        press(&mut app, KeyCode::KeyJ);
        let placed = app
            .world()
            .resource::<BuildingWorldRes>()
            .store
            .records_sorted()[0];
        assert_eq!(placed.id, BuildingId(1));

        press(&mut app, KeyCode::KeyK);
        assert_eq!(
            app.world().resource::<BuildingUiState>().selected,
            Some(placed.id)
        );
        press(&mut app, KeyCode::KeyL);
        assert_eq!(
            app.world().resource::<BuildingUiState>().moving,
            Some(placed.id)
        );
        let hash_before_cancel = app
            .world()
            .resource::<BuildingWorldRes>()
            .store
            .semantic_hash();
        press(&mut app, KeyCode::Escape);
        assert!(app.world().resource::<BuildingUiState>().moving.is_none());
        assert_eq!(
            app.world()
                .resource::<BuildingWorldRes>()
                .store
                .semantic_hash(),
            hash_before_cancel
        );

        // 与 R4 对象层同构：进入移动会把朝向恢复为构件当前值，
        // 玩家在移动过程中按 H 调整新朝向，提交时生效。
        press(&mut app, KeyCode::KeyL);
        press(&mut app, KeyCode::KeyH);
        app.world_mut().resource_mut::<CurrentPickRes>().0 = Some(terrain_hit(8, 5));
        press(&mut app, KeyCode::KeyJ);
        let moved = *app
            .world()
            .resource::<BuildingWorldRes>()
            .store
            .get(placed.id)
            .unwrap();
        assert_eq!(moved.id, placed.id);
        assert_eq!(moved.anchor, IVec3::new(8, 5, 5));
        assert_eq!(moved.yaw_quarters, 1);

        press(&mut app, KeyCode::Backspace);
        assert!(app.world().resource::<BuildingWorldRes>().store.is_empty());
    }
}
