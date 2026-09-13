//! 验收世界地形特征探测（A04）。
//!
//! 在固定验收预设上自动定位：平原、丘陵、山地、山谷、悬崖、洞口、地下洞穴与
//! 明显高差，并给出可用作相机证据机位的位置。探测结果是纯数据，供测试与
//! 验收控制器共享，保证"不依赖人工反复换 Seed 才找到场景"。

use crate::world::World;
use bevy::math::{IVec3, Vec3};

/// 一次特征命中的位置与说明。
#[derive(Clone, Debug)]
pub struct FeatureHit {
    pub name: &'static str,
    /// 观察中心（世界坐标，米）。
    pub view_center: Vec3,
    /// 命中体素（若适用）。
    pub voxel: Option<IVec3>,
    pub detail: String,
}

/// 探测结果汇总。
#[derive(Clone, Debug, Default)]
pub struct FeatureReport {
    pub plains: Option<FeatureHit>,
    pub hills: Option<FeatureHit>,
    pub mountains: Option<FeatureHit>,
    pub valleys: Option<FeatureHit>,
    pub cliffs: Option<FeatureHit>,
    pub cave_opening: Option<FeatureHit>,
    pub underground_cave: Option<FeatureHit>,
    pub elevation_min: i32,
    pub elevation_max: i32,
}

impl FeatureReport {
    /// A04 要求的八类特征是否齐备。
    pub fn all_present(&self) -> bool {
        self.plains.is_some()
            && self.hills.is_some()
            && self.mountains.is_some()
            && self.valleys.is_some()
            && self.cliffs.is_some()
            && self.cave_opening.is_some()
            && self.underground_cave.is_some()
            && (self.elevation_max - self.elevation_min) >= 40
    }

    pub fn summary(&self) -> String {
        let f = |o: &Option<FeatureHit>| {
            o.as_ref()
                .map(|h| h.detail.clone())
                .unwrap_or_else(|| "缺失".into())
        };
        format!(
            "平原: {} | 丘陵: {} | 山地: {} | 山谷: {} | 悬崖: {} | 洞口: {} | 地下洞穴: {} | 高差: {}m ({}..{})",
            f(&self.plains),
            f(&self.hills),
            f(&self.mountains),
            f(&self.valleys),
            f(&self.cliffs),
            f(&self.cave_opening),
            f(&self.underground_cave),
            self.elevation_max - self.elevation_min,
            self.elevation_min,
            self.elevation_max
        )
    }
}

/// 网格步长：探测采样密度（全量 256×256 列高度扫描代价可接受，直接全扫）。
const SCAN_STEP: i32 = 1;

/// 探测验收世界特征。
pub fn probe_world(world: &World) -> FeatureReport {
    let sx = world.size.x as i32;
    let sz = world.size.z as i32;

    // ---- 高度图 ----
    let mut height = vec![0i32; (sx * sz) as usize];
    let (mut hmin, mut hmax) = (i32::MAX, i32::MIN);
    for z in (0..sz).step_by(SCAN_STEP as usize) {
        for x in (0..sx).step_by(SCAN_STEP as usize) {
            let h = world.column_height(x, z);
            height[(z * sx + x) as usize] = h;
            hmin = hmin.min(h);
            hmax = hmax.max(h);
        }
    }

    let at = |x: i32, z: i32| height[(z * sx + x) as usize];
    let center_of =
        |x: i32, z: i32| Vec3::new(x as f32 + 0.5, at(x, z) as f32 + 0.5, z as f32 + 0.5);

    // ---- 平原：12×12 窗口内高差 ≤ 2 ----
    let mut plains = None;
    'plains: for z in (0..=sz - 12).step_by(4) {
        for x in (0..=sx - 12).step_by(4) {
            let (mut lo, mut hi) = (i32::MAX, i32::MIN);
            for dz in 0..12 {
                for dx in 0..12 {
                    let h = at(x + dx, z + dz);
                    lo = lo.min(h);
                    hi = hi.max(h);
                }
            }
            if hi - lo <= 2 {
                plains = Some(FeatureHit {
                    name: "平原",
                    view_center: center_of(x + 6, z + 6),
                    voxel: Some(IVec3::new(x + 6, at(x + 6, z + 6), z + 6)),
                    detail: format!("12×12 区域高差 {}m @ ({},{})", hi - lo, x + 6, z + 6),
                });
                break 'plains;
            }
        }
    }

    // ---- 丘陵：24×24 窗口高差 8~26 且非山地 ----
    let mut hills = None;
    'hills: for z in (0..=sz - 24).step_by(6) {
        for x in (0..=sx - 24).step_by(6) {
            let (mut lo, mut hi) = (i32::MAX, i32::MIN);
            let mut slope_sum = 0.0f32;
            let mut n = 0.0f32;
            for dz in 0..24 {
                for dx in 0..24 {
                    let h = at(x + dx, z + dz);
                    lo = lo.min(h);
                    hi = hi.max(h);
                }
            }
            for dz in 0..24 {
                for dx in 0..23 {
                    slope_sum += (at(x + dx + 1, z + dz) - at(x + dx, z + dz)).abs() as f32;
                    n += 1.0;
                }
            }
            let range = hi - lo;
            let mean_slope = slope_sum / n;
            if (8..=26).contains(&range) && (0.4..=2.2).contains(&mean_slope) {
                hills = Some(FeatureHit {
                    name: "丘陵",
                    view_center: center_of(x + 12, z + 12),
                    voxel: Some(IVec3::new(x + 12, hi, z + 12)),
                    detail: format!(
                        "24×24 高差 {range}m 平均坡降 {mean_slope:.2} @ ({},{})",
                        x + 12,
                        z + 12
                    ),
                });
                break 'hills;
            }
        }
    }

    // ---- 山地：全域最高峰，且峰值显著高于均值 ----
    let mut peak = (0i32, 0i32);
    let mut peak_h = i32::MIN;
    let mut sum = 0i64;
    for z in 0..sz {
        for x in 0..sx {
            sum += at(x, z) as i64;
            if at(x, z) > peak_h {
                peak_h = at(x, z);
                peak = (x, z);
            }
        }
    }
    let mean_h = (sum / (sx as i64 * sz as i64)) as i32;
    let mountains = if peak_h >= mean_h + 35 && peak_h >= 80 {
        Some(FeatureHit {
            name: "山地",
            view_center: Vec3::new(peak.0 as f32, peak_h as f32, peak.1 as f32),
            voxel: Some(IVec3::new(peak.0, peak_h, peak.1)),
            detail: format!(
                "最高峰 {peak_h}m（均值 {mean_h}m）@ ({},{})",
                peak.0, peak.1
            ),
        })
    } else {
        None
    };

    // ---- 山谷：低洼盆地（局部低点 + 周边环高）----
    let mut valleys = None;
    'valley: for z in (4..sz - 4).step_by(4) {
        for x in (4..sx - 4).step_by(4) {
            let h = at(x, z);
            let rim = [at(x - 4, z), at(x + 4, z), at(x, z - 4), at(x, z + 4)];
            let rim_min = rim.iter().min().copied().unwrap();
            if h <= mean_h - 8 && rim_min >= h + 6 {
                valleys = Some(FeatureHit {
                    name: "山谷",
                    view_center: center_of(x, z),
                    voxel: Some(IVec3::new(x, h, z)),
                    detail: format!("谷底 {h}m 周边 +{}m @ ({x},{z})", rim_min - h),
                });
                break 'valley;
            }
        }
    }

    // ---- 悬崖：相邻列高差 ≥ 8 的成对陡壁 ----
    let mut cliffs = None;
    let mut cliff_pairs = 0i32;
    'cliff: for z in 1..sz - 1 {
        for x in 1..sx - 1 {
            let h = at(x, z);
            let d = [
                (at(x + 1, z) - h).abs(),
                (at(x - 1, z) - h).abs(),
                (at(x, z + 1) - h).abs(),
                (at(x, z - 1) - h).abs(),
            ];
            if d.iter().any(|&v| v >= 8) {
                cliff_pairs += 1;
                if cliffs.is_none() {
                    cliffs = Some(FeatureHit {
                        name: "悬崖",
                        view_center: center_of(x, z),
                        voxel: Some(IVec3::new(x, h, z)),
                        detail: format!("邻列高差 {:?} @ ({x},{z})", d),
                    });
                }
                if cliff_pairs >= 30 {
                    break 'cliff;
                }
            }
        }
    }

    // ---- 洞口：地表开口（本列最高实体显著低于邻域表面，说明顶部被洞穴挖穿，
    //      且开口向下连通足够大的空气腔）----
    let mut cave_opening = None;
    'opening: for z in (2..sz - 2).step_by(2) {
        for x in (2..sx - 2).step_by(2) {
            let h = at(x, z);
            if h < 5 {
                continue;
            }
            // 邻域（8 向）表面显著更高 => 本列顶部被洞穴挖穿形成开口。
            let mut rim_max = i32::MIN;
            for dz in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dz == 0 {
                        continue;
                    }
                    rim_max = rim_max.max(at(x + dx, z + dz));
                }
            }
            if rim_max - h < 4 {
                continue;
            }
            // 表面挖穿深度：本列纯函数地形高度与实际最高实体之差 ≥4，
            // 且中间层全空气（"井"在实体顶之上，向天空开口）。
            let h_terrain = world
                .params
                .terrain_height(x, z)
                .clamp(1, (world.size.y as i32) - 2);
            if h_terrain - h < 4 {
                continue;
            }
            let mut open = true;
            for y in (h + 1)..h_terrain {
                if world.voxel(IVec3::new(x, y, z)).is_solid() {
                    open = false;
                    break;
                }
            }
            if !open {
                continue;
            }
            // 开口向下的空气连通体积。
            let vol = flood_air_volume(world, IVec3::new(x, h + 1, z), 4000);
            if vol >= 60 {
                cave_opening = Some(FeatureHit {
                    name: "洞口",
                    view_center: Vec3::new(x as f32, (h + 2) as f32, z as f32),
                    voxel: Some(IVec3::new(x, h, z)),
                    detail: format!("地表开口：本列顶 {h}m 低于邻域 {rim_max}m（-{}) 连通体积 {vol} @ ({x},{h},{z})", rim_max - h),
                });
                break 'opening;
            }
        }
    }

    // ---- 地下洞穴：深处封闭空气腔（体积 ≥ 200，距地表 ≥ 6）----
    let mut underground_cave = None;
    'cave: for z in (8..sz - 8).step_by(4) {
        for x in (8..sx - 8).step_by(4) {
            let h = at(x, z);
            for y in (6..h - 6).step_by(3) {
                if !world.voxel(IVec3::new(x, y, z)).is_solid() {
                    let vol = flood_air_volume(world, IVec3::new(x, y, z), 5000);
                    if vol >= 200 {
                        // 找腔内一个合适的观察点（3×3×3 空气邻域）
                        if let Some(vp) = find_interior_viewpoint(world, IVec3::new(x, y, z)) {
                            underground_cave = Some(FeatureHit {
                                name: "地下洞穴",
                                view_center: vp.0,
                                voxel: Some(vp.1),
                                detail: format!(
                                    "封闭腔体积 {vol} @ ({x},{y},{z})，内景位 {:?}",
                                    vp.1
                                ),
                            });
                            break 'cave;
                        }
                    }
                }
            }
        }
    }

    FeatureReport {
        plains,
        hills,
        mountains,
        valleys,
        cliffs,
        cave_opening,
        underground_cave,
        elevation_min: hmin,
        elevation_max: hmax,
    }
}

/// 空气连通体积（限界洪泛，防止 runaway）。
fn flood_air_volume(world: &World, start: IVec3, limit: usize) -> usize {
    use std::collections::VecDeque;
    let mut visited = std::collections::HashSet::new();
    let mut queue = VecDeque::new();
    if world.voxel(start).is_solid() {
        return 0;
    }
    queue.push_back(start);
    visited.insert(start);
    while let Some(v) = queue.pop_front() {
        if visited.len() >= limit {
            return visited.len();
        }
        for d in [
            (1, 0, 0),
            (-1, 0, 0),
            (0, 1, 0),
            (0, -1, 0),
            (0, 0, 1),
            (0, 0, -1),
        ] {
            let n = v + IVec3::new(d.0, d.1, d.2);
            if !world.size.contains(n) || n.y < 0 {
                continue;
            }
            if world.voxel(n).is_solid() {
                continue;
            }
            if visited.insert(n) {
                queue.push_back(n);
            }
        }
    }
    visited.len()
}

/// 在空气腔内找一个足够宽敞的位置作为内景观察点（focus 中心）。
/// 要求 5×3×5 全空气（x/z ±2，y ±1），保证轨道相机（最小 1.5m）也在空气中。
/// 返回 (世界坐标中心, 体素)。
fn find_interior_viewpoint(world: &World, near: IVec3) -> Option<(Vec3, IVec3)> {
    for dy in -8..=8 {
        for dz in -8..=8 {
            for dx in -8..=8 {
                let c = near + IVec3::new(dx, dy, dz);
                if c.x < 3
                    || c.y < 4
                    || c.z < 3
                    || c.x >= world.size.x as i32 - 3
                    || c.z >= world.size.z as i32 - 3
                    || c.y >= world.size.y as i32 - 2
                {
                    continue;
                }
                let mut ok = true;
                for oy in -1..=1 {
                    for oz in -2..=2 {
                        for ox in -2..=2 {
                            if world.voxel(c + IVec3::new(ox, oy, oz)).is_solid() {
                                ok = false;
                            }
                        }
                    }
                }
                if ok {
                    return Some((World::voxel_center(c), c));
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coords::WorldSize;
    use crate::generation::TerrainParams;

    /// 默认验收种子必须包含全部八类特征（A04 的回归防线）。
    #[test]
    fn acceptance_seed_has_all_features() {
        let seed = crate::config::DEFAULT_SEED;
        let world = World::generate_all(WorldSize::new(256, 128, 256), TerrainParams::new(seed));
        let report = probe_world(&world);
        assert!(
            report.all_present(),
            "验收世界特征缺失: {}",
            report.summary()
        );
    }
}
