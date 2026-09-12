//! Chunk Mesh 构建（面剔除）：纯数据函数，不依赖 Bevy 资源。
//!
//! 规则（任务书 4.4）：
//! - 完全被实体体素遮挡的面不提交；
//! - 相邻 Chunk 交界处由权威 `World::voxel` 查询邻居，不重复生成内部面；
//! - 空 Chunk 产生 0 面（调用方据此不生成 Mesh 实体）；
//! - 顶点/索引/法线/颜色数组自洽，可直接装配 Bevy `Mesh`。

use crate::coords::{chunk_origin, index_local};
use crate::voxel::{BlockId, FaceDir};
use crate::world::World;
use bevy::math::{IVec3, UVec3};

/// 一个 Chunk 的网格数据（单位：米，Chunk 局部坐标 + Chunk 原点平移由调用方处理）。
#[derive(Clone, Debug, Default)]
pub struct ChunkMeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

impl ChunkMeshData {
    pub fn face_count(&self) -> usize {
        self.indices.len() / 6
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
}

/// 调试图视图：Chunk 边界着色模式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugTint {
    Off,
    /// 按 (cx+cy+cz) 奇偶交替染色，凸显 Chunk 边界。
    ChunkParity,
}

/// 每个面的 4 个角点（相对体素局部坐标 [0,1]³，逆时针朝外绕序）。
fn face_corners(dir: FaceDir) -> [[f32; 3]; 4] {
    // 体素占据 [x, x+1]³。
    match dir {
        FaceDir::PosX => [[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [1.0, 1.0, 1.0], [1.0, 0.0, 1.0]],
        FaceDir::NegX => [[0.0, 0.0, 1.0], [0.0, 1.0, 1.0], [0.0, 1.0, 0.0], [0.0, 0.0, 0.0]],
        FaceDir::PosY => [[0.0, 1.0, 0.0], [0.0, 1.0, 1.0], [1.0, 1.0, 1.0], [1.0, 1.0, 0.0]],
        FaceDir::NegY => [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 0.0, 1.0]],
        FaceDir::PosZ => [[1.0, 0.0, 1.0], [1.0, 1.0, 1.0], [0.0, 1.0, 1.0], [0.0, 0.0, 1.0]],
        FaceDir::NegZ => [[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0], [1.0, 0.0, 0.0]],
    }
}

/// 构建 Chunk 网格。`world` 为权威数据源（跨界查询邻居）。
pub fn build_chunk_mesh(world: &World, cc: IVec3, tint: DebugTint) -> ChunkMeshData {
    let mut mesh = ChunkMeshData::default();
    let slot = match world.chunk(cc) {
        Some(s) if s.generated => s,
        _ => return mesh,
    };
    if slot.data.is_empty() {
        return mesh;
    }
    let origin = chunk_origin(cc);

    // 邻居 Chunk 数据预取（含自身），跨界查询零散时直接走 world 兜底。
    let mut neighbor_data: [Option<&crate::chunk::ChunkData>; 27] = [None; 27];
    for dy in -1..=1 {
        for dz in -1..=1 {
            for dx in -1..=1 {
                let ncc = cc + IVec3::new(dx, dy, dz);
                let idx = ((dy + 1) * 3 + (dz + 1)) * 3 + (dx + 1);
                neighbor_data[idx as usize] = world.chunk(ncc).filter(|s| s.generated).map(|s| &s.data);
            }
        }
    }
    let self_data = neighbor_data[13].unwrap();

    // 权威跨界查询：优先邻域缓存，缺失时回退 world（例如 y<0 底板语义）。
    let voxel_at = |wx: i32, wy: i32, wz: i32| -> BlockId {
        let ldx = wx - origin.x;
        let ldy = wy - origin.y;
        let ldz = wz - origin.z;
        let (cdx, cdy, cdz, rx, ry, rz);
        if ldx < 0 {
            cdx = -1;
            rx = ldx + 16;
        } else if ldx >= 16 {
            cdx = 1;
            rx = ldx - 16;
        } else {
            cdx = 0;
            rx = ldx;
        }
        if ldy < 0 {
            cdy = -1;
            ry = ldy + 16;
        } else if ldy >= 16 {
            cdy = 1;
            ry = ldy - 16;
        } else {
            cdy = 0;
            ry = ldy;
        }
        if ldz < 0 {
            cdz = -1;
            rz = ldz + 16;
        } else if ldz >= 16 {
            cdz = 1;
            rz = ldz - 16;
        } else {
            cdz = 0;
            rz = ldz;
        }
        if cdx == 0 && cdy == 0 && cdz == 0 {
            return self_data.get(UVec3::new(rx as u32, ry as u32, rz as u32));
        }
        let idx = ((cdy + 1) * 3 + (cdz + 1)) * 3 + (cdx + 1);
        match neighbor_data[idx as usize] {
            Some(data) => data.get(UVec3::new(rx as u32, ry as u32, rz as u32)),
            None => world.voxel(IVec3::new(wx, wy, wz)),
        }
    };

    let tint_factor = match tint {
        DebugTint::Off => 1.0,
        DebugTint::ChunkParity => {
            if (cc.x + cc.y + cc.z).rem_euclid(2) == 0 {
                1.0
            } else {
                0.72
            }
        }
    };

    for i in 0..crate::coords::CHUNK_VOLUME {
        let block = self_data.get_index(i);
        if !block.is_solid() {
            continue;
        }
        let local = index_local(i);
        let wx = origin.x + local.x as i32;
        let wy = origin.y + local.y as i32;
        let wz = origin.z + local.z as i32;
        let color = block.linear_color();
        for dir in FaceDir::ALL {
            let (nx, ny, nz) = dir.normal();
            let neighbor = voxel_at(wx + nx, wy + ny, wz + nz);
            if neighbor.is_solid() {
                continue; // 遮挡面剔除
            }
            let base = mesh.vertex_count() as u32;
            let corners = face_corners(dir);
            let shade = dir.shade() * tint_factor;
            let n = [nx as f32, ny as f32, nz as f32];
            for c in corners {
                mesh.positions.push([
                    wx as f32 + c[0],
                    wy as f32 + c[1],
                    wz as f32 + c[2],
                ]);
                mesh.normals.push(n);
                mesh.colors.push([color[0] * shade, color[1] * shade, color[2] * shade]);
            }
            // 两三角：base+0,+1,+2 与 base+0,+2,+3（corners 已按外向逆时针排列）。
            mesh.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coords::WorldSize;
    use crate::generation::TerrainParams;

    /// 手工构造世界：填充指定体素集合，其余空气。
    fn world_with(size: WorldSize, solid: &[IVec3]) -> World {
        let mut w = World::empty(size, TerrainParams::new(1));
        // 生成所有 chunk 为空气
        let n = size.chunks();
        for cy in 0..n.y as i32 {
            for cz in 0..n.z as i32 {
                for cx in 0..n.x as i32 {
                    w.ensure_chunk(IVec3::new(cx, cy, cz));
                }
            }
        }
        for v in solid {
            w.set_voxel(*v, BlockId::Stone).unwrap();
        }
        w.clear_all_dirty_for_test();
        w
    }

    #[test]
    fn single_voxel_six_faces() {
        let w = world_with(WorldSize::new(16, 16, 16), &[IVec3::new(8, 8, 8)]);
        let m = build_chunk_mesh(&w, IVec3::ZERO, DebugTint::Off);
        assert_eq!(m.face_count(), 6, "孤立单实体体素应有 6 面");
        assert_eq!(m.vertex_count(), 24);
        assert_eq!(m.indices.len(), 36);
    }

    #[test]
    fn adjacent_pair_hides_shared_faces() {
        let w = world_with(WorldSize::new(16, 16, 16), &[IVec3::new(8, 8, 8), IVec3::new(9, 8, 8)]);
        let m = build_chunk_mesh(&w, IVec3::ZERO, DebugTint::Off);
        assert_eq!(m.face_count(), 10, "两相邻体素应共 10 面（12 - 2 内部面）");
    }

    #[test]
    fn cross_chunk_pair_no_duplicate_interior_faces() {
        // 体素分属相邻 Chunk：x=15 在 chunk0，x=16 在 chunk1。
        let w = world_with(WorldSize::new(32, 16, 32), &[IVec3::new(15, 8, 8), IVec3::new(16, 8, 8)]);
        let m0 = build_chunk_mesh(&w, IVec3::new(0, 0, 0), DebugTint::Off);
        let m1 = build_chunk_mesh(&w, IVec3::new(1, 0, 0), DebugTint::Off);
        assert_eq!(m0.face_count(), 5, "chunk0 中该体素 +x 面被遮挡 => 5 面");
        assert_eq!(m1.face_count(), 5, "chunk1 中该体素 -x 面被遮挡 => 5 面");
        assert_eq!(m0.face_count() + m1.face_count(), 10, "跨界相邻不得产生内部面");
    }

    #[test]
    fn solid_cube_occludes_interior() {
        // 4x4x4 实心立方体：只有表面 6*16 面。
        let mut solid = Vec::new();
        for x in 4..8 {
            for y in 4..8 {
                for z in 4..8 {
                    solid.push(IVec3::new(x, y, z));
                }
            }
        }
        let w = world_with(WorldSize::new(16, 16, 16), &solid);
        let m = build_chunk_mesh(&w, IVec3::ZERO, DebugTint::Off);
        assert_eq!(m.face_count(), 96, "4³ 实心立方体仅 6×16=96 外表面");
    }

    #[test]
    fn empty_chunk_no_faces() {
        let w = world_with(WorldSize::new(16, 16, 16), &[]);
        let m = build_chunk_mesh(&w, IVec3::ZERO, DebugTint::Off);
        assert!(m.is_empty(), "空 Chunk 不生成面");
    }

    #[test]
    fn indices_normals_valid_and_winding_outward() {
        let mut solid = Vec::new();
        for x in 5..9 {
            for y in 5..9 {
                for z in 5..9 {
                    solid.push(IVec3::new(x, y, z));
                }
            }
        }
        let w = world_with(WorldSize::new(16, 16, 16), &solid);
        let m = build_chunk_mesh(&w, IVec3::ZERO, DebugTint::Off);
        // 索引有效
        for &ix in &m.indices {
            assert!((ix as usize) < m.vertex_count());
        }
        // 每个三角形面元法线（叉积）应与顶点法线一致（同向），验证绕序朝外。
        for tri in m.indices.chunks_exact(3) {
            let a = m.positions[tri[0] as usize];
            let b = m.positions[tri[1] as usize];
            let c = m.positions[tri[2] as usize];
            let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let cross = [
                ab[1] * ac[2] - ab[2] * ac[1],
                ab[2] * ac[0] - ab[0] * ac[2],
                ab[0] * ac[1] - ab[1] * ac[0],
            ];
            let n = m.normals[tri[0] as usize];
            let dot = cross[0] * n[0] + cross[1] * n[1] + cross[2] * n[2];
            assert!(dot > 0.0, "三角形绕序应与法线同向: dot={dot}");
        }
        // 法线均为单位轴向量
        for n in &m.normals {
            let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            assert!((l - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn boundary_edit_invalidates_and_rebuild_consistent() {
        // 跨界对：修改后两侧面数变化一致。
        let mut w = world_with(WorldSize::new(32, 16, 32), &[IVec3::new(15, 8, 8)]);
        let m0_before = build_chunk_mesh(&w, IVec3::new(0, 0, 0), DebugTint::Off).face_count();
        w.set_voxel(IVec3::new(16, 8, 8), BlockId::Stone).unwrap();
        let dirty = w.dirty_chunks();
        assert!(dirty.contains(&IVec3::new(0, 0, 0)), "-x 侧邻居应被标脏");
        assert!(dirty.contains(&IVec3::new(1, 0, 0)));
        let m0_after = build_chunk_mesh(&w, IVec3::new(0, 0, 0), DebugTint::Off).face_count();
        let m1_after = build_chunk_mesh(&w, IVec3::new(1, 0, 0), DebugTint::Off).face_count();
        assert_eq!(m0_before, 6);
        assert_eq!(m0_after, 5, "新邻居遮挡了 +x 面");
        assert_eq!(m1_after, 5);
    }

    #[test]
    fn full_world_generation_meshing_smoke() {
        // 小型完整世界：所有 chunk 可 mesh，无 panic，面数为正。
        let w = World::generate_all(WorldSize::new(64, 64, 64), TerrainParams::new(3));
        let mut total_faces = 0usize;
        for cy in 0..4 {
            for cz in 0..4 {
                for cx in 0..4 {
                    total_faces += build_chunk_mesh(&w, IVec3::new(cx, cy, cz), DebugTint::Off).face_count();
                }
            }
        }
        assert!(total_faces > 1000, "总面数应显著为正: {total_faces}");
    }
}
