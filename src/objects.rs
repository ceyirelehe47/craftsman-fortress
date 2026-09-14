//! R4 独立对象权威层。
//!
//! 对象不进入 Chunk/BlockId，也不使用 ECS Entity 作为持久身份。对象以稳定 ID、
//! 稳定类型、整数锚点和四向旋转描述；占地、净空、支撑与体素冲突由本模块统一判定。

use crate::picking::Ray;
use crate::voxel::BlockId;
use crate::world::World;
use bevy::math::{IVec3, UVec2, Vec3};
use std::collections::{BTreeMap, HashMap};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId(pub u64);

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ObjectKind {
    CampfireBasic,
    TentBasic,
    StonePileSmall,
}

impl ObjectKind {
    pub const ALL: [Self; 3] = [Self::CampfireBasic, Self::TentBasic, Self::StonePileSmall];

    pub const fn type_id(self) -> &'static str {
        match self {
            Self::CampfireBasic => "campfire_basic",
            Self::TentBasic => "tent_basic",
            Self::StonePileSmall => "stone_pile_small",
        }
    }

    pub const fn asset_id(self) -> &'static str {
        match self {
            Self::CampfireBasic => "bonefire",
            Self::TentBasic => "tent",
            Self::StonePileSmall => "stone_03",
        }
    }

    pub const fn footprint(self) -> UVec2 {
        match self {
            // R3 实测 bonefire 归一化后约 2.7m × 2.7m。
            Self::CampfireBasic => UVec2::new(3, 3),
            // R3 实测 tent 约 3.2m × 2.6m，向上取整为 4×3；旋转后交换。
            Self::TentBasic => UVec2::new(4, 3),
            // R3 实测 stone_03 约 4m × 4m。
            Self::StonePileSmall => UVec2::new(4, 4),
        }
    }

    pub const fn clearance(self) -> u32 {
        match self {
            Self::CampfireBasic => 1,
            Self::TentBasic => 3,
            Self::StonePileSmall => 1,
        }
    }

    /// R3 已验证样本的整体缩放；不进入永久存档，仅为当前视觉映射。
    pub const fn visual_scale(self) -> f32 {
        match self {
            Self::CampfireBasic => 0.30,
            Self::TentBasic => 0.20,
            Self::StonePileSmall => 0.40,
        }
    }

    pub fn from_type_id(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.type_id() == value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlacedObject {
    pub id: ObjectId,
    pub kind: ObjectKind,
    /// 占用体积的最小 X/Y/Z 体素坐标；Y 是地面上方第一格。
    pub anchor: IVec3,
    /// 0/1/2/3 = 0/90/180/270°。
    pub yaw_quarters: u8,
}

impl PlacedObject {
    pub fn rotated_footprint(self) -> UVec2 {
        rotated_footprint(self.kind, self.yaw_quarters)
    }

    pub fn volume_bounds(self) -> (IVec3, IVec3) {
        let footprint = self.rotated_footprint();
        let max = self.anchor
            + IVec3::new(
                footprint.x as i32,
                self.kind.clearance() as i32,
                footprint.y as i32,
            );
        (self.anchor, max)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlacementError {
    UnknownType(String),
    InvalidId(u64),
    DuplicateId(ObjectId),
    InvalidYaw(u8),
    IdExhausted,
    OutOfBounds(IVec3),
    ReservedTopLayer(IVec3),
    MissingSupport(IVec3),
    TerrainIntersection(IVec3),
    ObjectOverlap { voxel: IVec3, other: ObjectId },
    ObjectMissing(ObjectId),
    TerrainSupportsObject { voxel: IVec3, object: ObjectId },
    TerrainIntersectsObject { voxel: IVec3, object: ObjectId },
    InvalidNextId { next_id: u64, max_id: u64 },
}

impl fmt::Display for PlacementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownType(value) => write!(f, "unknown object type: {value}"),
            Self::InvalidId(id) => write!(f, "object id must be non-zero: {id}"),
            Self::DuplicateId(id) => write!(f, "duplicate object id: {id}"),
            Self::InvalidYaw(yaw) => write!(f, "yaw quarter must be 0..=3: {yaw}"),
            Self::IdExhausted => write!(f, "object id space exhausted"),
            Self::OutOfBounds(v) => write!(f, "object volume out of bounds at {v}"),
            Self::ReservedTopLayer(v) => write!(f, "object uses reserved top air layer at {v}"),
            Self::MissingSupport(v) => write!(f, "object has no solid support at {v}"),
            Self::TerrainIntersection(v) => write!(f, "object intersects solid terrain at {v}"),
            Self::ObjectOverlap { voxel, other } => {
                write!(f, "object overlaps {other} at {voxel}")
            }
            Self::ObjectMissing(id) => write!(f, "object does not exist: {id}"),
            Self::TerrainSupportsObject { voxel, object } => {
                write!(
                    f,
                    "terrain edit would remove support for {object} at {voxel}"
                )
            }
            Self::TerrainIntersectsObject { voxel, object } => {
                write!(f, "terrain edit would intersect {object} at {voxel}")
            }
            Self::InvalidNextId { next_id, max_id } => {
                write!(
                    f,
                    "next object id {next_id} must be greater than max id {max_id}"
                )
            }
        }
    }
}

impl std::error::Error for PlacementError {}

#[derive(Clone, Debug)]
pub struct ObjectStore {
    next_id: u64,
    objects: BTreeMap<ObjectId, PlacedObject>,
    occupied: HashMap<IVec3, ObjectId>,
    supports: HashMap<IVec3, ObjectId>,
    revision: u64,
}

impl Default for ObjectStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ObjectStore {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            objects: BTreeMap::new(),
            occupied: HashMap::new(),
            supports: HashMap::new(),
            revision: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.objects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    pub fn next_id(&self) -> u64 {
        self.next_id
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn get(&self, id: ObjectId) -> Option<&PlacedObject> {
        self.objects.get(&id)
    }

    pub fn records_sorted(&self) -> Vec<PlacedObject> {
        self.objects.values().copied().collect()
    }

    pub fn object_at_cell(&self, voxel: IVec3) -> Option<ObjectId> {
        self.occupied.get(&voxel).copied()
    }

    pub fn place(
        &mut self,
        world: &World,
        kind: ObjectKind,
        anchor: IVec3,
        yaw_quarters: u8,
    ) -> Result<ObjectId, PlacementError> {
        let id = ObjectId(self.next_id);
        let next = self
            .next_id
            .checked_add(1)
            .ok_or(PlacementError::IdExhausted)?;
        let record = PlacedObject {
            id,
            kind,
            anchor,
            yaw_quarters,
        };
        self.validate_record(world, record, None)?;
        self.insert_indexes(record);
        self.objects.insert(id, record);
        self.next_id = next;
        self.revision = self.revision.wrapping_add(1);
        Ok(id)
    }

    pub fn remove(&mut self, id: ObjectId) -> Option<PlacedObject> {
        let record = self.objects.remove(&id)?;
        self.remove_indexes(record);
        self.revision = self.revision.wrapping_add(1);
        Some(record)
    }

    pub fn move_object(
        &mut self,
        world: &World,
        id: ObjectId,
        anchor: IVec3,
        yaw_quarters: u8,
    ) -> Result<(), PlacementError> {
        let old = *self
            .objects
            .get(&id)
            .ok_or(PlacementError::ObjectMissing(id))?;
        self.remove_indexes(old);
        let new = PlacedObject {
            anchor,
            yaw_quarters,
            ..old
        };
        if let Err(error) = self.validate_record(world, new, Some(id)) {
            self.insert_indexes(old);
            return Err(error);
        }
        self.objects.insert(id, new);
        self.insert_indexes(new);
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    pub fn rotate_object(
        &mut self,
        world: &World,
        id: ObjectId,
        yaw_quarters: u8,
    ) -> Result<(), PlacementError> {
        let anchor = self
            .objects
            .get(&id)
            .ok_or(PlacementError::ObjectMissing(id))?
            .anchor;
        self.move_object(world, id, anchor, yaw_quarters)
    }

    pub fn from_persisted(
        world: &World,
        next_id: u64,
        records: &[PlacedObject],
    ) -> Result<Self, PlacementError> {
        let mut store = Self::new();
        for &record in records {
            if record.id.0 == 0 {
                return Err(PlacementError::InvalidId(0));
            }
            if store.objects.contains_key(&record.id) {
                return Err(PlacementError::DuplicateId(record.id));
            }
            store.validate_record(world, record, None)?;
            store.insert_indexes(record);
            store.objects.insert(record.id, record);
        }
        let max_id = records.iter().map(|record| record.id.0).max().unwrap_or(0);
        if next_id == 0 || next_id <= max_id {
            return Err(PlacementError::InvalidNextId { next_id, max_id });
        }
        store.next_id = next_id;
        store.revision = 1;
        Ok(store)
    }

    pub fn validate_candidate(
        &self,
        world: &World,
        kind: ObjectKind,
        anchor: IVec3,
        yaw_quarters: u8,
        ignore: Option<ObjectId>,
    ) -> Result<(), PlacementError> {
        self.validate_record(
            world,
            PlacedObject {
                id: ignore.unwrap_or(ObjectId(u64::MAX)),
                kind,
                anchor,
                yaw_quarters,
            },
            ignore,
        )
    }

    pub fn validate_all(&self, world: &World) -> Result<(), PlacementError> {
        let mut rebuilt = Self::new();
        for &record in self.objects.values() {
            rebuilt.validate_record(world, record, None)?;
            rebuilt.insert_indexes(record);
            rebuilt.objects.insert(record.id, record);
        }
        Ok(())
    }

    pub fn validate_terrain_edit(
        &self,
        voxel: IVec3,
        new_block: BlockId,
    ) -> Result<(), PlacementError> {
        if new_block.is_solid() {
            if let Some(&object) = self.occupied.get(&voxel) {
                return Err(PlacementError::TerrainIntersectsObject { voxel, object });
            }
        } else if let Some(&object) = self.supports.get(&voxel) {
            return Err(PlacementError::TerrainSupportsObject { voxel, object });
        }
        Ok(())
    }

    pub fn pick(&self, ray: &Ray, max_t: f32) -> Option<(ObjectId, f32)> {
        let mut best: Option<(ObjectId, f32)> = None;
        for record in self.objects.values().copied() {
            let (min_i, max_i) = record.volume_bounds();
            let min = min_i.as_vec3();
            let max = max_i.as_vec3();
            if let Some(t) = ray_aabb(ray, min, max, max_t) {
                if best.map(|(_, old_t)| t < old_t).unwrap_or(true) {
                    best = Some((record.id, t));
                }
            }
        }
        best
    }

    pub fn semantic_hash(&self) -> u64 {
        semantic_hash_records(self.next_id, self.objects.values().copied())
    }

    fn validate_record(
        &self,
        world: &World,
        record: PlacedObject,
        ignore: Option<ObjectId>,
    ) -> Result<(), PlacementError> {
        if record.id.0 == 0 {
            return Err(PlacementError::InvalidId(0));
        }
        if record.yaw_quarters > 3 {
            return Err(PlacementError::InvalidYaw(record.yaw_quarters));
        }
        let footprint = record.rotated_footprint();
        let clearance = record.kind.clearance() as i32;
        for dz in 0..footprint.y as i32 {
            for dx in 0..footprint.x as i32 {
                let support = record.anchor + IVec3::new(dx, -1, dz);
                if !world.size.contains(support) {
                    return Err(PlacementError::OutOfBounds(support));
                }
                if !world.voxel(support).is_solid() {
                    return Err(PlacementError::MissingSupport(support));
                }
                for dy in 0..clearance {
                    let voxel = record.anchor + IVec3::new(dx, dy, dz);
                    if !world.size.contains(voxel) {
                        return Err(PlacementError::OutOfBounds(voxel));
                    }
                    if voxel.y as u32 == world.size.y - 1 {
                        return Err(PlacementError::ReservedTopLayer(voxel));
                    }
                    if world.voxel(voxel).is_solid() {
                        return Err(PlacementError::TerrainIntersection(voxel));
                    }
                    if let Some(&other) = self.occupied.get(&voxel) {
                        if Some(other) != ignore {
                            return Err(PlacementError::ObjectOverlap { voxel, other });
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn insert_indexes(&mut self, record: PlacedObject) {
        let footprint = record.rotated_footprint();
        for dz in 0..footprint.y as i32 {
            for dx in 0..footprint.x as i32 {
                self.supports
                    .insert(record.anchor + IVec3::new(dx, -1, dz), record.id);
                for dy in 0..record.kind.clearance() as i32 {
                    self.occupied
                        .insert(record.anchor + IVec3::new(dx, dy, dz), record.id);
                }
            }
        }
    }

    fn remove_indexes(&mut self, record: PlacedObject) {
        self.occupied.retain(|_, id| *id != record.id);
        self.supports.retain(|_, id| *id != record.id);
    }
}

pub fn rotated_footprint(kind: ObjectKind, yaw_quarters: u8) -> UVec2 {
    let base = kind.footprint();
    if yaw_quarters.is_multiple_of(2) {
        base
    } else {
        UVec2::new(base.y, base.x)
    }
}

pub fn semantic_hash_records(next_id: u64, records: impl IntoIterator<Item = PlacedObject>) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    let mut feed = |bytes: &[u8]| {
        for &byte in bytes {
            h ^= byte as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    feed(&next_id.to_le_bytes());
    for record in records {
        feed(&record.id.0.to_le_bytes());
        feed(record.kind.type_id().as_bytes());
        feed(&[0]);
        feed(&record.anchor.x.to_le_bytes());
        feed(&record.anchor.y.to_le_bytes());
        feed(&record.anchor.z.to_le_bytes());
        feed(&[record.yaw_quarters]);
    }
    h
}

fn ray_aabb(ray: &Ray, min: Vec3, max: Vec3, max_t: f32) -> Option<f32> {
    let mut t_min = 0.0f32;
    let mut t_max = max_t;
    for axis in 0..3 {
        let origin = ray.origin[axis];
        let dir = ray.dir[axis];
        let lo = min[axis];
        let hi = max[axis];
        if dir.abs() < 1e-7 {
            if origin < lo || origin > hi {
                return None;
            }
            continue;
        }
        let inv = 1.0 / dir;
        let mut a = (lo - origin) * inv;
        let mut b = (hi - origin) * inv;
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        t_min = t_min.max(a);
        t_max = t_max.min(b);
        if t_max < t_min {
            return None;
        }
    }
    (t_max >= 0.0 && t_min <= max_t).then_some(t_min.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coords::WorldSize;
    use crate::generation::TerrainParams;

    fn flat_world() -> World {
        let mut world = World::empty(WorldSize::new(32, 32, 32), TerrainParams::new(9));
        world.fill_air_all_for_test();
        for z in 0..32 {
            for x in 0..32 {
                world
                    .set_voxel(IVec3::new(x, 4, z), BlockId::Stone)
                    .unwrap();
            }
        }
        world.clear_all_dirty_for_test();
        world
    }

    #[test]
    fn placement_rotation_overlap_and_support() {
        let world = flat_world();
        let mut store = ObjectStore::new();
        let tent = store
            .place(&world, ObjectKind::TentBasic, IVec3::new(4, 5, 4), 0)
            .unwrap();
        assert_eq!(
            store.get(tent).unwrap().rotated_footprint(),
            UVec2::new(4, 3)
        );
        store.rotate_object(&world, tent, 1).unwrap();
        assert_eq!(
            store.get(tent).unwrap().rotated_footprint(),
            UVec2::new(3, 4)
        );
        assert!(matches!(
            store.place(&world, ObjectKind::CampfireBasic, IVec3::new(4, 5, 4), 0),
            Err(PlacementError::ObjectOverlap { .. })
        ));
        assert!(matches!(
            store.place(&world, ObjectKind::CampfireBasic, IVec3::new(4, 8, 4), 0),
            Err(PlacementError::MissingSupport(_))
        ));
    }

    #[test]
    fn placement_rejects_out_of_bounds_top_layer_and_terrain() {
        let mut world = flat_world();
        let mut store = ObjectStore::new();
        // 越界：帐篷 4×3 footprint 在 X 边缘伸出世界。
        assert!(matches!(
            store.place(&world, ObjectKind::TentBasic, IVec3::new(30, 5, 4), 0),
            Err(PlacementError::OutOfBounds(_))
        ));
        // 世界顶层安全空气层 H-1：在 y=30 立柱上架篝火必然占用第 31 层。
        for z in 4..7 {
            for x in 4..7 {
                world
                    .set_voxel(IVec3::new(x, 30, z), BlockId::Stone)
                    .unwrap();
            }
        }
        assert!(matches!(
            store.place(&world, ObjectKind::CampfireBasic, IVec3::new(4, 31, 4), 0),
            Err(PlacementError::ReservedTopLayer(_))
        ));
        // 净空内地形实体：锚点层悬一块石头。
        world
            .set_voxel(IVec3::new(5, 5, 5), BlockId::Stone)
            .unwrap();
        assert!(matches!(
            store.place(&world, ObjectKind::CampfireBasic, IVec3::new(4, 5, 4), 0),
            Err(PlacementError::TerrainIntersection(_))
        ));
        // 失败无副作用：ID 不前进、存储为空；随后合法放置正常生效。
        assert_eq!(store.next_id(), 1);
        assert!(store.is_empty());
        store
            .place(&world, ObjectKind::CampfireBasic, IVec3::new(8, 5, 8), 0)
            .unwrap();
        assert_eq!(store.next_id(), 2);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn stable_ids_move_delete_and_hash() {
        let world = flat_world();
        let mut a = ObjectStore::new();
        let first = a
            .place(&world, ObjectKind::CampfireBasic, IVec3::new(2, 5, 2), 0)
            .unwrap();
        let second = a
            .place(&world, ObjectKind::StonePileSmall, IVec3::new(10, 5, 10), 0)
            .unwrap();
        assert_eq!(first, ObjectId(1));
        assert_eq!(second, ObjectId(2));
        a.move_object(&world, second, IVec3::new(16, 5, 10), 2)
            .unwrap();
        let records = a.records_sorted();
        let b = ObjectStore::from_persisted(&world, a.next_id(), &records).unwrap();
        assert_eq!(a.semantic_hash(), b.semantic_hash());
        assert_eq!(a.remove(first).unwrap().id, first);
        assert!(a.get(first).is_none());
    }

    #[test]
    fn terrain_edits_cannot_break_object_volume_or_support() {
        let world = flat_world();
        let mut store = ObjectStore::new();
        let id = store
            .place(&world, ObjectKind::CampfireBasic, IVec3::new(7, 5, 7), 0)
            .unwrap();
        assert!(matches!(
            store.validate_terrain_edit(IVec3::new(7, 5, 7), BlockId::Stone),
            Err(PlacementError::TerrainIntersectsObject { object, .. }) if object == id
        ));
        assert!(matches!(
            store.validate_terrain_edit(IVec3::new(7, 4, 7), BlockId::Air),
            Err(PlacementError::TerrainSupportsObject { object, .. }) if object == id
        ));
    }

    #[test]
    fn ray_pick_returns_nearest_object() {
        let world = flat_world();
        let mut store = ObjectStore::new();
        let near = store
            .place(&world, ObjectKind::CampfireBasic, IVec3::new(4, 5, 4), 0)
            .unwrap();
        store
            .place(&world, ObjectKind::CampfireBasic, IVec3::new(4, 5, 12), 0)
            .unwrap();
        let ray = Ray::normalized(Vec3::new(4.5, 5.5, 0.0), Vec3::Z);
        assert_eq!(store.pick(&ray, 100.0).unwrap().0, near);
    }
}
