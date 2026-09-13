//! R1 工程加固中的可复用自动检查。
//!
//! 这些检查只依赖权威世界数据与公开相机逻辑，不复制游戏状态；
//! 供验收判定（A03/A04/A08）与单元测试共同调用。

use crate::camera::{apply_interactive_focus_delta, CameraRig, PITCH_MIN};
use crate::coords::{index_local, voxel_of_local, CHUNK_VOLUME};
use crate::generation::generate_chunk;
use crate::noise::mix64;
use crate::world::World;
use bevy::math::IVec3;
use std::path::Path;

// ----------------------------------------------------------------------------
// A03：单 Chunk / 邻域生成 / 正式世界 三方逐体素等价
// ----------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct ChunkEquivalenceReport {
    pub samples: usize,
    pub matches: usize,
    pub first_difference: Option<String>,
}

impl ChunkEquivalenceReport {
    pub fn passed(&self) -> bool {
        self.samples > 0 && self.samples == self.matches && self.first_difference.is_none()
    }
}

/// A03：直接比较"单 Chunk 纯函数生成"、"独立 World 中邻域乱序生成"与
/// "正式世界中对应 Chunk"解码后的 `BlockId`，逐体素三方一致。
/// 样本由 seed 决定随机分布；邻域按哈希序生成且目标 Chunk 最后生成。
pub fn verify_chunk_equivalence(world: &World, requested_samples: usize) -> ChunkEquivalenceReport {
    let mut report = ChunkEquivalenceReport::default();
    let chunk_dims = world.size.chunks();
    let total = (chunk_dims.x * chunk_dims.y * chunk_dims.z) as usize;
    let samples = requested_samples.max(1).min(total.max(1));
    let mut rng = mix64(world.params.seed ^ 0x5231_4348_554E_4B53);

    for sample_index in 0..samples {
        rng = mix64(rng ^ sample_index as u64);
        let cx = (rng % chunk_dims.x as u64) as i32;
        rng = mix64(rng);
        let cy = (rng % chunk_dims.y as u64) as i32;
        rng = mix64(rng);
        let cz = (rng % chunk_dims.z as u64) as i32;
        let cc = IVec3::new(cx, cy, cz);

        // 1) 单 Chunk 纯函数直接生成。
        let isolated = generate_chunk(&world.params, cc, world.size.y);
        // 2) 独立 World 中先乱序生成邻域，目标 Chunk 最后生成。
        let mut neighborhood = World::empty(world.size, world.params.clone());
        let mut neighbors = Vec::new();
        for dy in -1..=1 {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let ncc = cc + IVec3::new(dx, dy, dz);
                    if world.size.contains_chunk(ncc) && ncc != cc {
                        neighbors.push(ncc);
                    }
                }
            }
        }
        // 固定但非空间顺序（哈希排序）：证明生成结果与生成顺序无关。
        neighbors.sort_by_key(|ncc| {
            mix64(
                world.params.seed
                    ^ (ncc.x as u64).wrapping_mul(0x9E37_79B9)
                    ^ (ncc.y as u64).wrapping_mul(0x85EB_CA6B)
                    ^ (ncc.z as u64).wrapping_mul(0xC2B2_AE35),
            )
        });
        for ncc in &neighbors {
            neighborhood.ensure_chunk(*ncc);
        }
        neighborhood.ensure_chunk(cc);

        report.samples += 1;
        let live = &world.chunk(cc).expect("正式世界 Chunk 必须存在").data;
        let neighbor = &neighborhood
            .chunk(cc)
            .expect("邻域世界目标 Chunk 必须存在")
            .data;
        let mut sample_ok = true;
        for index in 0..CHUNK_VOLUME {
            let a = isolated.get_index(index);
            let b = neighbor.get_index(index);
            let c = live.get_index(index);
            if a != b || b != c {
                let local = index_local(index);
                let voxel = voxel_of_local(cc, local);
                report.first_difference = Some(format!(
                    "sample={sample_index} chunk={cc} local={local} world={voxel} isolated={a:?} neighborhood={b:?} live={c:?}"
                ));
                sample_ok = false;
                break;
            }
        }
        if sample_ok {
            report.matches += 1;
        } else {
            break;
        }
    }

    report
}

// ----------------------------------------------------------------------------
// A08：普通交互焦点探针
// ----------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct FocusProbeReport {
    pub checks: usize,
    pub blocked: usize,
    pub failures: usize,
    pub first_failure: Option<String>,
}

impl FocusProbeReport {
    pub fn passed(&self) -> bool {
        self.checks >= 8 && self.blocked >= 8 && self.failures == 0
    }
}

/// A08：用与普通键鼠完全相同的焦点移动函数，向实体墙体/悬崖侧壁发起
/// 斜向移动，验证焦点与收缩后眼位均不得位于实体体素。
pub fn run_interactive_focus_probe(world: &World) -> FocusProbeReport {
    let mut report = FocusProbeReport::default();
    let directions = [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z];

    'scan: for z in (8..world.size.z as i32 - 8).step_by(2) {
        for x in (8..world.size.x as i32 - 8).step_by(2) {
            let top = world.column_height(x, z);
            if top < 8 {
                continue;
            }
            for y in (3..=top.min(world.size.y as i32 - 3)).step_by(2) {
                let solid = IVec3::new(x, y, z);
                if !world.voxel(solid).is_solid() {
                    continue;
                }
                for dir in directions {
                    let air = solid - dir;
                    if !world.size.contains(air) || world.voxel(air).is_solid() {
                        continue;
                    }
                    let start = World::voxel_center(air);
                    let mut rig = CameraRig::new(
                        (
                            world.size.x as f32,
                            world.size.y as f32,
                            world.size.z as f32,
                        ),
                        start,
                        0.8,
                        PITCH_MIN,
                        12.0,
                    );
                    rig.control_locked = false;
                    let delta = dir.as_vec3() * 1.05;
                    let corrected = apply_interactive_focus_delta(&mut rig, world, delta);
                    let clamped = rig.collision_clamped_dist(world);
                    rig.dist = rig.dist.min(clamped);
                    let eye = rig.eye();
                    let eye_voxel = IVec3::new(
                        eye.x.floor() as i32,
                        eye.y.floor() as i32,
                        eye.z.floor() as i32,
                    );
                    let focus_safe = !rig.focus_is_solid(world);
                    let eye_safe =
                        !world.size.contains(eye_voxel) || !world.voxel(eye_voxel).is_solid();
                    report.checks += 1;
                    if corrected {
                        report.blocked += 1;
                    }
                    if !corrected || !focus_safe || !eye_safe {
                        report.failures += 1;
                        if report.first_failure.is_none() {
                            report.first_failure = Some(format!(
                                "solid={solid} air={air} corrected={corrected} focus={:?} eye={eye:?} focus_safe={focus_safe} eye_safe={eye_safe}",
                                rig.focus
                            ));
                        }
                    }
                    if report.checks >= 12 {
                        break 'scan;
                    }
                }
            }
        }
    }

    report
}

// ----------------------------------------------------------------------------
// A04：截图图像有效性
// ----------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct ImageValidity {
    pub width: u32,
    pub height: u32,
    pub luma_mean: f64,
    pub luma_stddev: f64,
    pub quantized_colors: usize,
}

impl ImageValidity {
    pub fn summary(&self) -> String {
        format!(
            "{}x{} 亮度均值 {:.1} 标准差 {:.1} 量化颜色 {}",
            self.width, self.height, self.luma_mean, self.luma_stddev, self.quantized_colors
        )
    }

    /// 基本有效性门槛：分辨率正确、非全黑/全白/单色、
    /// 量化颜色数量达到最低限度（渲染全黑事故曾以"文件 >10KB"漏检）。
    pub fn is_valid(&self, expected: (u32, u32)) -> bool {
        self.width == expected.0
            && self.height == expected.1
            && self.luma_mean > 1.0
            && self.luma_mean < 254.0
            && self.luma_stddev > 1.0
            && self.quantized_colors >= 64
    }
}

/// A04：解码截图并统计亮度与量化颜色，发现空图、全黑图、近单色图与
/// 错误分辨率。统计只负责发现明显坏图，不能替代 Reviewer 目检。
pub fn validate_screenshot(
    path: &Path,
    expected_width: u32,
    expected_height: u32,
) -> Result<ImageValidity, String> {
    let image = image::open(path).map_err(|e| format!("解码失败: {e}"))?;
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    if width != expected_width || height != expected_height {
        return Err(format!(
            "分辨率 {width}x{height} 与窗口 {expected_width}x{expected_height} 不符"
        ));
    }
    let pixels: Vec<_> = rgba.pixels().collect();
    let n = pixels.len() as f64;
    let mut sum = 0f64;
    let mut sum2 = 0f64;
    let mut colors = std::collections::HashSet::new();
    for (i, px) in pixels.iter().enumerate() {
        let l = 0.2126 * px[0] as f64 + 0.7152 * px[1] as f64 + 0.0722 * px[2] as f64;
        sum += l;
        sum2 += l * l;
        // RGB 各 5bit 量化（32768 桶上限），步进采样控制开销。
        if i % 7 == 0 {
            colors.insert(
                ((px[0] as u32) >> 3) << 10 | ((px[1] as u32) >> 3) << 5 | ((px[2] as u32) >> 3),
            );
        }
    }
    let mean = sum / n;
    let stddev = ((sum2 / n).max(0.0) - mean * mean).sqrt();
    Ok(ImageValidity {
        width,
        height,
        luma_mean: mean,
        luma_stddev: stddev,
        quantized_colors: colors.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coords::WorldSize;
    use crate::generation::TerrainParams;

    /// 等价性检查在真实生成的小世界上通过（含洞穴与悬崖形态）。
    #[test]
    fn chunk_equivalence_compares_decoded_voxels() {
        let world = World::generate_all(WorldSize::new(64, 64, 64), TerrainParams::new(11));
        let report = verify_chunk_equivalence(&world, 8);
        assert!(
            report.passed(),
            "三方等价性失败: {:?}",
            report.first_difference
        );
        assert_eq!(report.samples, 8);
        assert_eq!(report.matches, 8);
    }

    /// 交互探针在真实生成的世界上通过（墙体阻挡 + 焦点/眼位安全）。
    #[test]
    fn interactive_focus_probe_passes_on_generated_world() {
        let world = World::generate_all(WorldSize::new(64, 64, 64), TerrainParams::new(11));
        let report = run_interactive_focus_probe(&world);
        assert!(
            report.passed(),
            "探针失败: checks={} blocked={} failures={} {:?}",
            report.checks,
            report.blocked,
            report.failures,
            report.first_failure
        );
    }
}
