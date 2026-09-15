//! R5 模块化建筑构件权威层。
//!
//! 建筑构件使用独立稳定 ID、整数网格锚点和四向旋转。墙、门窗、楼板、屋顶、柱梁
//! 都占用“薄表面/边/点”槽位，不进入 `BlockId` 或 Chunk。楼梯拥有离散体积和上下层
//! 入口，为 R6 导航保留确定性语义。

use crate::objects::{ObjectStore, PlacedObject};
use crate::picking::Ray;
use crate::voxel::BlockId;
use crate::world::World;
use bevy::math::{IVec3, Vec3};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

pub const STORY_HEIGHT: i32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BuildingId(pub u64);

impl fmt::Display for BuildingId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuildingKind {
    FloorBasic,
    WallBasic,
    DoorwayBasic,
    WindowBasic,
    StairBasic,
    RoofFlatBasic,
    ColumnBasic,
    BeamBasic,
}

impl BuildingKind {
    pub const ALL: [Self; 8] = [
        Self::FloorBasic,
        Self::WallBasic,
        Self::DoorwayBasic,
        Self::WindowBasic,
        Self::StairBasic,
        Self::RoofFlatBasic,
        Self::ColumnBasic,
        Self::BeamBasic,
    ];

    pub const fn type_id(self) -> &'static str {
        match self {
            Self::FloorBasic => "floor_basic",
            Self::WallBasic => "wall_basic",
            Self::DoorwayBasic => "doorway_basic",
            Self::WindowBasic => "window_basic",
            Self::StairBasic => "stair_basic",
            Self::RoofFlatBasic => "roof_flat_basic",
            Self::ColumnBasic => "column_basic",
            Self::BeamBasic => "beam_basic",
        }
    }

    pub fn from_type_id(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.type_id() == value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlacedBuilding {
    pub id: BuildingId,
    pub kind: BuildingKind,
    pub anchor: IVec3,
    /// 0/1/2/3 = +X/+Z/-X/-Z。
    pub yaw_quarters: u8,
}

impl PlacedBuilding {
    pub fn direction(self) -> IVec3 {
        direction(self.yaw_quarters)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum EdgeAxis {
    X,
    Z,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct EdgeKey {
    start: IVec3,
    axis: EdgeAxis,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SlotKey {
    Horizontal(IVec3),
    Vertical(EdgeKey),
    Post(IVec3),
    Beam(EdgeKey),
    StairCell(IVec3),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuildingError {
    InvalidId(u64),
    DuplicateId(BuildingId),
    InvalidYaw(u8),
    IdExhausted,
    InvalidNextId { next_id: u64, max_id: u64 },
    Missing(BuildingId),
    OutOfBounds(IVec3),
    ReservedTopLayer(IVec3),
    TerrainIntersection(IVec3),
    SlotOccupied { other: BuildingId },
    Unsupported { component: BuildingId, at: IVec3 },
    StairEndpointMissing { component: BuildingId, at: IVec3 },
    ComponentIntersection { a: BuildingId, b: BuildingId },
    ObjectIntersection { component: BuildingId },
    TerrainEditInvalidates { voxel: IVec3 },
}

impl fmt::Display for BuildingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId(id) => write!(f, "building id must be non-zero: {id}"),
            Self::DuplicateId(id) => write!(f, "duplicate building id: {id}"),
            Self::InvalidYaw(yaw) => write!(f, "building yaw quarter must be 0..=3: {yaw}"),
            Self::IdExhausted => write!(f, "building id space exhausted"),
            Self::InvalidNextId { next_id, max_id } => write!(
                f,
                "next building id {next_id} must be greater than max id {max_id}"
            ),
            Self::Missing(id) => write!(f, "building component does not exist: {id}"),
            Self::OutOfBounds(voxel) => write!(f, "building out of bounds near {voxel}"),
            Self::ReservedTopLayer(voxel) => {
                write!(f, "building uses reserved top air layer near {voxel}")
            }
            Self::TerrainIntersection(voxel) => {
                write!(f, "building intersects terrain near {voxel}")
            }
            Self::SlotOccupied { other } => write!(f, "building slot occupied by {other}"),
            Self::Unsupported { component, at } => {
                write!(f, "building {component} is unsupported at {at}")
            }
            Self::StairEndpointMissing { component, at } => {
                write!(f, "stair {component} has no floor endpoint at {at}")
            }
            Self::ComponentIntersection { a, b } => {
                write!(f, "building components {a} and {b} intersect")
            }
            Self::ObjectIntersection { component } => {
                write!(f, "building {component} intersects an independent object")
            }
            Self::TerrainEditInvalidates { voxel } => {
                write!(f, "terrain edit would invalidate buildings at {voxel}")
            }
        }
    }
}

impl std::error::Error for BuildingError {}

#[derive(Clone, Debug)]
pub struct BuildingStore {
    next_id: u64,
    components: BTreeMap<BuildingId, PlacedBuilding>,
    revision: u64,
}

impl Default for BuildingStore {
    fn default() -> Self {
        Self::new()
    }
}

impl BuildingStore {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            components: BTreeMap::new(),
            revision: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.components.len()
    }

    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }

    pub fn next_id(&self) -> u64 {
        self.next_id
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn get(&self, id: BuildingId) -> Option<&PlacedBuilding> {
        self.components.get(&id)
    }

    pub fn records_sorted(&self) -> Vec<PlacedBuilding> {
        self.components.values().copied().collect()
    }

    pub fn place(
        &mut self,
        world: &World,
        objects: &ObjectStore,
        kind: BuildingKind,
        anchor: IVec3,
        yaw_quarters: u8,
    ) -> Result<BuildingId, BuildingError> {
        let id = BuildingId(self.next_id);
        let next_id = self
            .next_id
            .checked_add(1)
            .ok_or(BuildingError::IdExhausted)?;
        let mut records = self.records_sorted();
        records.push(PlacedBuilding {
            id,
            kind,
            anchor,
            yaw_quarters,
        });
        validate_records(world, objects, &records, next_id, None)?;
        self.components.insert(id, *records.last().unwrap());
        self.next_id = next_id;
        self.revision = self.revision.wrapping_add(1);
        Ok(id)
    }

    pub fn move_component(
        &mut self,
        world: &World,
        objects: &ObjectStore,
        id: BuildingId,
        anchor: IVec3,
        yaw_quarters: u8,
    ) -> Result<(), BuildingError> {
        let old = *self.components.get(&id).ok_or(BuildingError::Missing(id))?;
        let mut records = self.records_sorted();
        let index = records
            .iter()
            .position(|record| record.id == id)
            .ok_or(BuildingError::Missing(id))?;
        let updated = PlacedBuilding {
            anchor,
            yaw_quarters,
            ..old
        };
        records[index] = updated;
        validate_records(world, objects, &records, self.next_id, None)?;
        self.components.insert(id, updated);
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    pub fn rotate_component(
        &mut self,
        world: &World,
        objects: &ObjectStore,
        id: BuildingId,
        yaw_quarters: u8,
    ) -> Result<(), BuildingError> {
        let anchor = self
            .components
            .get(&id)
            .ok_or(BuildingError::Missing(id))?
            .anchor;
        self.move_component(world, objects, id, anchor, yaw_quarters)
    }

    pub fn remove(
        &mut self,
        world: &World,
        objects: &ObjectStore,
        id: BuildingId,
    ) -> Result<PlacedBuilding, BuildingError> {
        let old = *self.components.get(&id).ok_or(BuildingError::Missing(id))?;
        let records: Vec<_> = self
            .components
            .values()
            .copied()
            .filter(|record| record.id != id)
            .collect();
        validate_records(world, objects, &records, self.next_id, None)?;
        self.components.remove(&id);
        self.revision = self.revision.wrapping_add(1);
        Ok(old)
    }

    pub fn validate_candidate(
        &self,
        world: &World,
        objects: &ObjectStore,
        kind: BuildingKind,
        anchor: IVec3,
        yaw_quarters: u8,
        ignore: Option<BuildingId>,
    ) -> Result<(), BuildingError> {
        let mut records = self.records_sorted();
        let next_id = if let Some(id) = ignore {
            let old = records
                .iter_mut()
                .find(|record| record.id == id)
                .ok_or(BuildingError::Missing(id))?;
            *old = PlacedBuilding {
                id,
                kind,
                anchor,
                yaw_quarters,
            };
            self.next_id
        } else {
            let next = self
                .next_id
                .checked_add(1)
                .ok_or(BuildingError::IdExhausted)?;
            records.push(PlacedBuilding {
                id: BuildingId(self.next_id),
                kind,
                anchor,
                yaw_quarters,
            });
            next
        };
        validate_records(world, objects, &records, next_id, None)
    }

    pub fn validate_all(&self, world: &World, objects: &ObjectStore) -> Result<(), BuildingError> {
        validate_records(world, objects, &self.records_sorted(), self.next_id, None)
    }

    pub fn validate_terrain_edit(
        &self,
        world: &World,
        objects: &ObjectStore,
        voxel: IVec3,
        new_block: BlockId,
    ) -> Result<(), BuildingError> {
        validate_records(
            world,
            objects,
            &self.records_sorted(),
            self.next_id,
            Some((voxel, new_block)),
        )
        .map_err(|_| BuildingError::TerrainEditInvalidates { voxel })
    }

    pub fn validate_object_record(&self, object: PlacedObject) -> Result<(), BuildingError> {
        let (min, max) = object.volume_bounds();
        let min = min.as_vec3();
        let max = max.as_vec3();
        if let Some(component) = self
            .components
            .values()
            .copied()
            .find(|record| component_intersects_box(*record, min, max))
        {
            return Err(BuildingError::ObjectIntersection {
                component: component.id,
            });
        }
        Ok(())
    }

    pub fn pick(&self, ray: &Ray, max_t: f32) -> Option<(BuildingId, f32)> {
        let mut best: Option<(BuildingId, f32)> = None;
        for record in self.components.values().copied() {
            for (min, max) in visual_boxes(record) {
                if let Some(t) = ray_aabb(ray, min, max, max_t) {
                    if best.map(|(_, old)| t < old).unwrap_or(true) {
                        best = Some((record.id, t));
                    }
                }
            }
        }
        best
    }

    pub fn semantic_hash(&self) -> u64 {
        semantic_hash_records(self.next_id, self.components.values().copied())
    }

    pub fn from_persisted(
        world: &World,
        objects: &ObjectStore,
        next_id: u64,
        records: &[PlacedBuilding],
    ) -> Result<Self, BuildingError> {
        validate_records(world, objects, records, next_id, None)?;
        let components = records.iter().map(|record| (record.id, *record)).collect();
        Ok(Self {
            next_id,
            components,
            revision: 1,
        })
    }
}

pub fn direction(yaw_quarters: u8) -> IVec3 {
    match yaw_quarters & 3 {
        0 => IVec3::X,
        1 => IVec3::Z,
        2 => -IVec3::X,
        _ => -IVec3::Z,
    }
}

pub fn semantic_hash_records(
    next_id: u64,
    records: impl IntoIterator<Item = PlacedBuilding>,
) -> u64 {
    let mut sorted: Vec<_> = records.into_iter().collect();
    sorted.sort_by_key(|record| record.id);
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut feed = |bytes: &[u8]| {
        for &byte in bytes {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    feed(&next_id.to_le_bytes());
    for record in sorted {
        feed(&record.id.0.to_le_bytes());
        feed(record.kind.type_id().as_bytes());
        feed(&[0]);
        feed(&record.anchor.x.to_le_bytes());
        feed(&record.anchor.y.to_le_bytes());
        feed(&record.anchor.z.to_le_bytes());
        feed(&[record.yaw_quarters]);
    }
    hash
}

pub fn visual_boxes(record: PlacedBuilding) -> Vec<(Vec3, Vec3)> {
    let a = record.anchor.as_vec3();
    let d = record.direction().as_vec3();
    match record.kind {
        BuildingKind::FloorBasic => vec![(
            Vec3::new(a.x, a.y - 0.12, a.z),
            Vec3::new(a.x + 1.0, a.y, a.z + 1.0),
        )],
        BuildingKind::RoofFlatBasic => vec![(
            Vec3::new(a.x, a.y - 0.18, a.z),
            Vec3::new(a.x + 1.0, a.y, a.z + 1.0),
        )],
        BuildingKind::WallBasic | BuildingKind::DoorwayBasic | BuildingKind::WindowBasic => {
            let end = a + d;
            let min = Vec3::new(a.x.min(end.x) - 0.06, a.y, a.z.min(end.z) - 0.06);
            let max = Vec3::new(
                a.x.max(end.x) + 0.06,
                a.y + STORY_HEIGHT as f32,
                a.z.max(end.z) + 0.06,
            );
            vec![(min, max)]
        }
        BuildingKind::ColumnBasic => vec![(
            Vec3::new(a.x - 0.09, a.y, a.z - 0.09),
            Vec3::new(a.x + 0.09, a.y + STORY_HEIGHT as f32, a.z + 0.09),
        )],
        BuildingKind::BeamBasic => {
            let end = a + d;
            vec![(
                Vec3::new(a.x.min(end.x) - 0.09, a.y - 0.18, a.z.min(end.z) - 0.09),
                Vec3::new(a.x.max(end.x) + 0.09, a.y, a.z.max(end.z) + 0.09),
            )]
        }
        BuildingKind::StairBasic => stair_step_boxes(record),
    }
}

fn validate_records(
    world: &World,
    objects: &ObjectStore,
    records: &[PlacedBuilding],
    next_id: u64,
    edit_override: Option<(IVec3, BlockId)>,
) -> Result<(), BuildingError> {
    let mut ids = HashSet::new();
    let mut max_id = 0u64;
    for record in records {
        if record.id.0 == 0 {
            return Err(BuildingError::InvalidId(0));
        }
        if !ids.insert(record.id) {
            return Err(BuildingError::DuplicateId(record.id));
        }
        if record.yaw_quarters > 3 {
            return Err(BuildingError::InvalidYaw(record.yaw_quarters));
        }
        max_id = max_id.max(record.id.0);
    }
    if next_id == 0 || next_id <= max_id {
        return Err(BuildingError::InvalidNextId { next_id, max_id });
    }

    let view = SupportView::build(records)?;
    for record in records.iter().copied() {
        validate_terrain(world, record, edit_override)?;
        validate_support(world, record, &view, edit_override)?;
    }

    for (index, a) in records.iter().copied().enumerate() {
        for b in records.iter().copied().skip(index + 1) {
            if (a.kind == BuildingKind::StairBasic || b.kind == BuildingKind::StairBasic)
                && components_intersect(a, b)
            {
                return Err(BuildingError::ComponentIntersection { a: a.id, b: b.id });
            }
        }
    }

    for object in objects.records_sorted() {
        let (min, max) = object.volume_bounds();
        let min = min.as_vec3();
        let max = max.as_vec3();
        if let Some(component) = records
            .iter()
            .copied()
            .find(|record| component_intersects_box(*record, min, max))
        {
            return Err(BuildingError::ObjectIntersection {
                component: component.id,
            });
        }
    }
    Ok(())
}

struct SupportView {
    platforms: HashMap<IVec3, Vec<BuildingId>>,
    floors: HashMap<IVec3, BuildingId>,
    beam_supports: HashMap<IVec3, Vec<BuildingId>>,
    slab_supports: HashMap<IVec3, Vec<BuildingId>>,
}

impl SupportView {
    fn build(records: &[PlacedBuilding]) -> Result<Self, BuildingError> {
        let mut slots = HashMap::new();
        let mut view = Self {
            platforms: HashMap::new(),
            floors: HashMap::new(),
            beam_supports: HashMap::new(),
            slab_supports: HashMap::new(),
        };
        for record in records.iter().copied() {
            for slot in slots_for(record) {
                if let Some(other) = slots.insert(slot, record.id) {
                    return Err(BuildingError::SlotOccupied { other });
                }
            }
            match record.kind {
                BuildingKind::FloorBasic => {
                    view.floors.insert(record.anchor, record.id);
                    for point in horizontal_corners(record.anchor) {
                        push_provider(&mut view.platforms, point, record.id);
                    }
                }
                BuildingKind::WallBasic
                | BuildingKind::DoorwayBasic
                | BuildingKind::WindowBasic => {
                    let (a, b) = edge_endpoints(record);
                    let top_a = a + IVec3::Y * STORY_HEIGHT;
                    let top_b = b + IVec3::Y * STORY_HEIGHT;
                    for point in [top_a, top_b] {
                        push_provider(&mut view.beam_supports, point, record.id);
                        push_provider(&mut view.slab_supports, point, record.id);
                    }
                }
                BuildingKind::ColumnBasic => {
                    let top = record.anchor + IVec3::Y * STORY_HEIGHT;
                    push_provider(&mut view.beam_supports, top, record.id);
                    push_provider(&mut view.slab_supports, top, record.id);
                }
                BuildingKind::BeamBasic => {
                    let (a, b) = edge_endpoints(record);
                    push_provider(&mut view.slab_supports, a, record.id);
                    push_provider(&mut view.slab_supports, b, record.id);
                }
                BuildingKind::StairBasic | BuildingKind::RoofFlatBasic => {}
            }
        }
        Ok(view)
    }
}

fn push_provider(map: &mut HashMap<IVec3, Vec<BuildingId>>, point: IVec3, id: BuildingId) {
    map.entry(point).or_default().push(id);
}

fn provided_by_other(map: &HashMap<IVec3, Vec<BuildingId>>, point: IVec3, id: BuildingId) -> bool {
    map.get(&point)
        .map(|providers| providers.iter().any(|provider| *provider != id))
        .unwrap_or(false)
}

fn validate_support(
    world: &World,
    record: PlacedBuilding,
    view: &SupportView,
    edit_override: Option<(IVec3, BlockId)>,
) -> Result<(), BuildingError> {
    match record.kind {
        BuildingKind::FloorBasic | BuildingKind::RoofFlatBasic => {
            let below = record.anchor - IVec3::Y;
            if voxel(world, edit_override, below).is_solid() {
                return Ok(());
            }
            for corner in horizontal_corners(record.anchor) {
                if !provided_by_other(&view.slab_supports, corner, record.id) {
                    return Err(BuildingError::Unsupported {
                        component: record.id,
                        at: corner,
                    });
                }
            }
        }
        BuildingKind::WallBasic | BuildingKind::DoorwayBasic | BuildingKind::WindowBasic => {
            let (a, b) = edge_endpoints(record);
            for point in [a, b] {
                if !terrain_supports_point(world, edit_override, point)
                    && !provided_by_other(&view.platforms, point, record.id)
                {
                    return Err(BuildingError::Unsupported {
                        component: record.id,
                        at: point,
                    });
                }
            }
        }
        BuildingKind::ColumnBasic => {
            if !terrain_supports_point(world, edit_override, record.anchor)
                && !provided_by_other(&view.platforms, record.anchor, record.id)
            {
                return Err(BuildingError::Unsupported {
                    component: record.id,
                    at: record.anchor,
                });
            }
        }
        BuildingKind::BeamBasic => {
            let (a, b) = edge_endpoints(record);
            for point in [a, b] {
                if !provided_by_other(&view.beam_supports, point, record.id) {
                    return Err(BuildingError::Unsupported {
                        component: record.id,
                        at: point,
                    });
                }
            }
        }
        BuildingKind::StairBasic => {
            let low = record.anchor;
            let high = record.anchor + record.direction() * STORY_HEIGHT + IVec3::Y * STORY_HEIGHT;
            for point in [low, high] {
                if !view
                    .floors
                    .get(&point)
                    .map(|id| *id != record.id)
                    .unwrap_or(false)
                {
                    return Err(BuildingError::StairEndpointMissing {
                        component: record.id,
                        at: point,
                    });
                }
            }
        }
    }
    Ok(())
}

fn validate_terrain(
    world: &World,
    record: PlacedBuilding,
    edit_override: Option<(IVec3, BlockId)>,
) -> Result<(), BuildingError> {
    let top_reserved = world.size.y as i32 - 1;
    if record.anchor.y <= 0 {
        return Err(BuildingError::OutOfBounds(record.anchor));
    }
    match record.kind {
        BuildingKind::FloorBasic | BuildingKind::RoofFlatBasic => {
            require_air(world, edit_override, record.anchor)?;
        }
        BuildingKind::WallBasic | BuildingKind::DoorwayBasic | BuildingKind::WindowBasic => {
            if record.anchor.y + STORY_HEIGHT >= top_reserved {
                return Err(BuildingError::ReservedTopLayer(
                    record.anchor + IVec3::Y * STORY_HEIGHT,
                ));
            }
            let edge = canonical_edge(record);
            for dy in 0..STORY_HEIGHT {
                for cell in edge_adjacent_cells(edge, record.anchor.y + dy) {
                    require_air(world, edit_override, cell)?;
                }
            }
        }
        BuildingKind::ColumnBasic => {
            if record.anchor.y + STORY_HEIGHT >= top_reserved {
                return Err(BuildingError::ReservedTopLayer(
                    record.anchor + IVec3::Y * STORY_HEIGHT,
                ));
            }
            for dy in 0..STORY_HEIGHT {
                for cell in point_adjacent_cells(record.anchor + IVec3::Y * dy) {
                    require_air(world, edit_override, cell)?;
                }
            }
        }
        BuildingKind::BeamBasic => {
            let edge = canonical_edge(record);
            for y in [record.anchor.y - 1, record.anchor.y] {
                for cell in edge_adjacent_cells(edge, y) {
                    require_air(world, edit_override, cell)?;
                }
            }
        }
        BuildingKind::StairBasic => {
            if record.anchor.y + STORY_HEIGHT >= top_reserved {
                return Err(BuildingError::ReservedTopLayer(
                    record.anchor + IVec3::Y * STORY_HEIGHT,
                ));
            }
            for cell in stair_clearance_cells(record) {
                require_air(world, edit_override, cell)?;
            }
        }
    }
    Ok(())
}

fn require_air(
    world: &World,
    edit_override: Option<(IVec3, BlockId)>,
    voxel_pos: IVec3,
) -> Result<(), BuildingError> {
    if !world.size.contains(voxel_pos) {
        return Err(BuildingError::OutOfBounds(voxel_pos));
    }
    if voxel_pos.y == world.size.y as i32 - 1 {
        return Err(BuildingError::ReservedTopLayer(voxel_pos));
    }
    if voxel(world, edit_override, voxel_pos).is_solid() {
        return Err(BuildingError::TerrainIntersection(voxel_pos));
    }
    Ok(())
}

fn voxel(world: &World, edit_override: Option<(IVec3, BlockId)>, pos: IVec3) -> BlockId {
    if let Some((edited, block)) = edit_override {
        if edited == pos {
            return block;
        }
    }
    world.voxel(pos)
}

fn terrain_supports_point(
    world: &World,
    edit_override: Option<(IVec3, BlockId)>,
    point: IVec3,
) -> bool {
    if point.y <= 0 {
        return false;
    }
    let y = point.y - 1;
    [
        IVec3::new(point.x - 1, y, point.z - 1),
        IVec3::new(point.x, y, point.z - 1),
        IVec3::new(point.x - 1, y, point.z),
        IVec3::new(point.x, y, point.z),
    ]
    .into_iter()
    .any(|cell| world.size.contains(cell) && voxel(world, edit_override, cell).is_solid())
}

fn slots_for(record: PlacedBuilding) -> Vec<SlotKey> {
    match record.kind {
        BuildingKind::FloorBasic | BuildingKind::RoofFlatBasic => {
            vec![SlotKey::Horizontal(record.anchor)]
        }
        BuildingKind::WallBasic | BuildingKind::DoorwayBasic | BuildingKind::WindowBasic => {
            vec![SlotKey::Vertical(canonical_edge(record))]
        }
        BuildingKind::ColumnBasic => vec![SlotKey::Post(record.anchor)],
        BuildingKind::BeamBasic => vec![SlotKey::Beam(canonical_edge(record))],
        BuildingKind::StairBasic => (0..STORY_HEIGHT)
            .map(|step| {
                SlotKey::StairCell(record.anchor + record.direction() * step + IVec3::Y * step)
            })
            .collect(),
    }
}

fn canonical_edge(record: PlacedBuilding) -> EdgeKey {
    let end = record.anchor + record.direction();
    let start = if (end.x, end.z) < (record.anchor.x, record.anchor.z) {
        end
    } else {
        record.anchor
    };
    let axis = if end.x != record.anchor.x {
        EdgeAxis::X
    } else {
        EdgeAxis::Z
    };
    EdgeKey { start, axis }
}

fn edge_endpoints(record: PlacedBuilding) -> (IVec3, IVec3) {
    (record.anchor, record.anchor + record.direction())
}

fn horizontal_corners(anchor: IVec3) -> [IVec3; 4] {
    [
        anchor,
        anchor + IVec3::X,
        anchor + IVec3::Z,
        anchor + IVec3::X + IVec3::Z,
    ]
}

fn edge_adjacent_cells(edge: EdgeKey, y: i32) -> [IVec3; 2] {
    match edge.axis {
        EdgeAxis::X => [
            IVec3::new(edge.start.x, y, edge.start.z - 1),
            IVec3::new(edge.start.x, y, edge.start.z),
        ],
        EdgeAxis::Z => [
            IVec3::new(edge.start.x - 1, y, edge.start.z),
            IVec3::new(edge.start.x, y, edge.start.z),
        ],
    }
}

fn point_adjacent_cells(point: IVec3) -> [IVec3; 4] {
    [
        IVec3::new(point.x - 1, point.y, point.z - 1),
        IVec3::new(point.x, point.y, point.z - 1),
        IVec3::new(point.x - 1, point.y, point.z),
        IVec3::new(point.x, point.y, point.z),
    ]
}

fn stair_clearance_cells(record: PlacedBuilding) -> Vec<IVec3> {
    let mut cells = Vec::new();
    for step in 0..STORY_HEIGHT {
        let base = record.anchor + record.direction() * step;
        for dy in 0..STORY_HEIGHT {
            cells.push(base + IVec3::Y * dy);
        }
    }
    cells
}

fn stair_step_boxes(record: PlacedBuilding) -> Vec<(Vec3, Vec3)> {
    let mut boxes = Vec::new();
    for step in 0..STORY_HEIGHT {
        let cell = record.anchor + record.direction() * step;
        let min = Vec3::new(cell.x as f32, record.anchor.y as f32, cell.z as f32);
        let max = Vec3::new(
            cell.x as f32 + 1.0,
            record.anchor.y as f32 + step as f32 + 1.0,
            cell.z as f32 + 1.0,
        );
        boxes.push((min, max));
    }
    boxes
}

fn stair_clearance_boxes(record: PlacedBuilding) -> Vec<(Vec3, Vec3)> {
    let mut boxes = Vec::new();
    for step in 0..STORY_HEIGHT {
        let cell = record.anchor + record.direction() * step;
        boxes.push((
            Vec3::new(cell.x as f32, record.anchor.y as f32, cell.z as f32),
            Vec3::new(
                cell.x as f32 + 1.0,
                record.anchor.y as f32 + STORY_HEIGHT as f32,
                cell.z as f32 + 1.0,
            ),
        ));
    }
    boxes
}

fn components_intersect(a: PlacedBuilding, b: PlacedBuilding) -> bool {
    if a.kind == BuildingKind::StairBasic {
        for (min, max) in stair_clearance_boxes(a) {
            if component_intersects_box(b, min, max) {
                return true;
            }
        }
    }
    if b.kind == BuildingKind::StairBasic {
        for (min, max) in stair_clearance_boxes(b) {
            if component_intersects_box(a, min, max) {
                return true;
            }
        }
    }
    false
}

fn component_intersects_box(record: PlacedBuilding, min: Vec3, max: Vec3) -> bool {
    let a = record.anchor.as_vec3();
    match record.kind {
        BuildingKind::FloorBasic | BuildingKind::RoofFlatBasic => {
            min.y < a.y
                && a.y < max.y
                && intervals_overlap(min.x, max.x, a.x, a.x + 1.0)
                && intervals_overlap(min.z, max.z, a.z, a.z + 1.0)
        }
        BuildingKind::WallBasic | BuildingKind::DoorwayBasic | BuildingKind::WindowBasic => {
            let edge = canonical_edge(record);
            match edge.axis {
                EdgeAxis::X => {
                    min.z < edge.start.z as f32
                        && (edge.start.z as f32) < max.z
                        && intervals_overlap(
                            min.x,
                            max.x,
                            edge.start.x as f32,
                            edge.start.x as f32 + 1.0,
                        )
                        && intervals_overlap(
                            min.y,
                            max.y,
                            record.anchor.y as f32,
                            record.anchor.y as f32 + STORY_HEIGHT as f32,
                        )
                }
                EdgeAxis::Z => {
                    min.x < edge.start.x as f32
                        && (edge.start.x as f32) < max.x
                        && intervals_overlap(
                            min.z,
                            max.z,
                            edge.start.z as f32,
                            edge.start.z as f32 + 1.0,
                        )
                        && intervals_overlap(
                            min.y,
                            max.y,
                            record.anchor.y as f32,
                            record.anchor.y as f32 + STORY_HEIGHT as f32,
                        )
                }
            }
        }
        BuildingKind::ColumnBasic => {
            min.x < a.x
                && a.x < max.x
                && min.z < a.z
                && a.z < max.z
                && intervals_overlap(min.y, max.y, a.y, a.y + STORY_HEIGHT as f32)
        }
        BuildingKind::BeamBasic => {
            let edge = canonical_edge(record);
            if !(min.y < a.y && a.y < max.y) {
                return false;
            }
            match edge.axis {
                EdgeAxis::X => {
                    min.z < edge.start.z as f32
                        && (edge.start.z as f32) < max.z
                        && intervals_overlap(
                            min.x,
                            max.x,
                            edge.start.x as f32,
                            edge.start.x as f32 + 1.0,
                        )
                }
                EdgeAxis::Z => {
                    min.x < edge.start.x as f32
                        && (edge.start.x as f32) < max.x
                        && intervals_overlap(
                            min.z,
                            max.z,
                            edge.start.z as f32,
                            edge.start.z as f32 + 1.0,
                        )
                }
            }
        }
        BuildingKind::StairBasic => stair_clearance_boxes(record)
            .into_iter()
            .any(|(other_min, other_max)| boxes_overlap(min, max, other_min, other_max)),
    }
}

fn intervals_overlap(a0: f32, a1: f32, b0: f32, b1: f32) -> bool {
    a0 < b1 && b0 < a1
}

fn boxes_overlap(a_min: Vec3, a_max: Vec3, b_min: Vec3, b_max: Vec3) -> bool {
    intervals_overlap(a_min.x, a_max.x, b_min.x, b_max.x)
        && intervals_overlap(a_min.y, a_max.y, b_min.y, b_max.y)
        && intervals_overlap(a_min.z, a_max.z, b_min.z, b_max.z)
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
    use crate::objects::ObjectKind;

    fn flat_world() -> World {
        let mut world = World::empty(WorldSize::new(48, 32, 48), TerrainParams::new(55));
        world.fill_air_all_for_test();
        for z in 0..48 {
            for x in 0..48 {
                world
                    .set_voxel(IVec3::new(x, 4, z), BlockId::Stone)
                    .unwrap();
            }
        }
        world.clear_all_dirty_for_test();
        world
    }

    #[test]
    fn canonical_wall_slot_and_stable_ids() {
        let world = flat_world();
        let objects = ObjectStore::new();
        let mut store = BuildingStore::new();
        let floor = store
            .place(
                &world,
                &objects,
                BuildingKind::FloorBasic,
                IVec3::new(5, 5, 5),
                0,
            )
            .unwrap();
        let wall = store
            .place(
                &world,
                &objects,
                BuildingKind::WallBasic,
                IVec3::new(5, 5, 5),
                0,
            )
            .unwrap();
        assert_eq!(floor, BuildingId(1));
        assert_eq!(wall, BuildingId(2));
        assert!(matches!(
            store.place(
                &world,
                &objects,
                BuildingKind::WindowBasic,
                IVec3::new(6, 5, 5),
                2,
            ),
            Err(BuildingError::SlotOccupied { .. })
        ));
    }

    #[test]
    fn elevated_slab_requires_structural_corners() {
        let world = flat_world();
        let objects = ObjectStore::new();
        let mut store = BuildingStore::new();
        assert!(matches!(
            store.place(
                &world,
                &objects,
                BuildingKind::FloorBasic,
                IVec3::new(5, 8, 5),
                0,
            ),
            Err(BuildingError::Unsupported { .. })
        ));
        for (anchor, yaw) in [(IVec3::new(5, 5, 5), 0), (IVec3::new(5, 5, 6), 0)] {
            store
                .place(&world, &objects, BuildingKind::WallBasic, anchor, yaw)
                .unwrap();
        }
        let upper = store
            .place(
                &world,
                &objects,
                BuildingKind::FloorBasic,
                IVec3::new(5, 8, 5),
                0,
            )
            .unwrap();
        assert!(store.remove(&world, &objects, BuildingId(1)).is_err());
        assert!(store.get(upper).is_some());
    }

    #[test]
    fn failed_move_rolls_back_without_revision_or_hash_change() {
        let world = flat_world();
        let objects = ObjectStore::new();
        let mut store = BuildingStore::new();
        let id = store
            .place(
                &world,
                &objects,
                BuildingKind::FloorBasic,
                IVec3::new(5, 5, 5),
                0,
            )
            .unwrap();
        let before = *store.get(id).unwrap();
        let hash = store.semantic_hash();
        let revision = store.revision();
        assert!(store
            .move_component(&world, &objects, id, IVec3::new(5, 8, 5), 1)
            .is_err());
        assert_eq!(*store.get(id).unwrap(), before);
        assert_eq!(store.semantic_hash(), hash);
        assert_eq!(store.revision(), revision);
    }

    #[test]
    fn semantic_hash_is_independent_of_record_input_order() {
        let world = flat_world();
        let objects = ObjectStore::new();
        let mut store = BuildingStore::new();
        store
            .place(
                &world,
                &objects,
                BuildingKind::FloorBasic,
                IVec3::new(5, 5, 5),
                0,
            )
            .unwrap();
        store
            .place(
                &world,
                &objects,
                BuildingKind::WallBasic,
                IVec3::new(5, 5, 5),
                0,
            )
            .unwrap();
        let reversed: Vec<_> = store.records_sorted().into_iter().rev().collect();
        let rebuilt =
            BuildingStore::from_persisted(&world, &objects, store.next_id(), &reversed).unwrap();
        assert_eq!(store.semantic_hash(), rebuilt.semantic_hash());
    }

    #[test]
    fn stair_requires_exact_low_and_high_floor_endpoints() {
        let world = flat_world();
        let objects = ObjectStore::new();
        let mut store = BuildingStore::new();
        store
            .place(
                &world,
                &objects,
                BuildingKind::FloorBasic,
                IVec3::new(5, 5, 5),
                0,
            )
            .unwrap();
        assert!(matches!(
            store.place(
                &world,
                &objects,
                BuildingKind::StairBasic,
                IVec3::new(5, 5, 5),
                0,
            ),
            Err(BuildingError::StairEndpointMissing { .. })
        ));
        for point in [
            IVec3::new(8, 5, 5),
            IVec3::new(9, 5, 5),
            IVec3::new(8, 5, 6),
            IVec3::new(9, 5, 6),
        ] {
            store
                .place(&world, &objects, BuildingKind::ColumnBasic, point, 0)
                .unwrap();
        }
        store
            .place(
                &world,
                &objects,
                BuildingKind::FloorBasic,
                IVec3::new(8, 8, 5),
                0,
            )
            .unwrap();
        store
            .place(
                &world,
                &objects,
                BuildingKind::StairBasic,
                IVec3::new(5, 5, 5),
                0,
            )
            .unwrap();
    }

    #[test]
    fn terrain_and_object_conflicts_are_rejected() {
        let world = flat_world();
        let mut objects = ObjectStore::new();
        objects
            .place(&world, ObjectKind::TentBasic, IVec3::new(20, 5, 20), 0)
            .unwrap();
        let mut store = BuildingStore::new();
        assert!(matches!(
            store.place(
                &world,
                &objects,
                BuildingKind::WallBasic,
                IVec3::new(21, 5, 20),
                1,
            ),
            Err(BuildingError::ObjectIntersection { .. })
        ));
        store
            .place(
                &world,
                &objects,
                BuildingKind::FloorBasic,
                IVec3::new(5, 5, 5),
                0,
            )
            .unwrap();
        assert!(store
            .validate_terrain_edit(&world, &objects, IVec3::new(5, 4, 5), BlockId::Air,)
            .is_err());
    }
}
