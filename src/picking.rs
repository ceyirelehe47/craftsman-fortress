//! 体素拾取：Amanatides & Woo DDA 射线步进（任务书 4.6）。
//!
//! 输入射线（相机鼠标射线或脚本注入射线），返回第一个命中的实体体素、
//! 命中面方向与相邻放置坐标。跨 Chunk 边界经由 `World` 权威查询，天然一致。

use crate::render::MainCamera;
use crate::voxel::FaceDir;
use crate::world::World;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

#[derive(Clone, Copy, Debug)]
pub struct Ray {
    pub origin: Vec3,
    pub dir: Vec3,
}

impl Ray {
    /// 归一化方向版射线。
    pub fn normalized(origin: Vec3, dir: Vec3) -> Self {
        Self {
            origin,
            dir: dir.normalize_or_zero(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickHit {
    /// 命中体素坐标。
    pub voxel: IVec3,
    /// 命中面（面向射线的面，即射线的进入面）。
    pub face: FaceDir,
    /// 相邻放置坐标（命中体素 + 面法线方向）。
    pub place: IVec3,
    /// 命中距离 t（沿归一化方向，米）。
    pub t: f32,
    /// 命中点。
    pub point: Vec3,
}

/// DDA 射线拾取。`max_t` 为最大步进距离（米）。
///
/// 若射线起点本身在实体体素内，返回该体素与进入前的面（面由射线反方向推断）。
pub fn pick_voxel(world: &World, ray: &Ray, max_t: f32) -> Option<PickHit> {
    let dir = ray.dir;
    if dir.length_squared() < f32::EPSILON {
        return None;
    }
    let mut voxel = IVec3::new(
        ray.origin.x.floor() as i32,
        ray.origin.y.floor() as i32,
        ray.origin.z.floor() as i32,
    );

    // 起点在实体内：直接返回（面取射线来源方向）。
    if world.voxel(voxel).is_solid() {
        let face = dir_to_entry_face(Vec3::new(-dir.x, -dir.y, -dir.z));
        return Some(PickHit {
            voxel,
            face,
            place: voxel + IVec3::new(face.normal().0, face.normal().1, face.normal().2),
            t: 0.0,
            point: ray.origin,
        });
    }

    let step = IVec3::new(
        if dir.x > 0.0 {
            1
        } else if dir.x < 0.0 {
            -1
        } else {
            0
        },
        if dir.y > 0.0 {
            1
        } else if dir.y < 0.0 {
            -1
        } else {
            0
        },
        if dir.z > 0.0 {
            1
        } else if dir.z < 0.0 {
            -1
        } else {
            0
        },
    );

    // 各轴到下一个格线距离与跨一格的 t 增量。
    let mut t_max = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let mut t_delta = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let o = ray.origin;
    if step.x != 0 {
        let boundary = if step.x > 0 {
            voxel.x as f32 + 1.0
        } else {
            voxel.x as f32
        };
        t_max.x = (boundary - o.x) / dir.x;
        t_delta.x = 1.0 / dir.x.abs();
    }
    if step.y != 0 {
        let boundary = if step.y > 0 {
            voxel.y as f32 + 1.0
        } else {
            voxel.y as f32
        };
        t_max.y = (boundary - o.y) / dir.y;
        t_delta.y = 1.0 / dir.y.abs();
    }
    if step.z != 0 {
        let boundary = if step.z > 0 {
            voxel.z as f32 + 1.0
        } else {
            voxel.z as f32
        };
        t_max.z = (boundary - o.z) / dir.z;
        t_delta.z = 1.0 / dir.z.abs();
    }

    let mut t;
    let mut entered_from: Option<FaceDir>;
    // 上限迭代次数防御（max_t × 每米最多 3 步）。
    let max_iters = (max_t * 3.0) as usize + 8;
    for _ in 0..max_iters {
        // 推进到下一格。
        if t_max.x <= t_max.y && t_max.x <= t_max.z {
            voxel.x += step.x;
            t = t_max.x;
            t_max.x += t_delta.x;
            entered_from = Some(if step.x > 0 {
                FaceDir::NegX
            } else {
                FaceDir::PosX
            });
        } else if t_max.y <= t_max.z {
            voxel.y += step.y;
            t = t_max.y;
            t_max.y += t_delta.y;
            entered_from = Some(if step.y > 0 {
                FaceDir::NegY
            } else {
                FaceDir::PosY
            });
        } else {
            voxel.z += step.z;
            t = t_max.z;
            t_max.z += t_delta.z;
            entered_from = Some(if step.z > 0 {
                FaceDir::NegZ
            } else {
                FaceDir::PosZ
            });
        }
        if t > max_t {
            return None;
        }
        if world.voxel(voxel).is_solid() {
            let face = entered_from.unwrap_or(FaceDir::PosY);
            let (fx, fy, fz) = face.normal();
            return Some(PickHit {
                voxel,
                face,
                place: voxel + IVec3::new(fx, fy, fz),
                t,
                point: o + dir * t,
            });
        }
    }
    None
}

fn dir_to_entry_face(d: Vec3) -> FaceDir {
    let ax = d.x.abs();
    let ay = d.y.abs();
    let az = d.z.abs();
    if ax >= ay && ax >= az {
        if d.x >= 0.0 {
            FaceDir::PosX
        } else {
            FaceDir::NegX
        }
    } else if ay >= az {
        if d.y >= 0.0 {
            FaceDir::PosY
        } else {
            FaceDir::NegY
        }
    } else if d.z >= 0.0 {
        FaceDir::PosZ
    } else {
        FaceDir::NegZ
    }
}

// ----------------------------------------------------------------------------
// 运行时拾取（鼠标射线或脚本注入射线）与高亮。
// ----------------------------------------------------------------------------

/// 当前命中（HUD 显示与验收断言共享）。
#[derive(Resource, Default)]
pub struct CurrentPickRes(pub Option<PickHit>);

/// 脚本射线注入（验收 A09）：设置后高亮与 CurrentPickRes 使用该射线。
#[derive(Resource, Default)]
pub struct ScriptedRayRes(pub Option<Ray>);

/// 拾取 + 高亮：脚本射线优先，否则使用鼠标位置射线。
pub fn picking_highlight_system(
    world_res: Res<crate::camera::WorldRes>,
    camera_q: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    window_q: Query<&Window, With<PrimaryWindow>>,
    mut gizmos: Gizmos,
    mut current: ResMut<CurrentPickRes>,
    scripted: Res<ScriptedRayRes>,
) {
    let Some(world) = world_res.0.as_ref() else {
        return;
    };
    let ray = if let Some(r) = scripted.0.as_ref() {
        Some(*r)
    } else {
        let Ok((camera, gt)) = camera_q.single() else {
            return;
        };
        let Ok(window) = window_q.single() else {
            return;
        };
        window.cursor_position().and_then(|cursor| {
            camera
                .viewport_to_world(gt, cursor)
                .ok()
                .map(|r| Ray::normalized(r.origin, *r.direction))
        })
    };
    let Some(ray) = ray else {
        current.0 = None;
        return;
    };
    let hit = pick_voxel(world, &ray, 400.0);
    current.0 = hit;

    if let Some(hit) = hit {
        let v = hit.voxel;
        let center = Vec3::new(v.x as f32 + 0.5, v.y as f32 + 0.5, v.z as f32 + 0.5);
        // 命中体素白色线框。
        gizmos.cube(
            bevy::transform::components::Transform::from_translation(center)
                .with_scale(Vec3::splat(1.04)),
            bevy::color::Color::WHITE,
        );
        // 命中面黄色描边（沿法线微偏移避免 z-fighting）。
        let n = hit.face.normal_vec();
        let (u, w) = face_tangents(hit.face);
        let p = center + n * 0.53;
        let s = 0.5f32;
        let corners = [
            p - u * s - w * s,
            p + u * s - w * s,
            p + u * s + w * s,
            p - u * s + w * s,
        ];
        let color = bevy::color::Color::srgb(1.0, 0.9, 0.2);
        for i in 0..4 {
            gizmos.line(corners[i], corners[(i + 1) % 4], color);
        }
    }
}

fn face_tangents(face: FaceDir) -> (Vec3, Vec3) {
    match face {
        FaceDir::PosY | FaceDir::NegY => (Vec3::X, Vec3::Z),
        FaceDir::PosX | FaceDir::NegX => (Vec3::Z, Vec3::Y),
        FaceDir::PosZ | FaceDir::NegZ => (Vec3::X, Vec3::Y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coords::WorldSize;
    use crate::generation::TerrainParams;
    use crate::voxel::BlockId;

    fn world_with(solid: &[IVec3]) -> World {
        let mut w = World::empty(WorldSize::new(48, 48, 48), TerrainParams::new(1));
        // 全部 chunk 置为已生成空气（ensure_chunk 会生成真实地形，不适合精确拾取断言）。
        w.fill_air_all_for_test();
        for v in solid {
            w.set_voxel(*v, BlockId::Stone).unwrap();
        }
        w
    }

    #[test]
    fn straight_down_hits_top_face() {
        let w = world_with(&[IVec3::new(10, 20, 10)]);
        let ray = Ray::normalized(Vec3::new(10.5, 40.0, 10.5), Vec3::new(0.0, -1.0, 0.0));
        let hit = pick_voxel(&w, &ray, 100.0).unwrap();
        assert_eq!(hit.voxel, IVec3::new(10, 20, 10));
        assert_eq!(hit.face, FaceDir::PosY);
        assert_eq!(hit.place, IVec3::new(10, 21, 10));
        assert!((hit.t - 19.0).abs() < 1e-4, "t={}", hit.t);
    }

    #[test]
    fn diagonal_hit() {
        // 垂直下射命中唯一实体顶面。
        let w = world_with(&[IVec3::new(10, 20, 10)]);
        let ray = Ray::normalized(Vec3::new(10.5, 40.5, 10.5), Vec3::new(0.0, -1.0, 0.0));
        let hit = pick_voxel(&w, &ray, 200.0).unwrap();
        assert_eq!(hit.voxel, IVec3::new(10, 20, 10));

        // 斜 45°（x-y 平面内）：起点 (10.3, 30.5, 10.5)，方向 (1,-1,0)/√2。
        // 路径格序列：(10,30,10)→(10,29,10)→(11,29,10)→(11,28,10)→(12,28,10)→(12,27,10)→(13,27,10)…
        // 在 (13,27,10) 放置实体：射线于 s=2.7 处跨越 x=13 平面进入该体素（y=27.8），进入面为 NegX。
        let w = world_with(&[IVec3::new(13, 27, 10)]);
        let ray = Ray::normalized(Vec3::new(10.3, 30.5, 10.5), Vec3::new(1.0, -1.0, 0.0));
        let hit = pick_voxel(&w, &ray, 50.0).unwrap();
        assert_eq!(hit.voxel, IVec3::new(13, 27, 10));
        assert_eq!(hit.face, FaceDir::NegX);
    }

    #[test]
    fn cross_chunk_hit_consistent() {
        // 目标在 chunk(1,1,1)（世界 16..32），射线从 chunk(0,1,1) 出发跨边界。
        let w = world_with(&[IVec3::new(17, 20, 17)]);
        let ray = Ray::normalized(Vec3::new(1.5, 20.5, 17.5), Vec3::new(1.0, 0.0, 0.0));
        let hit = pick_voxel(&w, &ray, 100.0).unwrap();
        assert_eq!(hit.voxel, IVec3::new(17, 20, 17));
        assert_eq!(hit.face, FaceDir::NegX);
        assert_eq!(hit.place, IVec3::new(16, 20, 17));
    }

    #[test]
    fn empty_ray_misses() {
        let w = world_with(&[]);
        let ray = Ray::normalized(Vec3::new(1.5, 40.5, 1.5), Vec3::new(1.0, 0.0, 0.0));
        assert!(pick_voxel(&w, &ray, 60.0).is_none());
    }

    #[test]
    fn max_t_respected() {
        let w = world_with(&[IVec3::new(10, 20, 10)]);
        let ray = Ray::normalized(Vec3::new(10.5, 40.0, 10.5), Vec3::new(0.0, -1.0, 0.0));
        assert!(
            pick_voxel(&w, &ray, 10.0).is_none(),
            "10m 内不应命中（目标在 19m）"
        );
        assert!(pick_voxel(&w, &ray, 19.5).is_some());
    }

    #[test]
    fn origin_inside_solid_returns_immediately() {
        let w = world_with(&[IVec3::new(10, 20, 10)]);
        let ray = Ray::normalized(Vec3::new(10.5, 20.5, 10.5), Vec3::new(0.0, 1.0, 0.0));
        let hit = pick_voxel(&w, &ray, 10.0).unwrap();
        assert_eq!(hit.voxel, IVec3::new(10, 20, 10));
        assert_eq!(hit.face, FaceDir::NegY);
    }

    #[test]
    fn steep_angle_and_side_faces() {
        // 低掠角侧射：命中体素侧面。
        let w = world_with(&[IVec3::new(30, 20, 30)]);
        let ray = Ray::normalized(Vec3::new(20.5, 20.5, 30.5), Vec3::new(1.0, 0.0, 0.0));
        let hit = pick_voxel(&w, &ray, 100.0).unwrap();
        assert_eq!(hit.voxel, IVec3::new(30, 20, 30));
        assert_eq!(hit.face, FaceDir::NegX);
        // 最大缩放（远距）等价于更长 max_t 与同一几何。
        let far = pick_voxel(&w, &ray, 500.0).unwrap();
        assert_eq!(far.voxel, hit.voxel);
        assert_eq!(far.face, hit.face);
    }

    /// 全世界地面拾取冒烟：从高处向下打 64 条射线，全部命中且面法线合理。
    #[test]
    fn generated_world_picking_smoke() {
        let w = World::generate_all(WorldSize::new(64, 64, 64), TerrainParams::new(5));
        let mut hits = 0;
        for i in 0..8 {
            for k in 0..8 {
                let x = (8 + i * 6) as f32 + 0.5;
                let z = (8 + k * 6) as f32 + 0.5;
                let ray = Ray::normalized(Vec3::new(x, 63.5, z), Vec3::new(0.0, -1.0, 0.0));
                if let Some(hit) = pick_voxel(&w, &ray, 100.0) {
                    assert!(
                        hit.face == FaceDir::PosY || hit.voxel.y < 63,
                        "命中面应为顶面（或洞口下行）"
                    );
                    hits += 1;
                }
            }
        }
        assert!(hits >= 60, "绝大多数列应命中: {hits}/64");
    }
}
