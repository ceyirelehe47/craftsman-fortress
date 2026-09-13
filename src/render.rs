//! 渲染层：Chunk Mesh 装配、材质光照、雾、调试视图与加载流程。
//!
//! 职责边界：`meshing` 产出纯数据 `ChunkMeshData`，本模块只做
//! Bevy `Mesh`/`StandardMaterial` 资源管理与实体生命周期。

use crate::app_state::GameState;
use crate::camera::WorldRes;
use crate::meshing::{build_chunk_mesh, ChunkMeshData, DebugTint};
use bevy::asset::{Assets, Handle, RenderAssetUsages};
use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::view::window::screenshot::{Screenshot, ScreenshotCaptured};

/// 主相机标记（相机求解、拾取、截图都引用它）。
#[derive(Component)]
pub struct MainCamera;

/// Chunk 网格实体标记。
#[derive(Component)]
pub struct ChunkMeshEntity {
    pub cc: IVec3,
}

/// Chunk 网格注册表：Chunk 坐标 -> (实体, Mesh 句柄)。
#[derive(Resource, Default)]
pub struct ChunkMeshes {
    pub entries: std::collections::HashMap<IVec3, (Entity, Handle<Mesh>)>,
    /// 已完成网格化但结果为空（数据全空气、或非空但被邻居完全遮挡 => 0 面，
    /// 无需绘制实体）的 Chunk 集合。供 visible-unready 诊断区分
    /// "无需网格"与"缺网格"。
    pub meshed_empty: std::collections::HashSet<IVec3>,
}

impl ChunkMeshes {
    pub fn meshed_count(&self) -> usize {
        self.entries.len()
    }

    /// 该 Chunk 的渲染准备是否完成（有实体，或确认无需实体）。
    pub fn is_ready(&self, cc: IVec3) -> bool {
        self.entries.contains_key(&cc) || self.meshed_empty.contains(&cc)
    }
}

/// 调试着色状态（F3 切换）。
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct DebugTintRes(pub DebugTint);

impl Default for DebugTintRes {
    fn default() -> Self {
        Self(DebugTint::Off)
    }
}

/// 全局材质句柄资源（单一白色底色 + 顶点色材质）。
#[derive(Resource, Default)]
pub struct StandardMaterialHandle(pub Option<Handle<StandardMaterial>>);

/// Mesh/实体资源生命周期计数（A12 资源时间序列的数据源）。
/// 只增不减；"在册实体 = spawned - despawned" 与 "Mesh 资产 = created - removed"
/// 是泄漏检测的守恒断言。
#[derive(Resource, Default, Clone, Copy)]
pub struct MeshResourceStats {
    pub mesh_created: u64,
    pub mesh_removed: u64,
    /// 句柄复用的原地内容替换（不创建新资产）。
    pub mesh_replaced: u64,
    pub entity_spawned: u64,
    pub entity_despawned: u64,
    pub material_created: u64,
}

/// 加载进度游标。
#[derive(Resource, Default)]
pub struct LoadingCursor {
    pub gen_next: usize,
    pub mesh_next: usize,
}

/// 加载文本实体标记。
#[derive(Component)]
pub struct LoadingText;

pub fn plugin(app: &mut App) {
    app.init_resource::<ChunkMeshes>();
    app.init_resource::<DebugTintRes>();
    app.init_resource::<LoadingCursor>();
    app.init_resource::<StandardMaterialHandle>();
    app.init_resource::<MeshResourceStats>();
    app.add_systems(
        OnEnter(GameState::Loading),
        (setup_scene, create_material).chain(),
    );
    app.add_systems(
        Update,
        loading_progress_system.run_if(in_state(GameState::Loading)),
    );
    app.add_systems(
        Update,
        (
            rebuild_dirty_system.run_if(in_state(GameState::Ready)),
            toggle_tint_system.run_if(in_state(GameState::Ready)),
            debug_screenshot_system.run_if(in_state(GameState::Ready)),
        ),
    );
}

/// 场景固定装置：相机、光照、雾。
fn setup_scene(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        bevy::render::view::Msaa::Off,
        Camera {
            clear_color: ClearColorConfig::Custom(sky_color()),
            ..Default::default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: 50f32.to_radians(),
            near: 0.1,
            far: 520.0,
            ..Default::default()
        }),
        Transform::from_xyz(0.0, 80.0, -120.0).looking_at(Vec3::ZERO, Vec3::Y),
        MainCamera,
        IsDefaultUiCamera,
        DistanceFog {
            color: fog_color(),
            falloff: FogFalloff::Linear {
                start: 130.0,
                end: 380.0,
            },
            // 禁用雾的"太阳辉光"散射：shader 对 dot<=0 的视线取
            // pow(0, exponent)，exponent=0 时为 WGSL 未定义行为（NaN），
            // 且 NaN 会经 mix 污染整个片元（即使雾 alpha=0）导致全黑。
            // Color::NONE（alpha=0）使 shader 跳过该分支，是官方文档指定的禁用方式。
            directional_light_color: Color::NONE,
            directional_light_exponent: 8.0,
        },
    ));

    // 光照说明：本版无独立光源——材质为 unlit，明暗来自 meshing 烘焙的
    // 面朝向系数（见 create_material 注释与决策记录"渲染/光照模型"）。
}

pub fn sky_color() -> Color {
    Color::srgb_u8(150, 178, 205)
}

pub fn fog_color() -> Color {
    Color::srgb_u8(158, 182, 206)
}

fn create_material(
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut handle: ResMut<StandardMaterialHandle>,
    mut stats: ResMut<MeshResourceStats>,
) {
    let mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        // unlit + 烘焙顶点色（meshing 按 FaceDir 乘 0.55..1.0 明暗）是本版的光照模型：
        // 方向光无阴影贴图会把背光面压成死黑、并把阳光"泄漏"进洞穴；
        // 烘焙面着色让任意朝向的面都保持调色板可读性。
        unlit: true,
        ..Default::default()
    });
    handle.0 = Some(mat);
    stats.material_created += 1;
}

/// 从纯数据装配 Bevy Mesh。
pub fn mesh_from_data(data: &ChunkMeshData) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, data.positions.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, data.normals.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, data.colors.clone());
    mesh.insert_indices(Indices::U32(data.indices.clone()));
    mesh
}

/// （重新）为一个 Chunk 构建/替换 Mesh 实体。空数据 => 移除实体。
#[allow(clippy::too_many_arguments)]
pub fn apply_chunk_mesh(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: &Handle<StandardMaterial>,
    registry: &mut ChunkMeshes,
    stats: &mut MeshResourceStats,
    cc: IVec3,
    data: &ChunkMeshData,
) {
    if data.is_empty() {
        if let Some((entity, handle)) = registry.entries.remove(&cc) {
            commands.entity(entity).despawn();
            meshes.remove(&handle);
            stats.mesh_removed += 1;
            stats.entity_despawned += 1;
        }
        // 记录"已网格化但结果为空"：0 面 chunk 无需实体，但准备状态算完成。
        registry.meshed_empty.insert(cc);
        return;
    }
    let mesh = mesh_from_data(data);
    registry.meshed_empty.remove(&cc);
    match registry.entries.get(&cc) {
        Some((_, handle)) => {
            // 句柄复用：原地替换资源内容，实体与句柄保持稳定。
            // 0.19 起 Assets::insert 返回 Result（句柄已存在时必然成功）。
            let _ = meshes.insert(handle.id(), mesh);
            stats.mesh_replaced += 1;
        }
        None => {
            let handle = meshes.add(mesh);
            let entity = commands
                .spawn((
                    Mesh3d(handle.clone()),
                    MeshMaterial3d(material.clone()),
                    ChunkMeshEntity { cc },
                ))
                .id();
            registry.entries.insert(cc, (entity, handle));
            stats.mesh_created += 1;
            stats.entity_spawned += 1;
        }
    }
}

/// 线性索引 -> Chunk 坐标（与 World 槽位排布一致：cx + cz*Wx + cy*Wx*Wz）。
fn linear_to_chunk(i: usize, n: bevy::math::UVec3) -> IVec3 {
    let cy = i / ((n.x * n.z) as usize);
    let rem = i % ((n.x * n.z) as usize);
    let cz = rem / (n.x as usize);
    let cx = rem % (n.x as usize);
    IVec3::new(cx as i32, cy as i32, cz as i32)
}

/// 加载推进：每帧限时分片生成 Chunk 与构建 Mesh，完成后切入 Ready。
#[allow(clippy::too_many_arguments)]
fn loading_progress_system(
    mut commands: Commands,
    mut world_res: ResMut<WorldRes>,
    mut meshes: ResMut<Assets<Mesh>>,
    material: Res<StandardMaterialHandle>,
    mut registry: ResMut<ChunkMeshes>,
    mut stats: ResMut<MeshResourceStats>,
    mut cursor: ResMut<LoadingCursor>,
    tint: Res<DebugTintRes>,
    mut next_state: ResMut<NextState<GameState>>,
    mut loading_text: Query<&mut Text, With<LoadingText>>,
) {
    let Some(world) = world_res.0.as_mut() else {
        return;
    };
    let frame_budget = std::time::Duration::from_millis(10);
    let start = std::time::Instant::now();
    let total = world.total_chunks();
    let n = world.size.chunks();
    let Some(material) = material.0.as_ref() else {
        return; // 材质尚未就绪（下一帧再跑）
    };

    // 阶段 1：增量生成。
    while cursor.gen_next < total && start.elapsed() < frame_budget {
        for _ in 0..8 {
            if cursor.gen_next >= total {
                break;
            }
            world.ensure_chunk(linear_to_chunk(cursor.gen_next, n));
            cursor.gen_next += 1;
        }
    }
    // 阶段 2：增量 Mesh（要求全部生成完毕，跨界查询才完备）。
    if cursor.gen_next >= total {
        while cursor.mesh_next < total && start.elapsed() < frame_budget {
            for _ in 0..8 {
                if cursor.mesh_next >= total {
                    break;
                }
                let cc = linear_to_chunk(cursor.mesh_next, n);
                let data = build_chunk_mesh(world, cc, tint.0);
                apply_chunk_mesh(
                    &mut commands,
                    &mut meshes,
                    material,
                    &mut registry,
                    &mut stats,
                    cc,
                    &data,
                );
                cursor.mesh_next += 1;
            }
        }
    }

    let pct = if total == 0 {
        100
    } else {
        (cursor.gen_next + cursor.mesh_next) * 100 / (total * 2)
    };
    if let Ok(mut text) = loading_text.single_mut() {
        **text = format!(
            "Craftsman Fortress · loading world\nGen {:>4}/{} · Mesh {:>4}/{} · {:>2}%",
            cursor.gen_next, total, cursor.mesh_next, total, pct
        );
    }

    if cursor.mesh_next >= total {
        info!(
            "世界加载完成：{} chunks，{} mesh 实体（可见未准备=0）",
            total,
            registry.meshed_count()
        );
        next_state.set(GameState::Ready);
    }
}

/// Ready 后的脏 Chunk 重建（单体素修改 / 调试切换触发）。
fn rebuild_dirty_system(
    mut commands: Commands,
    mut world_res: ResMut<WorldRes>,
    mut meshes: ResMut<Assets<Mesh>>,
    material: Res<StandardMaterialHandle>,
    mut registry: ResMut<ChunkMeshes>,
    mut stats: ResMut<MeshResourceStats>,
    tint: Res<DebugTintRes>,
) {
    let Some(world) = world_res.0.as_mut() else {
        return;
    };
    let Some(material) = material.0.as_ref() else {
        return;
    };
    let dirty = world.dirty_chunks();
    for cc in dirty.into_iter().take(64) {
        let data = build_chunk_mesh(world, cc, tint.0);
        apply_chunk_mesh(
            &mut commands,
            &mut meshes,
            material,
            &mut registry,
            &mut stats,
            cc,
            &data,
        );
        world.clear_dirty(cc);
    }
}

/// F3 切换 Chunk 边界调试着色：全体 Chunk 重网格。
fn toggle_tint_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut tint: ResMut<DebugTintRes>,
    mut world_res: ResMut<WorldRes>,
) {
    if !keys.just_pressed(KeyCode::F3) {
        return;
    }
    tint.0 = if tint.0 == DebugTint::Off {
        DebugTint::ChunkParity
    } else {
        DebugTint::Off
    };
    if let Some(world) = world_res.0.as_mut() {
        let n = world.size.chunks();
        for cy in 0..n.y as i32 {
            for cz in 0..n.z as i32 {
                for cx in 0..n.x as i32 {
                    world.mark_dirty(IVec3::new(cx, cy, cz));
                }
            }
        }
        info!("调试着色切换为 {:?}", tint.0);
    }
}

/// F12 调试截图：走与验收相同的 Screenshot 管线，保存到工作目录。
fn debug_screenshot_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut counter: Local<u32>,
) {
    if !keys.just_pressed(KeyCode::F12) {
        return;
    }
    *counter += 1;
    let path = std::path::PathBuf::from(format!("debug_shot_{}.png", *counter));
    commands
        .spawn(Screenshot::primary_window())
        .observe(move |trigger: On<ScreenshotCaptured>| {
            let img: bevy::image::Image = std::ops::Deref::deref(trigger.event()).clone();
            match crate::acceptance::save_screenshot(&img, &path, false) {
                Ok((w, h, bytes)) => {
                    info!("[调试] F12 截图已保存 {path:?}: {w}x{h}, {bytes}B");
                }
                Err(e) => info!("[调试] F12 截图保存失败 {path:?}: {e}"),
            }
        });
}
