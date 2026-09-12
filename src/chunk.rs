//! Chunk 数据：palette + 紧凑索引（任务书"存储方向"约束）。
//!
//! 每个 Chunk 保存一个小 palette（去重的方块类型表）与逐体素的 1 字节索引数组；
//! 单一权威来源由 `World` 持有，渲染 / 拾取 / 生成均通过 `World` 查询，
//! 不得各自维护副本。

use crate::coords::{local_index, CHUNK_VOLUME};
use crate::voxel::BlockId;
use bevy::math::UVec3;

/// Chunk 的体素存储。palette[0] 恒为初始填充块。
#[derive(Clone, Debug)]
pub struct ChunkData {
    palette: Vec<BlockId>,
    /// 逐体素 palette 索引（紧凑表示：1 字节/体素）。
    indices: Vec<u8>,
}

impl ChunkData {
    /// 以统一填充块创建。
    pub fn filled(block: BlockId) -> Self {
        Self {
            palette: vec![block],
            indices: vec![0u8; CHUNK_VOLUME],
        }
    }

    pub fn filled_air() -> Self {
        Self::filled(BlockId::Air)
    }

    #[inline]
    pub fn get(&self, local: UVec3) -> BlockId {
        self.palette[self.indices[local_index(local)] as usize]
    }

    #[inline]
    pub fn get_index(&self, index: usize) -> BlockId {
        self.palette[self.indices[index] as usize]
    }

    #[inline]
    pub fn set(&mut self, local: UVec3, block: BlockId) {
        let idx = match self.palette.iter().position(|&b| b == block) {
            Some(i) => i,
            None => {
                debug_assert!(self.palette.len() < 256, "palette 溢出：方块类型过多");
                self.palette.push(block);
                self.palette.len() - 1
            }
        };
        self.indices[local_index(local)] = idx as u8;
    }

    #[inline]
    pub fn set_index(&mut self, index: usize, block: BlockId) {
        self.set(crate::coords::index_local(index), block);
    }

    /// palette 快照（用于哈希与测试）。
    pub fn palette(&self) -> &[BlockId] {
        &self.palette
    }

    /// 索引数组（用于哈希与测试）。
    pub fn indices(&self) -> &[u8] {
        &self.indices
    }

    /// 是否全为空气（无需生成 Mesh）。
    pub fn is_empty(&self) -> bool {
        // palette 只有空气一种时必然全空；否则扫描。
        if self.palette.len() == 1 {
            return self.palette[0] == BlockId::Air;
        }
        self.indices.iter().any(|&i| self.palette[i as usize] != BlockId::Air)
            .not()
    }
}

/// `is_empty` 的语义反转小助手（避免 `!x.iter().any(...)` 的可读性问题）。
trait Not {
    fn not(self) -> bool;
}
impl Not for bool {
    fn not(self) -> bool {
        !self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::UVec3;

    #[test]
    fn set_get_roundtrip() {
        let mut c = ChunkData::filled_air();
        c.set(UVec3::new(0, 0, 0), BlockId::Stone);
        c.set(UVec3::new(15, 15, 15), BlockId::Grass);
        c.set(UVec3::new(3, 7, 9), BlockId::Bedrock);
        assert_eq!(c.get(UVec3::new(0, 0, 0)), BlockId::Stone);
        assert_eq!(c.get(UVec3::new(15, 15, 15)), BlockId::Grass);
        assert_eq!(c.get(UVec3::new(3, 7, 9)), BlockId::Bedrock);
        assert_eq!(c.get(UVec3::new(1, 0, 0)), BlockId::Air);
    }

    #[test]
    fn palette_dedup() {
        let mut c = ChunkData::filled_air();
        for i in 0..64 {
            c.set_index(i, BlockId::Stone);
        }
        for i in 0..64 {
            c.set_index(i, BlockId::Dirt);
        }
        // 空气 + Stone + Dirt = 3 个（Stone 已被复用后移除？不：palette 只增不减）
        assert_eq!(c.palette().len(), 3);
        assert_eq!(c.get_index(0), BlockId::Dirt);
        assert_eq!(c.get_index(64), BlockId::Air);
    }

    #[test]
    fn empty_detection() {
        assert!(ChunkData::filled_air().is_empty());
        let mut c = ChunkData::filled_air();
        c.set(UVec3::ONE, BlockId::Stone);
        assert!(!c.is_empty());
        // 即便 palette 混入后全部清回空气，只要存在非空气 palette 且体素全为空气即空
        let mut d = ChunkData::filled_air();
        d.set(UVec3::ONE, BlockId::Stone);
        d.set(UVec3::ONE, BlockId::Air);
        assert!(d.is_empty());
    }
}
