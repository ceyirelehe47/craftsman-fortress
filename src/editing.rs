//! R2 交互编辑与手动存档。
//!
//! - Delete：删除当前命中体素；
//! - B：在命中面外侧放置 Stone；
//! - F5：保存到配置的双槽逻辑路径。

use crate::app_state::GameState;
use crate::camera::WorldRes;
use crate::persistence;
use crate::picking::CurrentPickRes;
use crate::voxel::BlockId;
use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;
use bevy::text::FontSize;
use std::path::PathBuf;

#[derive(Resource, Debug)]
pub struct SaveRuntime {
    pub logical_path: PathBuf,
    pub dirty: bool,
    pub last_generation: Option<u64>,
    pub last_message: String,
}

impl SaveRuntime {
    pub fn new(logical_path: PathBuf, loaded_generation: Option<u64>) -> Self {
        let last_message = loaded_generation
            .map(|g| format!("Loaded generation {g}"))
            .unwrap_or_else(|| "New world".into());
        Self {
            logical_path,
            dirty: false,
            last_generation: loaded_generation,
            last_message,
        }
    }
}

#[derive(Component)]
struct SaveStatusText;

pub fn plugin(app: &mut App, logical_path: PathBuf, loaded_generation: Option<u64>) {
    app.insert_resource(SaveRuntime::new(logical_path, loaded_generation));
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

fn edit_and_save_input(
    keys: Res<ButtonInput<KeyCode>>,
    pick: Res<CurrentPickRes>,
    mut world_res: ResMut<WorldRes>,
    mut save: ResMut<SaveRuntime>,
) {
    let Some(world) = world_res.0.as_mut() else {
        return;
    };

    if keys.just_pressed(KeyCode::Delete) {
        match pick.0 {
            Some(hit) => match world.try_user_edit(hit.voxel, BlockId::Air) {
                Ok(true) => {
                    save.dirty = true;
                    save.last_message = format!("Removed voxel {}", hit.voxel);
                }
                Ok(false) => save.last_message = "Remove: no change".into(),
                Err(e) => save.last_message = format!("Remove blocked: {e}"),
            },
            None => save.last_message = "Remove: no voxel selected".into(),
        }
    }

    if keys.just_pressed(KeyCode::KeyB) {
        match pick.0 {
            Some(hit) => match world.try_user_edit(hit.place, BlockId::Stone) {
                Ok(true) => {
                    save.dirty = true;
                    save.last_message = format!("Placed Stone at {}", hit.place);
                }
                Ok(false) => save.last_message = "Place: no change".into(),
                Err(e) => save.last_message = format!("Place blocked: {e}"),
            },
            None => save.last_message = "Place: no voxel selected".into(),
        }
    }

    if keys.just_pressed(KeyCode::F5) {
        match persistence::save_atomic(&save.logical_path, world) {
            Ok(receipt) => {
                save.dirty = false;
                save.last_generation = Some(receipt.generation);
                save.last_message = format!(
                    "Saved gen {} · {} edits · {} bytes",
                    receipt.generation, receipt.edit_count, receipt.bytes
                );
                info!(
                    "存档成功：{} generation={} edits={} hash={:#x}",
                    receipt.slot_path.display(),
                    receipt.generation,
                    receipt.edit_count,
                    receipt.world_semantic_hash
                );
            }
            Err(e) => {
                save.last_message = format!("Save failed: {e}");
                error!("存档失败：{e}");
            }
        }
    }
}

fn update_save_status(
    world_res: Res<WorldRes>,
    save: Res<SaveRuntime>,
    mut text: Query<&mut Text, With<SaveStatusText>>,
) {
    let Some(world) = world_res.0.as_ref() else {
        return;
    };
    let Ok(mut text) = text.single_mut() else {
        return;
    };
    let state = if save.dirty { "DIRTY" } else { "SAVED" };
    let generation = save
        .last_generation
        .map(|g| g.to_string())
        .unwrap_or_else(|| "-".into());
    **text = format!(
        "World {state} · edits {} · gen {generation} · Delete remove · B place Stone · F5 save\n{} · {}",
        world.modification_count(),
        save.logical_path.display(),
        save.last_message
    );
}
