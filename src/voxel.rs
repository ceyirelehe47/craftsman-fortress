//! 体素方块类型与视觉注册表。
//!
//! 最小方块集合：空气、表层（草）、土壤、岩石、底部/边界（基岩）、
//! 谷底表层（沙）。语义清楚即可，名称可调整。

/// 方块类型。`repr(u8)`，直接作为 palette 存储值。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BlockId {
    /// 空气。非实体。
    Air = 0,
    /// 表层（草覆盖的地面顶部）。
    Grass = 1,
    /// 土壤（表层之下数米）。
    Dirt = 2,
    /// 岩石（深层主体）。
    Stone = 3,
    /// 底部/边界材料（世界底板，不可被洞穴穿透）。
    Bedrock = 4,
    /// 谷底/低洼地表层（沙）。
    Sand = 5,
}

impl BlockId {
    pub const ALL: [BlockId; 6] = [
        BlockId::Air,
        BlockId::Grass,
        BlockId::Dirt,
        BlockId::Stone,
        BlockId::Bedrock,
        BlockId::Sand,
    ];

    #[inline]
    pub fn try_from_u8(v: u8) -> Option<BlockId> {
        match v {
            0 => Some(BlockId::Air),
            1 => Some(BlockId::Grass),
            2 => Some(BlockId::Dirt),
            3 => Some(BlockId::Stone),
            4 => Some(BlockId::Bedrock),
            5 => Some(BlockId::Sand),
            _ => None,
        }
    }

    #[inline]
    pub fn from_u8(v: u8) -> BlockId {
        Self::try_from_u8(v).unwrap_or_else(|| panic!("未知方块 id: {v}"))
    }

    /// 是否为实体（参与遮挡与拾取）。
    #[inline]
    pub fn is_solid(self) -> bool {
        self != BlockId::Air
    }

    /// sRGB 基色（渲染前转为线性空间）。
    #[inline]
    pub fn srgb_color(self) -> [u8; 3] {
        match self {
            BlockId::Air => [0, 0, 0],
            BlockId::Grass => [104, 156, 66],
            BlockId::Dirt => [126, 90, 60],
            BlockId::Stone => [130, 130, 134],
            BlockId::Bedrock => [46, 46, 50],
            BlockId::Sand => [214, 198, 148],
        }
    }

    /// 线性空间基色（顶点色直接使用）。
    #[inline]
    pub fn linear_color(self) -> [f32; 3] {
        let c = self.srgb_color();
        [
            srgb_to_linear(c[0]),
            srgb_to_linear(c[1]),
            srgb_to_linear(c[2]),
        ]
    }
}

#[inline]
fn srgb_to_linear(c: u8) -> f32 {
    let v = c as f32 / 255.0;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// 六个轴向面。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FaceDir {
    PosX,
    NegX,
    PosY,
    NegY,
    PosZ,
    NegZ,
}

impl FaceDir {
    pub const ALL: [FaceDir; 6] = [
        FaceDir::PosX,
        FaceDir::NegX,
        FaceDir::PosY,
        FaceDir::NegY,
        FaceDir::PosZ,
        FaceDir::NegZ,
    ];

    /// 面外法线（指向空气一侧）。
    #[inline]
    pub fn normal(self) -> (i32, i32, i32) {
        match self {
            FaceDir::PosX => (1, 0, 0),
            FaceDir::NegX => (-1, 0, 0),
            FaceDir::PosY => (0, 1, 0),
            FaceDir::NegY => (0, -1, 0),
            FaceDir::PosZ => (0, 0, 1),
            FaceDir::NegZ => (0, 0, -1),
        }
    }

    /// 面相对方向的单位 `Vec3`（拾取返回、网格生成共用）。
    #[inline]
    pub fn normal_vec(self) -> bevy::math::Vec3 {
        let (x, y, z) = self.normal();
        bevy::math::Vec3::new(x as f32, y as f32, z as f32)
    }

    /// 烘焙面着色系数（顶部最亮、底部最暗、两侧略有差异，增强可读性）。
    #[inline]
    pub fn shade(self) -> f32 {
        match self {
            FaceDir::PosY => 1.00,
            FaceDir::PosX => 0.86,
            FaceDir::NegX => 0.80,
            FaceDir::PosZ => 0.74,
            FaceDir::NegZ => 0.70,
            FaceDir::NegY => 0.55,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_id_roundtrip() {
        for b in BlockId::ALL {
            assert_eq!(BlockId::try_from_u8(b as u8), Some(b));
            assert_eq!(BlockId::from_u8(b as u8), b);
        }
        assert_eq!(BlockId::try_from_u8(255), None);
        assert!(!BlockId::Air.is_solid());
        assert!(BlockId::Bedrock.is_solid());
    }

    #[test]
    fn face_normals_distinct() {
        let all: Vec<_> = FaceDir::ALL.iter().map(|f| f.normal()).collect();
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                assert_ne!(all[i], all[j]);
            }
        }
    }
}
