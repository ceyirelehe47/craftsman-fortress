//! R2 交互编辑与手动存档。
//!
//! - Delete：删除当前命中体素；
//! - B：在命中面外侧放置 Stone；
//! - F5：保存到配置的双槽逻辑路径。

use crate::app_state::GameState;
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
    pub dirty: bool,
    pub last_generation: Option<u64>,
    pub last_object_generation: Option<u64>,
    pub last_message: String,
}

impl SaveRuntime {
    pub fn new(
        logical_path: PathBuf,
        object_logical_path: PathBuf,
        loaded_generation: Option<u64>,
    ) -> Self {
        let last_message = loaded_generation
            .map(|g| format!("Loaded generation {g}"))
            .unwrap_or_else(|| "New world".into());
        Self {
            logical_path,
            object_logical_path,
            dirty: false,
            last_generation: loaded_generation,
            last_object_generation: None,
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
    loaded_generation: Option<u64>,
) {
    app.insert_resource(SaveRuntime::new(
        logical_path,
        object_logical_path,
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
    hit: Option<PickHit>,
    action: EditAction,
    save: &mut SaveRuntime,
) {
    match apply_edit_action(world, objects, hit, action) {
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
        Err(e) => {
            save.last_message = match action {
                EditAction::Remove => format!("Remove blocked: {e}"),
                EditAction::PlaceStone => format!("Place blocked: {e}"),
            };
        }
    }
}

fn edit_and_save_input(
    keys: Res<ButtonInput<KeyCode>>,
    pick: Res<CurrentPickRes>,
    mut world_res: ResMut<WorldRes>,
    mut objects: ResMut<ObjectWorldRes>,
    mut save: ResMut<SaveRuntime>,
) {
    let Some(world) = world_res.0.as_mut() else {
        return;
    };

    if keys.just_pressed(KeyCode::Delete) {
        apply_action_and_update_status(
            world,
            &objects.store,
            pick.0,
            EditAction::Remove,
            &mut save,
        );
    }

    if keys.just_pressed(KeyCode::KeyB) {
        apply_action_and_update_status(
            world,
            &objects.store,
            pick.0,
            EditAction::PlaceStone,
            &mut save,
        );
    }

    if keys.just_pressed(KeyCode::F5) {
        // 对象先写入未受保护槽；只有随后地形保存成功，该对象槽才会与最新地形哈希匹配。
        // 当前磁盘地形对应的完整有效对象槽始终受保护，因此中途失败可回退。
        let durable_world = persistence::load_latest(&save.logical_path)
            .ok()
            .map(|loaded| loaded.world);
        match object_persistence::save_atomic(
            &save.object_logical_path,
            world,
            &objects.store,
            durable_world.as_ref(),
        ) {
            Ok(object_receipt) => match persistence::save_atomic(&save.logical_path, world) {
                Ok(receipt) => {
                    save.dirty = false;
                    objects.dirty = false;
                    save.last_generation = Some(receipt.generation);
                    save.last_object_generation = Some(object_receipt.generation);
                    objects.last_generation = Some(object_receipt.generation);
                    save.last_message = format!(
                        "Saved terrain gen {} / object gen {} · {} edits · {} objects",
                        receipt.generation,
                        object_receipt.generation,
                        receipt.edit_count,
                        object_receipt.object_count
                    );
                    objects.last_message = save.last_message.clone();
                    info!(
                        "会话存档成功：terrain={} object={} terrain_hash={:#x} object_hash={:#x}",
                        receipt.slot_path.display(),
                        object_receipt.slot_path.display(),
                        receipt.world_semantic_hash,
                        object_receipt.object_semantic_hash
                    );
                }
                Err(error) => {
                    save.last_message =
                        format!("Terrain save failed after protected object prewrite: {error}");
                    error!("地形存档失败（旧匹配对象槽已保留）：{error}");
                }
            },
            Err(error) => {
                save.last_message = format!("Object save failed: {error}");
                error!("对象存档失败；未写入地形：{error}");
            }
        }
    }
}

fn update_save_status(
    world_res: Res<WorldRes>,
    save: Res<SaveRuntime>,
    objects: Res<ObjectWorldRes>,
    mut text: Query<&mut Text, With<SaveStatusText>>,
) {
    let Some(world) = world_res.0.as_ref() else {
        return;
    };
    let Ok(mut text) = text.single_mut() else {
        return;
    };
    let state = if save.dirty || objects.dirty {
        "DIRTY"
    } else {
        "SAVED"
    };
    let generation = save
        .last_generation
        .map(|g| g.to_string())
        .unwrap_or_else(|| "-".into());
    let object_generation = save
        .last_object_generation
        .map(|generation| generation.to_string())
        .unwrap_or_else(|| "-".into());
    **text = format!(
        "Session {state} · terrain edits {} · objects {} · terrain gen {generation} · object gen {object_generation} · F5 save\n{} + {} · {}",
        world.modification_count(),
        objects.store.len(),
        save.logical_path.display(),
        save.object_logical_path.display(),
        save.last_message
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coords::WorldSize;
    use crate::generation::TerrainParams;

    #[test]
    fn no_selection_edit_is_a_true_noop() {
        let mut world = World::generate_all(WorldSize::new(32, 32, 32), TerrainParams::new(7));
        let hash = world.semantic_hash();
        let edits = world.modification_count();

        assert_eq!(
            apply_edit_action(&mut world, &ObjectStore::new(), None, EditAction::Remove,).unwrap(),
            EditOutcome::NoSelection
        );
        assert_eq!(
            apply_edit_action(
                &mut world,
                &ObjectStore::new(),
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
}
