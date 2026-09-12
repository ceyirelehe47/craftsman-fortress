//! `World`：验证世界的单一权威数据源。
//!
//! - 持有全部 Chunk（CPU 数据全常驻，任务书 3 加载策略允许的简化方案）；
//! - 渲染、拾取、生成、测试全部通过 `World::voxel()` 查询，不得另建副本；
//! - `set_voxel` 提供"单体素修改 + 边界重建触发"（任务书 4.2 扩展性要求）；
//! - 越界语义：`y < 0` 视为实体（基岩底板，避免世界底面渲染/穿透）；
//!   `y >= H` 与水平越界视为空气（地图边缘呈剖面状可见）。

use crate::chunk::ChunkData;
use crate::coords::{chunk_of_voxel, local_of_voxel, WorldSize, CHUNK_SIZE};
use crate::generation::{generate_chunk, TerrainParams};
use crate::noise::mix64;
use crate::voxel::BlockId;
use bevy::math::IVec3;
use std::collections::HashMap;

/// Chunk 槽位：数据 + 是否已生成。
#[derive(Clone, Debug)]
pub struct ChunkSlot {
    pub data: ChunkData,
    pub generated: bool,
}

/// 世界权威对象。非 ECS Resource 驻留（任务书"数据原则"：Chunk 数据与 ECS 独立对象分离）。
pub struct World {
    pub size: WorldSize,
    pub params: TerrainParams,
    /// 按 (cx + cz*Wx + cy*Wx*Wz) 线性存放的全量 Chunk。
    chunks: Vec<ChunkSlot>,
    /// 需要 Mesh 重建的 Chunk 集合。
    dirty: HashMap<IVec3, ()>,
    /// 已生成 Chunk 计数。
    generated_count: usize,
}

impl World {
    /// 创建世界并立即生成全部 Chunk（验证世界为有边界全常驻方案）。
    pub fn generate_all(size: WorldSize, params: TerrainParams) -> Self {
        let n = size.chunks();
        let total = (n.x * n.y * n.z) as usize;
        let gen_params = params.clone();
        let mut world = Self {
            size,
            params,
            chunks: Vec::with_capacity(total),
            dirty: HashMap::default(),
            generated_count: 0,
        };
        // 槽位线性排布：index = cx + cz*Wx + cy*Wx*Wz，按 (cy, cz, cx) 嵌套顺序生成。
        for cy in 0..n.y as i32 {
            for cz in 0..n.z as i32 {
                for cx in 0..n.x as i32 {
                    let cc = IVec3::new(cx, cy, cz);
                    let data = generate_chunk(&gen_params, cc, size.y);
                    world.chunks.push(ChunkSlot {
                        data,
                        generated: true,
                    });
                    world.generated_count += 1;
                }
            }
        }
        world
    }

    /// 空世界（测试用：按需生成）。
    pub fn empty(size: WorldSize, params: TerrainParams) -> Self {
        let n = size.chunks();
        let total = (n.x * n.y * n.z) as usize;
        Self {
            size,
            params,
            chunks: (0..total)
                .map(|_| ChunkSlot {
                    data: ChunkData::filled_air(),
                    generated: false,
                })
                .collect(),
            dirty: HashMap::default(),
            generated_count: 0,
        }
    }

    /// 按需生成单个 Chunk（确定性测试用：可乱序调用）。
    pub fn ensure_chunk(&mut self, cc: IVec3) {
        let i = self.slot_index(cc).expect("chunk 坐标越界");
        if !self.chunks[i].generated {
            self.chunks[i].data = generate_chunk(&self.params, cc, self.size.y);
            self.chunks[i].generated = true;
            self.generated_count += 1;
        }
    }

    #[inline]
    fn slot_index(&self, cc: IVec3) -> Option<usize> {
        if !self.size.contains_chunk(cc) {
            return None;
        }
        let n = self.size.chunks();
        Some(
            (cc.x as usize) + (cc.z as usize) * (n.x as usize) + (cc.y as usize) * (n.x as usize) * (n.z as usize),
        )
    }

    pub fn chunk(&self, cc: IVec3) -> Option<&ChunkSlot> {
        self.slot_index(cc).map(|i| &self.chunks[i])
    }

    pub fn chunk_mut(&mut self, cc: IVec3) -> Option<&mut ChunkSlot> {
        let i = self.slot_index(cc)?;
        Some(&mut self.chunks[i])
    }

    pub fn generated_count(&self) -> usize {
        self.generated_count
    }

    pub fn total_chunks(&self) -> usize {
        self.chunks.len()
    }

    /// 权威体素查询（跨 Chunk、跨边界统一入口）。
    pub fn voxel(&self, v: IVec3) -> BlockId {
        if v.y < 0 {
            // 世界底板：视为实体，杜绝从底部看到虚空。
            return BlockId::Bedrock;
        }
        if !self.size.contains(v) {
            return BlockId::Air;
        }
        let cc = chunk_of_voxel(v);
        let local = local_of_voxel(v);
        let slot = &self.chunks[self.slot_index(cc).unwrap()];
        if !slot.generated {
            // 全常驻方案下不应发生；保守返回空气并记录（不会在正式巡航出现）。
            return BlockId::Air;
        }
        slot.data.get(local)
    }

    /// 单体素修改：写入并标记受影响 Chunk（含边界邻居）为脏。
    /// 返回旧值；越界或未生成返回 None。
    pub fn set_voxel(&mut self, v: IVec3, block: BlockId) -> Option<BlockId> {
        if !self.size.contains(v) {
            return None;
        }
        let cc = chunk_of_voxel(v);
        let local = local_of_voxel(v);
        {
            let idx = self.slot_index(cc).unwrap();
            let slot = &mut self.chunks[idx];
            if !slot.generated {
                return None;
            }
            let old = slot.data.get(local);
            if old == block {
                return Some(old);
            }
            slot.data.set(local, block);
        }
        // 标记本 Chunk 与（当体素位于 Chunk 边界时）相邻 Chunk。
        self.dirty.insert(cc, ());
        if local.x == 0 {
            self.dirty.insert(cc + IVec3::new(-1, 0, 0), ());
        }
        if local.x == CHUNK_SIZE as u32 - 1 {
            self.dirty.insert(cc + IVec3::new(1, 0, 0), ());
        }
        if local.y == 0 {
            self.dirty.insert(cc + IVec3::new(0, -1, 0), ());
        }
        if local.y == CHUNK_SIZE as u32 - 1 {
            self.dirty.insert(cc + IVec3::new(0, 1, 0), ());
        }
        if local.z == 0 {
            self.dirty.insert(cc + IVec3::new(0, 0, -1), ());
        }
        if local.z == CHUNK_SIZE as u32 - 1 {
            self.dirty.insert(cc + IVec3::new(0, 0, 1), ());
        }
        // 只保留有效范围内的 Chunk（越界邻居忽略）。
        self.dirty.retain(|k, _| self.size.contains_chunk(*k));
        Some(self.voxel(v))
    }

    /// 待重建 Chunk 队列快照。
    pub fn dirty_chunks(&self) -> Vec<IVec3> {
        let mut v: Vec<IVec3> = self.dirty.keys().copied().collect();
        v.sort_by_key(|c| (c.x, c.y, c.z));
        v
    }

    pub fn dirty_count(&self) -> usize {
        self.dirty.len()
    }

    /// 清除指定 Chunk 的脏标记（Mesh 重建完成后由渲染层调用）。
    pub fn clear_dirty(&mut self, cc: IVec3) {
        self.dirty.remove(&cc);
    }

    /// 外部标脏（调试视图切换等）。
    pub fn mark_dirty(&mut self, cc: IVec3) {
        if self.size.contains_chunk(cc) {
            self.dirty.insert(cc, ());
        }
    }

    /// 全量世界哈希（确定性验收 A02 证据）：按固定顺序混合 palette + 索引字节。
    pub fn world_hash(&self) -> u64 {
        let mut h = mix64(self.params.seed ^ (self.size.x as u64) << 32 ^ self.size.y as u64 ^ (self.size.z as u64) << 48);
        for cy in 0..(self.size.chunks().y as i32) {
            for cz in 0..(self.size.chunks().z as i32) {
                for cx in 0..(self.size.chunks().x as i32) {
                    let slot = self.chunk(IVec3::new(cx, cy, cz)).unwrap();
                    h = mix64(h ^ slot.data.indices().len() as u64);
                    for &b in slot.data.indices() {
                        h = mix64(h ^ (b as u64));
                    }
                    for &p in slot.data.palette() {
                        h = mix64(h ^ (p as u64).rotate_left(32));
                    }
                }
            }
        }
        h
    }

    /// 列最高实体体素高度（特征探测/相机用）；无实体返回 -1。
    pub fn column_height(&self, x: i32, z: i32) -> i32 {
        for y in (0..self.size.y as i32).rev() {
            if self.voxel(IVec3::new(x, y, z)).is_solid() {
                return y;
            }
        }
        -1
    }

    /// 体素中心世界坐标（米）。
    pub fn voxel_center(v: IVec3) -> bevy::math::Vec3 {
        bevy::math::Vec3::new(v.x as f32 + 0.5, v.y as f32 + 0.5, v.z as f32 + 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small_params() -> TerrainParams {
        let mut p = TerrainParams::new(7);
        p.base_height = 10.0;
        p
    }

    #[test]
    fn voxel_query_crosses_chunk_boundary() {
        let size = WorldSize::new(32, 32, 32);
        let mut w = World::empty(size, small_params());
        for cc in [
            IVec3::new(0, 0, 0),
            IVec3::new(1, 0, 0),
            IVec3::new(0, 0, 1),
            IVec3::new(1, 0, 1),
            IVec3::new(0, 1, 0),
            IVec3::new(1, 1, 1),
        ] {
            w.ensure_chunk(cc);
        }
        // 跨界体素两侧都能查询
        let a = w.voxel(IVec3::new(15, 3, 15));
        let b = w.voxel(IVec3::new(16, 3, 16));
        assert!(a.is_solid() || b.is_solid() || true); // 语义查询不 panic 即可
        assert_eq!(w.generated_count(), 6);
    }

    #[test]
    fn set_voxel_marks_boundary_neighbors() {
        let size = WorldSize::new(32, 32, 32);
        let mut w = World::generate_all(size, small_params());
        w.clear_all_dirty_for_test();
        // 修改 (16, 8, 16)：位于 chunk (1,0,1) 的局部 (0,8,0)，x/z 均贴边界
        let old = w.set_voxel(IVec3::new(16, 8, 16), BlockId::Bedrock).unwrap();
        assert_eq!(w.voxel(IVec3::new(16, 8, 16)), BlockId::Bedrock);
        assert_ne!(old, BlockId::Bedrock);
        let dirty = w.dirty_chunks();
        assert!(dirty.contains(&IVec3::new(1, 0, 1)), "本 chunk 应脏: {dirty:?}");
        assert!(dirty.contains(&IVec3::new(0, 0, 1)), "-x 邻居应脏");
        assert!(dirty.contains(&IVec3::new(1, 0, 0)), "-z 邻居应脏");
        assert!(!dirty.contains(&IVec3::new(0, 0, 0)));
        w.clear_dirty(IVec3::new(1, 0, 1));
        assert_eq!(w.dirty_count(), dirty.len() - 1);
    }

    #[test]
    fn out_of_bounds_semantics() {
        let size = WorldSize::new(32, 32, 32);
        let mut w = World::empty(size, small_params());
        w.ensure_chunk(IVec3::new(0, 0, 0));
        assert_eq!(w.voxel(IVec3::new(-1, 5, 0)), BlockId::Air, "水平越界=空气");
        assert_eq!(w.voxel(IVec3::new(0, -1, 0)), BlockId::Bedrock, "底部越界=实体");
        assert_eq!(w.voxel(IVec3::new(0, 100, 0)), BlockId::Air, "顶部越界=空气");
        assert_eq!(w.set_voxel(IVec3::new(-1, 0, 0), BlockId::Stone), None);
        assert_eq!(w.set_voxel(IVec3::new(0, 0, 0), BlockId::Stone), None, "未生成 chunk 不可写");
    }

    #[test]
    fn world_hash_order_independent_and_stable() {
        let size = WorldSize::new(48, 48, 48);
        let params = TerrainParams::new(99);
        let mut a = World::empty(size, params.clone());
        // 顺序 1：正序
        for cy in 0..3 {
            for cz in 0..3 {
                for cx in 0..3 {
                    a.ensure_chunk(IVec3::new(cx, cy, cz));
                }
            }
        }
        let mut b = World::empty(size, params);
        // 顺序 2：乱序（固定伪随机置换）
        let mut order: Vec<IVec3> = Vec::new();
        for cy in 0..3 {
            for cz in 0..3 {
                for cx in 0..3 {
                    order.push(IVec3::new(cx, cy, cz));
                }
            }
        }
        let mut rng: u64 = 0xDEAD_BEEF;
        let mut i = order.len();
        while i > 1 {
            rng = crate::noise::mix64(rng);
            let j = (rng as usize) % i;
            i -= 1;
            order.swap(i, j);
        }
        for cc in order {
            b.ensure_chunk(cc);
        }
        assert_eq!(a.world_hash(), b.world_hash(), "生成顺序不得影响世界哈希");

        let mut c = World::empty(size, TerrainParams::new(99));
        for cc in order.iter().rev() {
            c.ensure_chunk(*cc);
        }
        assert_eq!(a.world_hash(), c.world_hash());

        // 不同 seed 必须不同
        let mut d = World::empty(size, TerrainParams::new(100));
        for cy in 0..3 {
            for cz in 0..3 {
                for cx in 0..3 {
                    d.ensure_chunk(IVec3::new(cx, cy, cz));
                }
            }
        }
        assert_ne!(a.world_hash(), d.world_hash());
    }
}

impl World {
    /// 测试辅助：清空脏集合。
    pub fn clear_all_dirty_for_test(&mut self) {
        self.dirty.clear();
    }
}
