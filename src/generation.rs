//! 确定性世界生成。
//!
//! 方案：高度场（大陆起伏 + 丘陵 + 山区脊线 + 台地化制造悬崖 + 负向河谷）
//! 叠加三维密度雕刻（两条"意大利面"通道的交集 + 深部大洞室）产生洞穴与洞口。
//!
//! 关键确定性保证：`terrain_height` 与 `carve_cave` 都是仅依赖 `(seed, 世界坐标)`
//! 的纯函数，与 Chunk 生成顺序无关；因此单独生成一个 Chunk 与在邻域中生成的
//! 结果必然逐位一致（验收 A02/A03）。

use crate::chunk::ChunkData;
use crate::coords::{chunk_origin, local_index, CHUNK_SIZE};
use crate::noise::{fbm2, fbm3, mix64, ridged2, smoothstep, value_noise3};
use crate::voxel::BlockId;
use bevy::math::{IVec3, UVec3};

/// 地形生成参数（集中配置；调整会改变世界，等价于生成版本变化）。
#[derive(Clone, Debug)]
pub struct TerrainParams {
    pub seed: u64,
    /// 基础高度。
    pub base_height: f32,
    /// 大陆级起伏。
    pub continent_freq: f32,
    pub continent_amp: f32,
    /// 丘陵。
    pub hills_freq: f32,
    pub hills_amp: f32,
    /// 山区掩码 / 脊线。
    pub mountain_mask_freq: f32,
    pub mountain_mask_lo: f32,
    pub mountain_mask_hi: f32,
    pub mountain_freq: f32,
    pub mountain_amp: f32,
    /// 台地化（悬崖）。
    pub terrace_step: f32,
    pub terrace_plateau: f32,
    /// 河谷。
    pub valley_freq: f32,
    pub valley_threshold: f32,
    pub valley_depth: f32,
    /// 低洼地表沙线（低于该高度的地表为沙）。
    pub sand_level: i32,
    /// 洞穴通道。
    pub cave_freq: f32,
    pub cave_y_freq: f32,
    pub cave_threshold: f32,
    /// 深部大洞室。
    pub cavern_freq: f32,
    pub cavern_y_freq: f32,
    pub cavern_threshold: f32,
    pub cavern_max_y: i32,
    /// 基岩保护层厚度（底部）。
    pub bedrock_layers: i32,
}

impl TerrainParams {
    /// 默认验收参数。
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            base_height: 34.0,
            continent_freq: 0.0055,
            continent_amp: 13.0,
            hills_freq: 0.021,
            hills_amp: 6.5,
            mountain_mask_freq: 0.0042,
            mountain_mask_lo: 0.44,
            mountain_mask_hi: 0.78,
            mountain_freq: 0.013,
            mountain_amp: 78.0,
            terrace_step: 7.0,
            terrace_plateau: 0.5,
            valley_freq: 0.009,
            valley_threshold: 0.62,
            valley_depth: 26.0,
            sand_level: 24,
            cave_freq: 0.045,
            cave_y_freq: 0.062,
            cave_threshold: 0.080,
            cavern_freq: 0.021,
            cavern_y_freq: 0.032,
            cavern_threshold: 0.66,
            cavern_max_y: 44,
            bedrock_layers: 3,
        }
    }
}

impl TerrainParams {
    /// 列地形高度（不含洞穴）：纯函数 of (seed, x, z)。
    pub fn terrain_height(&self, x: i32, z: i32) -> i32 {
        let xf = x as f32;
        let zf = z as f32;
        let s = self.seed;

        // 大陆起伏（低频大波长）。
        let continent = fbm2(
            mix64(s ^ 0xC0FF_EE01),
            xf * self.continent_freq,
            zf * self.continent_freq,
            4,
        );
        let mut h = self.base_height + continent * self.continent_amp;

        // 丘陵。
        let hills = fbm2(
            mix64(s ^ 0x0F0F_0F0F),
            xf * self.hills_freq,
            zf * self.hills_freq,
            3,
        );
        h += hills * self.hills_amp;

        // 山区：低频掩码 × 脊线噪声，再台地化形成悬崖式陡壁。
        let mask_raw = fbm2(
            mix64(s ^ 0xAAAA_0001),
            xf * self.mountain_mask_freq,
            zf * self.mountain_mask_freq,
            3,
        ) * 0.5
            + 0.5;
        let mask = smoothstep(self.mountain_mask_lo, self.mountain_mask_hi, mask_raw);
        if mask > 0.0 {
            let ridge = ridged2(
                mix64(s ^ 0x1234_ABCD),
                xf * self.mountain_freq,
                zf * self.mountain_freq,
                4,
            );
            let mtn = ridge * self.mountain_amp * mask;
            // 台地化：量化到 terrace_step，并压缩台阶内坡度形成平顶 + 陡壁。
            let q = mtn / self.terrace_step;
            let whole = q.floor();
            let frac = q - whole;
            let f2 = smoothstep(self.terrace_plateau, 1.0, frac);
            h += (whole + f2) * self.terrace_step;
        }

        // 河谷：低频掩码负向下切（山区豁免，避免削峰）。
        let valley_raw = fbm2(
            mix64(s ^ 0x5C5C_5C5C),
            xf * self.valley_freq,
            zf * self.valley_freq,
            3,
        ) * 0.5
            + 0.5;
        if valley_raw > self.valley_threshold {
            let t = (valley_raw - self.valley_threshold) / (1.0 - self.valley_threshold);
            h -= t * t * self.valley_depth * (1.0 - mask);
        }

        h.round() as i32
    }

    /// 洞穴雕刻：纯函数 of (seed, x, y, z)。返回 true 表示该体素应为空气。
    pub fn carve_cave(&self, x: i32, y: i32, z: i32) -> bool {
        if y <= self.bedrock_layers {
            return false;
        }
        let s = self.seed;
        let xf = x as f32;
        let yf = y as f32;
        let zf = z as f32;

        // 意大利面通道：两条通道噪声的窄带交集 => 管状隧道，可穿透地表形成洞口。
        let n1 = value_noise3(
            mix64(s ^ 0xCAFE_0001),
            xf * self.cave_freq,
            yf * self.cave_y_freq,
            zf * self.cave_freq,
        ) - 0.5;
        if n1.abs() < self.cave_threshold {
            let n2 = value_noise3(
                mix64(s ^ 0xBEEF_0002),
                xf * self.cave_freq,
                yf * self.cave_y_freq,
                zf * self.cave_freq,
            ) - 0.5;
            if n2.abs() < self.cave_threshold {
                return true;
            }
        }

        // 深部大洞室。
        if y < self.cavern_max_y {
            let cavern = fbm3(
                mix64(s ^ 0xD00D_0003),
                xf * self.cavern_freq,
                yf * self.cavern_y_freq,
                zf * self.cavern_freq,
                3,
            );
            if cavern > self.cavern_threshold {
                return true;
            }
        }
        false
    }

    /// 表层方块选择。
    pub fn surface_block(&self, height: i32) -> BlockId {
        if height <= self.sand_level {
            BlockId::Sand
        } else {
            BlockId::Grass
        }
    }
}

/// 返回确定性生成世界在单个坐标上的基础方块。
/// R2 修改覆盖层用它判断某项编辑是否已恢复为 Seed 世界原值。
pub fn generated_block_at(params: &TerrainParams, voxel: IVec3, world_y: u32) -> BlockId {
    if voxel.y < 0 || voxel.y >= world_y as i32 {
        return BlockId::Air;
    }
    let height = params
        .terrain_height(voxel.x, voxel.z)
        .clamp(1, world_y as i32 - 2);
    if voxel.y > height {
        BlockId::Air
    } else if voxel.y <= params.bedrock_layers {
        BlockId::Bedrock
    } else if params.carve_cave(voxel.x, voxel.y, voxel.z) {
        BlockId::Air
    } else if voxel.y == height {
        params.surface_block(height)
    } else if voxel.y >= height - 3 {
        BlockId::Dirt
    } else {
        BlockId::Stone
    }
}

/// 生成单个 Chunk（纯函数：只依赖参数与 Chunk 坐标）。
pub fn generate_chunk(params: &TerrainParams, cc: IVec3, world_y: u32) -> ChunkData {
    let origin = chunk_origin(cc);
    let y_min = origin.y;
    let bedrock_top = params.bedrock_layers;

    // 整块在基岩层之下（不可能：基岩从 0 开始）或整块在地形之上（快路径：全空气）。
    // 注意：地形高度必须先算出来才能判断，因此按列处理。
    let mut chunk = ChunkData::filled(BlockId::Air);

    // 预计算本 Chunk 覆盖列的地形高度与表层块。
    let mut heights = [[0i32; CHUNK_SIZE]; CHUNK_SIZE];
    let mut tops = [[BlockId::Air; CHUNK_SIZE]; CHUNK_SIZE];
    for lz in 0..CHUNK_SIZE {
        for lx in 0..CHUNK_SIZE {
            let h = params.terrain_height(origin.x + lx as i32, origin.z + lz as i32);
            let h_clamped = h.clamp(1, (world_y as i32) - 2);
            heights[lz][lx] = h_clamped;
            tops[lz][lx] = params.surface_block(h_clamped);
        }
    }
    let max_h = y_max_height(&heights);

    for ly in 0..CHUNK_SIZE {
        let wy = y_min + ly as i32;
        if wy > bedrock_top && wy > max_h {
            // 本层整体高于地形 => 空气层（洞穴只在地形内雕刻）。
            continue;
        }
        for lz in 0..CHUNK_SIZE {
            for lx in 0..CHUNK_SIZE {
                let h = heights[lz][lx];
                let block = if wy > h {
                    // 高于地形：默认空气；洞穴逻辑不适用（本来就不是实体）。
                    BlockId::Air
                } else if wy <= bedrock_top {
                    BlockId::Bedrock
                } else {
                    // 洞穴雕刻（可穿透到地表，形成洞口）。
                    if params.carve_cave(origin.x + lx as i32, wy, origin.z + lz as i32) {
                        BlockId::Air
                    } else if wy == h {
                        tops[lz][lx]
                    } else if wy >= h - 3 {
                        BlockId::Dirt
                    } else {
                        BlockId::Stone
                    }
                };
                if block != BlockId::Air {
                    chunk.set_index(
                        local_index(UVec3::new(lx as u32, ly as u32, lz as u32)),
                        block,
                    );
                }
            }
        }
    }
    chunk
}

fn y_max_height(heights: &[[i32; CHUNK_SIZE]; CHUNK_SIZE]) -> i32 {
    let mut m = i32::MIN;
    for row in heights {
        for &h in row {
            if h > m {
                m = h;
            }
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coords::local_of_voxel;

    /// 单独生成一个 Chunk 与"先算高度再生成"的一致性由纯函数保证；
    /// 这里直接验证：相同输入重复生成逐位一致。
    #[test]
    fn chunk_generation_deterministic() {
        let p = TerrainParams::new(20260912);
        let a = generate_chunk(&p, IVec3::new(3, 2, 5), 128);
        let b = generate_chunk(&p, IVec3::new(3, 2, 5), 128);
        assert_eq!(a.indices(), b.indices());
        assert_eq!(a.palette(), b.palette());
    }

    /// 不同 seed 产生不同地形。
    #[test]
    fn seed_changes_terrain() {
        let p1 = TerrainParams::new(1);
        let p2 = TerrainParams::new(2);
        let mut diff = 0;
        for x in 0..64 {
            for z in 0..64 {
                if p1.terrain_height(x, z) != p2.terrain_height(x, z) {
                    diff += 1;
                }
            }
        }
        assert!(diff > 100, "不同 seed 应产生不同地形，diff={diff}");
    }

    /// 高度场应在合理范围内，且地图上有明显高低差。
    #[test]
    fn height_field_sane() {
        let p = TerrainParams::new(20260912);
        let (mut hmin, mut hmax) = (i32::MAX, i32::MIN);
        for x in (0..256).step_by(2) {
            for z in (0..256).step_by(2) {
                let h = p.terrain_height(x, z);
                hmin = hmin.min(h);
                hmax = hmax.max(h);
                assert!((1..=126).contains(&h), "h={h}");
            }
        }
        assert!(hmax - hmin >= 40, "高低差不足: {hmin}..{hmax}");
    }

    /// 底部 Chunk 必有基岩（y<=bedrock_layers）；顶层 Chunk 全空气。
    #[test]
    fn bottom_bedrock_top_air() {
        let p = TerrainParams::new(20260912);
        let bottom = generate_chunk(&p, IVec3::new(0, 0, 0), 128);
        assert_eq!(
            bottom.get(local_of_voxel(IVec3::new(5, 0, 5))),
            BlockId::Bedrock,
            "y=0 基岩"
        );
        assert_eq!(
            bottom.get(local_of_voxel(IVec3::new(5, 3, 5))),
            BlockId::Bedrock,
            "y=3 仍在基岩保护层（bedrock_layers=3 => y∈[0,3]）"
        );
        // y=4 越过基岩层：应为岩石或被洞穴挖空的空气（两者皆合法），绝不能仍是基岩。
        assert_ne!(
            bottom.get(local_of_voxel(IVec3::new(5, 4, 5))),
            BlockId::Bedrock,
            "y=4 越过基岩层"
        );
        let top = generate_chunk(&p, IVec3::new(0, 7, 0), 128);
        let solid = top
            .indices()
            .iter()
            .filter(|&&i| top.palette()[i as usize] != BlockId::Air)
            .count();
        assert_eq!(solid, 0, "顶层 Chunk 应为空气");
    }

    #[test]
    fn point_generation_matches_chunk_generation() {
        let params = TerrainParams::new(12345);
        for voxel in [
            IVec3::new(0, 0, 0),
            IVec3::new(15, 18, 15),
            IVec3::new(16, 18, 16),
            IVec3::new(31, 40, 7),
        ] {
            let cc = crate::coords::chunk_of_voxel(voxel);
            let chunk = generate_chunk(&params, cc, 64);
            assert_eq!(
                generated_block_at(&params, voxel, 64),
                chunk.get(local_of_voxel(voxel)),
                "point/chunk mismatch at {voxel}"
            );
        }
    }
}
