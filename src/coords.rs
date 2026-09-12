//! 体素 / Chunk / 世界坐标转换。
//!
//! 约定：
//! - 体素坐标用有符号 `IVec3`（支持负数，数学上完整定义）；
//! - 验证世界的数据范围为 `[0, size)`（`size` 为体素数），越界访问由 `World` 权威处理；
//! - Chunk 为 16×16×16，Chunk 坐标 = 体素坐标算术右移 4 位（负数向负无穷取整，正确）。

use bevy::math::{IVec3, UVec3};

/// Chunk 边长（体素数）。集中配置点：如需调整，应同步审查 meshing 与索引排布。
pub const CHUNK_SIZE: usize = 16;
/// `CHUNK_SIZE` 的位偏移（仅支持 2 的幂尺寸）。
pub const CHUNK_SHIFT: u32 = 4;
/// 每 Chunk 体素数。
pub const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;

/// 体素坐标 -> Chunk 坐标（负数向负无穷取整）。
#[inline]
pub fn chunk_of_voxel(v: IVec3) -> IVec3 {
    v >> CHUNK_SHIFT as i32
}

/// 体素坐标 -> Chunk 内局部坐标（各分量落在 `[0, 16)`）。
#[inline]
pub fn local_of_voxel(v: IVec3) -> UVec3 {
    v.map(|c| c.rem_euclid(CHUNK_SIZE as i32)).as_uvec3()
}

/// Chunk 坐标 + 局部坐标 -> 体素坐标。
#[inline]
pub fn voxel_of_local(cc: IVec3, local: UVec3) -> IVec3 {
    cc * CHUNK_SIZE as i32 + local.as_ivec3()
}

/// Chunk 原点（最小角）的世界体素坐标。
#[inline]
pub fn chunk_origin(cc: IVec3) -> IVec3 {
    cc * CHUNK_SIZE as i32
}

/// Chunk 内局部坐标 -> 线性索引。
/// 排布：x 最快，其次 z，最后 y（同一竖列内存相邻，利于按列填充与扫描）。
#[inline]
pub fn local_index(local: UVec3) -> usize {
    (local.y as usize * CHUNK_SIZE + local.z as usize) * CHUNK_SIZE + local.x as usize
}

/// 线性索引 -> Chunk 内局部坐标。
#[inline]
pub fn index_local(index: usize) -> UVec3 {
    debug_assert!(index < CHUNK_VOLUME);
    let x = (index % CHUNK_SIZE) as u32;
    let z = ((index / CHUNK_SIZE) % CHUNK_SIZE) as u32;
    let y = (index / (CHUNK_SIZE * CHUNK_SIZE)) as u32;
    UVec3::new(x, y, z)
}

/// 世界体素尺寸（各轴体素数）。尺寸必须是 CHUNK_SIZE 的整数倍。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldSize {
    pub x: u32,
    pub y: u32,
    pub z: u32,
}

impl WorldSize {
    pub const fn new(x: u32, y: u32, z: u32) -> Self {
        Self { x, y, z }
    }

    /// 各轴 Chunk 数量。
    pub const fn chunks(&self) -> UVec3 {
        UVec3::new(
            self.x / CHUNK_SIZE as u32,
            self.y / CHUNK_SIZE as u32,
            self.z / CHUNK_SIZE as u32,
        )
    }

    /// 体素坐标是否在验证世界范围内。
    pub fn contains(&self, v: IVec3) -> bool {
        v.x >= 0
            && v.y >= 0
            && v.z >= 0
            && (v.x as u32) < self.x
            && (v.y as u32) < self.y
            && (v.z as u32) < self.z
    }

    /// Chunk 坐标是否在验证世界范围内。
    pub fn contains_chunk(&self, cc: IVec3) -> bool {
        let n = self.chunks();
        cc.x >= 0
            && cc.y >= 0
            && cc.z >= 0
            && (cc.x as u32) < n.x
            && (cc.y as u32) < n.y
            && (cc.z as u32) < n.z
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_conversion_nonnegative() {
        let v = IVec3::new(0, 0, 0);
        assert_eq!(chunk_of_voxel(v), IVec3::ZERO);
        assert_eq!(local_of_voxel(v), UVec3::ZERO);

        let v = IVec3::new(15, 15, 15);
        assert_eq!(chunk_of_voxel(v), IVec3::ZERO);
        let v = IVec3::new(16, 16, 16);
        assert_eq!(chunk_of_voxel(v), IVec3::ONE);
        assert_eq!(local_of_voxel(v), UVec3::ZERO);
    }

    #[test]
    fn chunk_conversion_negative() {
        // 负数：算术右移 = 向负无穷取整，正确覆盖负数坐标。
        let v = IVec3::new(-1, -1, -1);
        assert_eq!(chunk_of_voxel(v), IVec3::NEG_ONE);
        assert_eq!(local_of_voxel(v), UVec3::new(15, 15, 15));

        let v = IVec3::new(-16, -33, -49);
        assert_eq!(chunk_of_voxel(v), IVec3::new(-1, -3, -4));
        assert_eq!(local_of_voxel(v), UVec3::new(0, 15, 15));

        // 负 -> 正往返一致
        for x in -40..40 {
            for z in [-17, 0, 5] {
                let v = IVec3::new(x, -9, z);
                let cc = chunk_of_voxel(v);
                let lc = local_of_voxel(v);
                assert!(lc.x < 16 && lc.y < 16 && lc.z < 16);
                assert_eq!(voxel_of_local(cc, lc), v);
            }
        }
    }

    #[test]
    fn index_roundtrip() {
        for i in 0..CHUNK_VOLUME {
            assert_eq!(local_index(index_local(i)), i);
        }
        // 排布断言：x 最快
        assert_eq!(local_index(UVec3::new(1, 0, 0)), 1);
        assert_eq!(local_index(UVec3::new(0, 0, 1)), 16);
        assert_eq!(local_index(UVec3::new(0, 1, 0)), 256);
    }

    #[test]
    fn world_size_bounds() {
        let s = WorldSize::new(256, 128, 256);
        assert_eq!(s.chunks(), UVec3::new(16, 8, 16));
        assert!(s.contains(IVec3::new(255, 127, 255)));
        assert!(!s.contains(IVec3::new(256, 127, 255)));
        assert!(!s.contains(IVec3::new(-1, 0, 0)));
        assert!(s.contains_chunk(IVec3::new(15, 7, 15)));
        assert!(!s.contains_chunk(IVec3::new(16, 0, 0)));
        assert!(!s.contains_chunk(IVec3::NEG_ONE));
    }
}
