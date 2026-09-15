//! R2/R4/R5 交互编辑与三层伴随存档。
//!
//! - Delete：删除当前命中体素；
//! - B：在命中面外侧放置 Stone；
//! - F5：先预写建筑、再预写对象、最后提交地形，保留可恢复的旧三层配对。

use crate::app_state::GameState;
use crate::building_persistence;
use crate::building_runtime::BuildingWorldRes;
use crate::buildings::BuildingStore;
use crate::camera::WorldRes;
use crate::object_persistence;
use crate::object_runtime::ObjectWorldRes;
use crate::objects::ObjectStore;
use crate::persistence;
use crate::picking::{CurrentPickRes, PickHit};
use crate::voxel::BlockId;
use crate::world::World;
use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;
use bevy::text::FontSize;
use std::path::PathBuf;

#[derive(Resource, Debug)]
pub struct SaveRuntime {
    pub logical_path: PathBuf,
    pub object_logical_path: PathBuf,
    pub building_logical_path: PathBuf,
    pub dirty: bool,
    pub last_generation: Option<u64>,
    pub last_object_generation: Option<u64>,
    pub last_building_generation: Option<u64>,
    pub last_message: String,
}

impl SaveRuntime {
    pub fn new(
        logical_path: PathBuf,
        object_logical_path: PathBuf,
        building_logical_path: PathBuf,
        loaded_generation: Option<u64>,
    ) -> Self {
        let last_message = loaded_generation
            .map(|generation| format!("Loaded generation {generation}"))
            .unwrap_or_else(|| "New world".into());
        Self {
            logical_path,
            object_logical_path,
            building_logical_path,
            dirty: false,
            last_generation: loaded_generation,
            last_object_generation: None,
            last_building_generation: None,
            last_message,
        }
    }
}

#[derive(Component)]
struct SaveStatusText;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditAction {
    Remove,
    PlaceStone,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditOutcome {
    NoSelection,
    NoChange,
    Changed { voxel: IVec3, block: BlockId },
}

fn apply_edit_action(
    world: &mut World,
    objects: &ObjectStore,
    buildings: &BuildingStore,
    hit: Option<PickHit>,
    action: EditAction,
) -> Result<EditOutcome, String> {
    let Some(hit) = hit else {
        return Ok(EditOutcome::NoSelection);
    };
    let (voxel, block) = match action {
        EditAction::Remove => (hit.voxel, BlockId::Air),
        EditAction::PlaceStone => (hit.place, BlockId::Stone),
    };
    objects
        .validate_terrain_edit(voxel, block)
        .map_err(|error| error.to_string())?;
    buildings
        .validate_terrain_edit(world, objects, voxel, block)
        .map_err(|error| error.to_string())?;
    if world
        .try_user_edit(voxel, block)
        .map_err(|error| error.to_string())?
    {
        Ok(EditOutcome::Changed { voxel, block })
    } else {
        Ok(EditOutcome::NoChange)
    }
}

pub fn plugin(
    app: &mut App,
    logical_path: PathBuf,
    object_logical_path: PathBuf,
    building_logical_path: PathBuf,
    loaded_generation: Option<u64>,
) {
    app.insert_resource(SaveRuntime::new(
        logical_path,
        object_logical_path,
        building_logical_path,
        loaded_generation,
    ));
    app.add_systems(Startup, spawn_save_status);
    app.add_systems(
        Update,
        (edit_and_save_input, update_save_status)
            .chain()
            .run_if(in_state(GameState::Ready)),
    );
}

fn spawn_save_status(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(13.0),
            ..Default::default()
        },
        TextColor(Color::srgb(0.9, 0.94, 0.98)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(28.0),
            left: Val::Px(10.0),
            ..Default::default()
        },
        SaveStatusText,
    ));
}

fn apply_action_and_update_status(
    world: &mut World,
    objects: &ObjectStore,
    buildings: &BuildingStore,
    hit: Option<PickHit>,
    action: EditAction,
    save: &mut SaveRuntime,
) {
    match apply_edit_action(world, objects, buildings, hit, action) {
        Ok(EditOutcome::Changed { voxel, block }) => {
            save.dirty = true;
            save.last_message = if block == BlockId::Air {
                format!("Removed voxel {voxel}")
            } else {
                format!("Placed Stone at {voxel}")
            };
        }
        Ok(EditOutcome::NoChange) => {
            save.last_message = match action {
                EditAction::Remove => "Remove: no change".into(),
                EditAction::PlaceStone => "Place: no change".into(),
            };
        }
        Ok(EditOutcome::NoSelection) => {
            save.last_message = match action {
                EditAction::Remove => "Remove: no voxel selected".into(),
                EditAction::PlaceStone => "Place: no voxel selected".into(),
            };
        }
        Err(error) => {
            save.last_message = match action {
                EditAction::Remove => format!("Remove blocked: {error}"),
                EditAction::PlaceStone => format!("Place blocked: {error}"),
            };
        }
    }
}

fn edit_and_save_input(
    keys: Res<ButtonInput<KeyCode>>,
    pick: Res<CurrentPickRes>,
    mut world_res: ResMut<WorldRes>,
    mut objects: ResMut<ObjectWorldRes>,
    mut buildings: ResMut<BuildingWorldRes>,
    mut save: ResMut<SaveRuntime>,
) {
    let Some(world) = world_res.0.as_mut() else {
        return;
    };

    if keys.just_pressed(KeyCode::Delete) {
        apply_action_and_update_status(
            world,
            &objects.store,
            &buildings.store,
            pick.0,
            EditAction::Remove,
            &mut save,
        );
    }

    if keys.just_pressed(KeyCode::KeyB) {
        apply_action_and_update_status(
            world,
            &objects.store,
            &buildings.store,
            pick.0,
            EditAction::PlaceStone,
            &mut save,
        );
    }

    if keys.just_pressed(KeyCode::F5) {
        // 若磁盘已有地形槽，必须先完整物化其对象层；否则无法证明旧三层配对受保护。
        let terrain_slots_exist = persistence::slot_paths(&save.logical_path)
            .iter()
            .any(|path| path.exists());
        let durable_pair: Option<(World, ObjectStore)> = if terrain_slots_exist {
            let durable = match persistence::load_latest(&save.logical_path) {
                Ok(loaded) => loaded,
                Err(error) => {
                    save.last_message = format!("Save blocked: durable terrain invalid: {error}");
                    error!("无法建立旧三层保护配对：{error}");
                    return;
                }
            };
            let durable_objects =
                match object_persistence::load_latest(&save.object_logical_path, &durable.world) {
                    Ok(Some(loaded)) => loaded.store,
                    Ok(None) => ObjectStore::new(),
                    Err(error) => {
                        save.last_message =
                            format!("Save blocked: durable objects invalid: {error}");
                        error!("旧对象伴随层无完整有效槽：{error}");
                        return;
                    }
                };
            Some((durable.world, durable_objects))
        } else {
            None
        };

        let protected_building_pair = durable_pair
            .as_ref()
            .map(|(durable_world, durable_objects)| (durable_world, durable_objects));
        let building_receipt = match building_persistence::save_atomic(
            &save.building_logical_path,
            world,
            &objects.store,
            &buildings.store,
            protected_building_pair,
        ) {
            Ok(receipt) => receipt,
            Err(error) => {
                save.last_message = format!("Building save failed: {error}");
                error!("建筑预写失败；对象和地形均未写入：{error}");
                return;
            }
        };

        let protected_world = durable_pair
            .as_ref()
            .map(|(durable_world, _)| durable_world);
        let object_receipt = match object_persistence::save_atomic(
            &save.object_logical_path,
            world,
            &objects.store,
            protected_world,
        ) {
            Ok(receipt) => receipt,
            Err(error) => {
                save.last_message =
                    format!("Object save failed after protected building prewrite: {error}");
                error!("对象预写失败；地形未提交，旧三层配对仍保留：{error}");
                return;
            }
        };

        match persistence::save_atomic(&save.logical_path, world) {
            Ok(receipt) => {
                save.dirty = false;
                objects.dirty = false;
                buildings.dirty = false;
                save.last_generation = Some(receipt.generation);
                save.last_object_generation = Some(object_receipt.generation);
                save.last_building_generation = Some(building_receipt.generation);
                objects.last_generation = Some(object_receipt.generation);
                buildings.last_generation = Some(building_receipt.generation);
                save.last_message = format!(
                    "Saved terrain {} / objects {} / buildings {} · {} edits · {} objects · {} components",
                    receipt.generation,
                    object_receipt.generation,
                    building_receipt.generation,
                    receipt.edit_count,
                    object_receipt.object_count,
                    building_receipt.component_count
                );
                objects.last_message = save.last_message.clone();
                buildings.last_message = save.last_message.clone();
                info!(
                    "会话三层存档成功：terrain={} object={} building={} terrain_hash={:#x} object_hash={:#x} building_hash={:#x}",
                    receipt.slot_path.display(),
                    object_receipt.slot_path.display(),
                    building_receipt.slot_path.display(),
                    receipt.world_semantic_hash,
                    object_receipt.object_semantic_hash,
                    building_receipt.building_semantic_hash
                );
            }
            Err(error) => {
                save.last_message =
                    format!("Terrain save failed after protected companion prewrites: {error}");
                error!("地形存档失败；旧三层配对已保留：{error}");
            }
        }
    }
}

fn update_save_status(
    world_res: Res<WorldRes>,
    save: Res<SaveRuntime>,
    objects: Res<ObjectWorldRes>,
    buildings: Res<BuildingWorldRes>,
    mut text: Query<&mut Text, With<SaveStatusText>>,
) {
    let Some(world) = world_res.0.as_ref() else {
        return;
    };
    let Ok(mut text) = text.single_mut() else {
        return;
    };
    let state = if save.dirty || objects.dirty || buildings.dirty {
        "DIRTY"
    } else {
        "SAVED"
    };
    let terrain_generation = save
        .last_generation
        .map(|generation| generation.to_string())
        .unwrap_or_else(|| "-".into());
    let object_generation = save
        .last_object_generation
        .map(|generation| generation.to_string())
        .unwrap_or_else(|| "-".into());
    let building_generation = save
        .last_building_generation
        .map(|generation| generation.to_string())
        .unwrap_or_else(|| "-".into());
    **text = format!(
        "Session {state} · terrain edits {} · objects {} · buildings {} · gen T/O/B {terrain_generation}/{object_generation}/{building_generation} · F5 save\n{} + {} + {} · {}",
        world.modification_count(),
        objects.store.len(),
        buildings.store.len(),
        save.logical_path.display(),
        save.object_logical_path.display(),
        save.building_logical_path.display(),
        save.last_message
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buildings::BuildingKind;
    use crate::coords::WorldSize;
    use crate::generation::TerrainParams;

    fn flat_world() -> World {
        let mut world = World::empty(WorldSize::new(32, 32, 32), TerrainParams::new(7));
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

    #[test]
    fn no_selection_edit_is_a_true_noop() {
        let mut world = flat_world();
        let hash = world.semantic_hash();
        let edits = world.modification_count();
        let objects = ObjectStore::new();
        let buildings = BuildingStore::new();

        assert_eq!(
            apply_edit_action(&mut world, &objects, &buildings, None, EditAction::Remove,).unwrap(),
            EditOutcome::NoSelection
        );
        assert_eq!(
            apply_edit_action(
                &mut world,
                &objects,
                &buildings,
                None,
                EditAction::PlaceStone,
            )
            .unwrap(),
            EditOutcome::NoSelection
        );
        assert_eq!(world.semantic_hash(), hash);
        assert_eq!(world.modification_count(), edits);
        assert_eq!(world.dirty_count(), 0);
    }

    #[test]
    fn terrain_edit_cannot_remove_building_support() {
        let mut world = flat_world();
        let objects = ObjectStore::new();
        let mut buildings = BuildingStore::new();
        buildings
            .place(
                &world,
                &objects,
                BuildingKind::FloorBasic,
                IVec3::new(5, 5, 5),
                0,
            )
            .unwrap();
        let hit = PickHit {
            voxel: IVec3::new(5, 4, 5),
            face: crate::voxel::FaceDir::PosY,
            place: IVec3::new(5, 5, 5),
            t: 1.0,
            point: Vec3::new(5.5, 5.0, 5.5),
        };
        assert!(apply_edit_action(
            &mut world,
            &objects,
            &buildings,
            Some(hit),
            EditAction::Remove,
        )
        .is_err());
        assert!(world.voxel(IVec3::new(5, 4, 5)).is_solid());
    }
}
