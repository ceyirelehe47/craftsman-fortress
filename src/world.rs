//! `World`：验证世界的单一权威数据源。
//!
//! - 持有全部 Chunk（CPU 数据全常驻的加载策略简化方案）；
//! - 渲染、拾取、生成、测试全部通过 `World::voxel()` 查询，不得另建副本；
//! - `set_voxel` 提供"单体素修改 + 边界重建触发"；
//! - 越界语义：`y < 0` 视为实体（基岩底板，避免世界底面渲染/穿透）；
//!   `y >= H` 与水平越界视为空气（地图边缘呈剖面状可见）。

use crate::chunk::ChunkData;
use crate::coords::{chunk_of_voxel, local_of_voxel, WorldSize, CHUNK_SIZE, CHUNK_VOLUME};
use crate::generation::{generate_chunk, generated_block_at, TerrainParams};
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditReject {
    OutOfBounds(IVec3),
    ChunkNotLoaded(IVec3),
    ProtectedBedrock(IVec3),
    ReservedTopLayer(IVec3),
    BedrockPlacement(IVec3),
}

impl std::fmt::Display for EditReject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditReject::OutOfBounds(v) => write!(f, "out of bounds: {v}"),
            EditReject::ChunkNotLoaded(v) => write!(f, "chunk not loaded: {v}"),
            EditReject::ProtectedBedrock(v) => write!(f, "protected bedrock: {v}"),
            EditReject::ReservedTopLayer(v) => write!(f, "top safety layer must stay air: {v}"),
            EditReject::BedrockPlacement(v) => write!(f, "Bedrock cannot be placed: {v}"),
        }
    }
}

impl std::error::Error for EditReject {}

/// 世界权威对象。非 ECS Resource 驻留（数据原则：Chunk 数据与 ECS 独立对象分离）。
pub struct World {
    pub size: WorldSize,
    pub params: TerrainParams,
    /// 按 (cx + cz*Wx + cy*Wx*Wz) 线性存放的全量 Chunk。
    chunks: Vec<ChunkSlot>,
    /// 需要 Mesh 重建的 Chunk 集合。
    dirty: HashMap<IVec3, ()>,
    /// 已生成 Chunk 计数。
    generated_count: usize,
    /// 相对确定性 Seed 世界的最小修改覆盖层。
    modifications: HashMap<IVec3, BlockId>,
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
            modifications: HashMap::default(),
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
            modifications: HashMap::default(),
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
            (cc.x as usize)
                + (cc.z as usize) * (n.x as usize)
                + (cc.y as usize) * (n.x as usize) * (n.z as usize),
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

    /// 单体素修改：写入、维护修改覆盖层，并标记受影响 Chunk（含边界邻居）为脏。
    /// 返回旧值；越界或未生成返回 None。
    pub fn set_voxel(&mut self, v: IVec3, block: BlockId) -> Option<BlockId> {
        self.write_voxel(v, block, true, true)
    }

    fn write_voxel(
        &mut self,
        v: IVec3,
        block: BlockId,
        mark_dirty: bool,
        track_modification: bool,
    ) -> Option<BlockId> {
        if !self.size.contains(v) {
            return None;
        }
        let cc = chunk_of_voxel(v);
        let local = local_of_voxel(v);
        let old = {
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
            old
        };

        if track_modification {
            let generated = generated_block_at(&self.params, v, self.size.y);
            if block == generated {
                self.modifications.remove(&v);
            } else {
                self.modifications.insert(v, block);
            }
        }
        if mark_dirty {
            self.mark_voxel_dirty(cc, local);
        }
        Some(old)
    }

    fn mark_voxel_dirty(&mut self, cc: IVec3, local: bevy::math::UVec3) {
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
        self.dirty.retain(|k, _| self.size.contains_chunk(*k));
    }

    /// 玩家可见的编辑规则：基岩不可修改、不可放置 Bedrock、H-1 保持为空气安全层。
    pub fn try_user_edit(&mut self, v: IVec3, block: BlockId) -> Result<bool, EditReject> {
        if !self.size.contains(v) {
            return Err(EditReject::OutOfBounds(v));
        }
        let old = self.voxel(v);
        if old == block {
            return Ok(false);
        }
        if old == BlockId::Bedrock || v.y <= self.params.bedrock_layers {
            return Err(EditReject::ProtectedBedrock(v));
        }
        if block == BlockId::Bedrock {
            return Err(EditReject::BedrockPlacement(v));
        }
        if v.y as u32 == self.size.y - 1 && block.is_solid() {
            return Err(EditReject::ReservedTopLayer(v));
        }
        self.set_voxel(v, block)
            .map(|previous| previous != block)
            .ok_or(EditReject::ChunkNotLoaded(v))
    }

    pub fn modification_count(&self) -> usize {
        self.modifications.len()
    }

    pub fn modifications_sorted(&self) -> Vec<(IVec3, BlockId)> {
        let mut edits: Vec<_> = self.modifications.iter().map(|(v, b)| (*v, *b)).collect();
        edits.sort_by_key(|(v, _)| (v.x, v.y, v.z));
        edits
    }

    /// 已生成世界的语义哈希：按固定 Chunk/体素顺序混合解码后的 BlockId，
    /// 与 palette 插入顺序无关，适合作为存档前后状态一致性的证据。
    pub fn semantic_hash(&self) -> u64 {
        let mut h = mix64(
            self.params.seed
                ^ (self.size.x as u64) << 32
                ^ self.size.y as u64
                ^ (self.size.z as u64) << 48
                ^ 0x5345_4D41_4E54_4943,
        );
        for cy in 0..self.size.chunks().y as i32 {
            for cz in 0..self.size.chunks().z as i32 {
                for cx in 0..self.size.chunks().x as i32 {
                    let slot = self.chunk(IVec3::new(cx, cy, cz)).unwrap();
                    for index in 0..CHUNK_VOLUME {
                        h = mix64(h ^ slot.data.get_index(index) as u64);
                    }
                }
            }
        }
        h
    }

    /// 不依赖当前修改覆盖层，重新按纯函数生成基础 Chunk 并计算语义哈希。
    pub fn generated_semantic_hash(&self) -> u64 {
        let mut h = mix64(
            self.params.seed
                ^ (self.size.x as u64) << 32
                ^ self.size.y as u64
                ^ (self.size.z as u64) << 48
                ^ 0x5345_4D41_4E54_4943,
        );
        for cy in 0..self.size.chunks().y as i32 {
            for cz in 0..self.size.chunks().z as i32 {
                for cx in 0..self.size.chunks().x as i32 {
                    let data = generate_chunk(&self.params, IVec3::new(cx, cy, cz), self.size.y);
                    for index in 0..CHUNK_VOLUME {
                        h = mix64(h ^ data.get_index(index) as u64);
                    }
                }
            }
        }
        h
    }

    /// 在已完整生成的世界上安装存档覆盖层；加载阶段不制造脏 Mesh 队列。
    pub fn apply_persisted_edits(&mut self, edits: &[(IVec3, BlockId)]) -> Result<(), String> {
        self.modifications.clear();
        for &(v, block) in edits {
            self.write_voxel(v, block, false, true)
                .ok_or_else(|| format!("cannot apply persisted edit at {v}"))?;
        }
        self.dirty.clear();
        Ok(())
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
        let mut h = mix64(
            self.params.seed
                ^ (self.size.x as u64) << 32
                ^ self.size.y as u64
                ^ (self.size.z as u64) << 48,
        );
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

    /// 测试辅助：把全部 chunk 置为已生成的空气（构造几何断言所需的受控空世界；
    /// `ensure_chunk` 生成的是真实地形，不适合精确面数/拾取断言）。
    pub fn fill_air_all_for_test(&mut self) {
        for slot in &mut self.chunks {
            slot.data = ChunkData::filled_air();
            slot.generated = true;
        }
        self.generated_count = self.chunks.len();
        self.dirty.clear();
        self.modifications.clear();
    }

    pub fn clear_all_dirty(&mut self) {
        self.dirty.clear();
    }

    /// 测试辅助：清空脏集合。
    pub fn clear_all_dirty_for_test(&mut self) {
        self.clear_all_dirty();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::UVec3;

    fn small_params() -> TerrainParams {
        let mut p = TerrainParams::new(7);
        p.base_height = 10.0;
        p
    }

    #[test]
    fn voxel_query_crosses_chunk_boundary() {
        let size = WorldSize::new(32, 32, 32);
        let mut w = World::empty(size, small_params());
        let touched = [
            IVec3::new(0, 0, 0),
            IVec3::new(1, 0, 0),
            IVec3::new(0, 0, 1),
            IVec3::new(1, 0, 1),
            IVec3::new(0, 1, 0),
            IVec3::new(1, 1, 1),
        ];
        for cc in touched {
            w.ensure_chunk(cc);
        }
        // 跨 Chunk 语义断言：World::voxel 的跨界查询必须与
        // generate_chunk 纯函数的直接输出逐体素一致（含洞穴等任何
        // 特殊地形——生成函数即权威）。这验证查询被正确路由到正确的
        // chunk 并返回纯函数结果，而不只是"不 panic"。
        for cc in touched {
            let direct = generate_chunk(&w.params, cc, size.y);
            let origin = IVec3::new(cc.x * 16, cc.y * 16, cc.z * 16);
            // 抽样 + 全部边界行：局部 (0|15, *, 0|15) 与若干内部点。
            for local in [
                UVec3::new(0, 3, 0),
                UVec3::new(15, 3, 15),
                UVec3::new(15, 3, 0),
                UVec3::new(0, 3, 15),
                UVec3::new(8, 5, 8),
                UVec3::new(15, 15, 15),
                UVec3::new(0, 0, 0),
            ] {
                let v = IVec3::new(
                    origin.x + local.x as i32,
                    origin.y + local.y as i32,
                    origin.z + local.z as i32,
                );
                assert_eq!(
                    w.voxel(v),
                    direct.get(local),
                    "跨界查询 {v:?}（chunk {cc} 局部 {local}）与纯函数生成不一致"
                );
            }
        }
        assert_eq!(w.generated_count(), 6);
    }

    #[test]
    fn set_voxel_marks_boundary_neighbors() {
        let size = WorldSize::new(32, 32, 32);
        let mut w = World::generate_all(size, small_params());
        w.clear_all_dirty_for_test();
        // 修改 (16, 8, 16)：位于 chunk (1,0,1) 的局部 (0,8,0)，x/z 均贴边界
        let old = w
            .set_voxel(IVec3::new(16, 8, 16), BlockId::Bedrock)
            .unwrap();
        assert_eq!(w.voxel(IVec3::new(16, 8, 16)), BlockId::Bedrock);
        assert_ne!(old, BlockId::Bedrock);
        let dirty = w.dirty_chunks();
        assert!(
            dirty.contains(&IVec3::new(1, 0, 1)),
            "本 chunk 应脏: {dirty:?}"
        );
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
        assert_eq!(
            w.voxel(IVec3::new(0, -1, 0)),
            BlockId::Bedrock,
            "底部越界=实体"
        );
        assert_eq!(
            w.voxel(IVec3::new(0, 100, 0)),
            BlockId::Air,
            "顶部越界=空气"
        );
        assert_eq!(w.set_voxel(IVec3::new(-1, 0, 0), BlockId::Stone), None);
        assert_eq!(
            w.set_voxel(IVec3::new(0, 0, 16), BlockId::Stone),
            None,
            "未生成 chunk 不可写"
        );
    }

    #[test]
    fn edit_overlay_normalizes_back_to_generated_value() {
        let size = WorldSize::new(32, 32, 32);
        let mut world = World::generate_all(size, TerrainParams::new(17));
        let voxel = IVec3::new(8, 12, 8);
        let generated = generated_block_at(&world.params, voxel, world.size.y);
        let changed = if generated.is_solid() {
            BlockId::Air
        } else {
            BlockId::Stone
        };
        world.set_voxel(voxel, changed).unwrap();
        assert_eq!(world.modification_count(), 1);
        world.set_voxel(voxel, generated).unwrap();
        assert_eq!(world.modification_count(), 0);
    }

    #[test]
    fn user_edit_rules_protect_bedrock_and_top_air() {
        let size = WorldSize::new(32, 32, 32);
        let mut world = World::generate_all(size, TerrainParams::new(17));
        assert!(world
            .try_user_edit(IVec3::new(4, 0, 4), BlockId::Air)
            .is_err());
        assert!(world
            .try_user_edit(IVec3::new(4, 31, 4), BlockId::Stone)
            .is_err());
    }

    #[test]
    fn semantic_hash_ignores_palette_edit_order() {
        let size = WorldSize::new(32, 32, 32);
        let params = TerrainParams::new(17);
        let mut a = World::generate_all(size, params.clone());
        let mut b = World::generate_all(size, params);
        let edits = [
            (IVec3::new(8, 12, 8), BlockId::Air),
            (IVec3::new(9, 12, 8), BlockId::Stone),
        ];
        for &(v, block) in &edits {
            a.set_voxel(v, block).unwrap();
        }
        for &(v, block) in edits.iter().rev() {
            b.set_voxel(v, block).unwrap();
        }
        assert_eq!(a.semantic_hash(), b.semantic_hash());
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
        for cc in &order {
            b.ensure_chunk(*cc);
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
