//! R5 建筑构件伴随存档。
//!
//! 地形继续使用 CFSAVE02，对象继续使用 CFOBJ001。建筑构件写入
//! `<terrain-logical>.buildings.slot0/.slot1`，每个槽同时通过地形语义哈希与对象
//! 语义哈希和当前两层权威状态配对，避免同一地形下不同对象状态误配旧建筑槽。

use crate::buildings::{
    semantic_hash_records, BuildingId, BuildingKind, BuildingStore, PlacedBuilding,
};
use crate::coords::WorldSize;
use crate::objects::ObjectStore;
use crate::world::World;
use bevy::math::IVec3;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MAGIC: [u8; 8] = *b"CFBLD001";
pub const BUILDING_FORMAT_VERSION: u32 = 1;
pub const BUILDING_SCHEMA_REVISION: u32 = 1;
const HEADER_LEN: usize = 80;
const TYPE_ID_LEN: usize = 24;
const RECORD_LEN: usize = 48;
const TRAILER_LEN: usize = 8;
pub const MAX_BUILDING_COUNT: u32 = 100_000;
const MAX_SAVE_BYTES: u64 =
    HEADER_LEN as u64 + MAX_BUILDING_COUNT as u64 * RECORD_LEN as u64 + TRAILER_LEN as u64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildingSaveMeta {
    pub format_version: u32,
    pub schema_revision: u32,
    pub generation: u64,
    pub seed: u64,
    pub size: WorldSize,
    pub component_count: u32,
    pub next_id: u64,
    pub terrain_semantic_hash: u64,
    pub object_semantic_hash: u64,
    pub building_semantic_hash: u64,
}

pub struct LoadedBuildings {
    pub store: BuildingStore,
    pub meta: BuildingSaveMeta,
    pub slot_path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct BuildingSaveReceipt {
    pub generation: u64,
    pub slot_path: PathBuf,
    pub bytes: u64,
    pub component_count: usize,
    pub terrain_semantic_hash: u64,
    pub object_semantic_hash: u64,
    pub building_semantic_hash: u64,
}

#[derive(Debug)]
pub enum BuildingSaveError {
    Io(std::io::Error),
    Invalid(String),
    NoValidSlot {
        logical: PathBuf,
        details: Vec<String>,
    },
}

impl fmt::Display for BuildingSaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::Invalid(error) => write!(f, "invalid building save: {error}"),
            Self::NoValidSlot { logical, details } => {
                write!(f, "no valid building slot for {}", logical.display())?;
                if !details.is_empty() {
                    write!(f, ": {}", details.join(" | "))?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for BuildingSaveError {}

impl From<std::io::Error> for BuildingSaveError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Clone, Debug)]
struct DecodedBuildingSave {
    meta: BuildingSaveMeta,
    records: Vec<PlacedBuilding>,
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take<const N: usize>(&mut self) -> Result<[u8; N], BuildingSaveError> {
        let end = self
            .pos
            .checked_add(N)
            .ok_or_else(|| BuildingSaveError::Invalid("offset overflow".into()))?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or_else(|| BuildingSaveError::Invalid("unexpected end of file".into()))?;
        self.pos = end;
        let mut out = [0u8; N];
        out.copy_from_slice(slice);
        Ok(out)
    }

    fn u8(&mut self) -> Result<u8, BuildingSaveError> {
        Ok(self.take::<1>()?[0])
    }

    fn u32(&mut self) -> Result<u32, BuildingSaveError> {
        Ok(u32::from_le_bytes(self.take()?))
    }

    fn i32(&mut self) -> Result<i32, BuildingSaveError> {
        Ok(i32::from_le_bytes(self.take()?))
    }

    fn u64(&mut self) -> Result<u64, BuildingSaveError> {
        Ok(u64::from_le_bytes(self.take()?))
    }
}

pub fn building_logical_path(terrain_logical: &Path) -> PathBuf {
    append_suffix(terrain_logical, ".buildings")
}

pub fn slot_paths(logical: &Path) -> [PathBuf; 2] {
    [
        append_suffix(logical, ".slot0"),
        append_suffix(logical, ".slot1"),
    ]
}

fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

fn temp_path(slot: &Path) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    append_suffix(slot, &format!(".tmp-{}-{stamp}", std::process::id()))
}

fn checksum64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn encode_type_id(kind: BuildingKind) -> [u8; TYPE_ID_LEN] {
    let mut out = [0u8; TYPE_ID_LEN];
    let bytes = kind.type_id().as_bytes();
    debug_assert!(bytes.len() <= TYPE_ID_LEN);
    out[..bytes.len()].copy_from_slice(bytes);
    out
}

fn decode_type_id(bytes: [u8; TYPE_ID_LEN]) -> Result<BuildingKind, BuildingSaveError> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(TYPE_ID_LEN);
    if bytes[end..].iter().any(|byte| *byte != 0) {
        return Err(BuildingSaveError::Invalid(
            "building type id has non-zero bytes after terminator".into(),
        ));
    }
    let value = std::str::from_utf8(&bytes[..end])
        .map_err(|_| BuildingSaveError::Invalid("building type id is not UTF-8".into()))?;
    BuildingKind::from_type_id(value)
        .ok_or_else(|| BuildingSaveError::Invalid(format!("unknown building type: {value}")))
}

fn encode_store(
    world: &World,
    objects: &ObjectStore,
    store: &BuildingStore,
    generation: u64,
) -> Result<Vec<u8>, BuildingSaveError> {
    store
        .validate_all(world, objects)
        .map_err(|error| BuildingSaveError::Invalid(error.to_string()))?;
    let records = store.records_sorted();
    let component_count = u32::try_from(records.len())
        .map_err(|_| BuildingSaveError::Invalid("too many building components".into()))?;
    if component_count > MAX_BUILDING_COUNT {
        return Err(BuildingSaveError::Invalid(format!(
            "building count {component_count} exceeds limit {MAX_BUILDING_COUNT}"
        )));
    }
    let record_bytes = records
        .len()
        .checked_mul(RECORD_LEN)
        .ok_or_else(|| BuildingSaveError::Invalid("building record size overflow".into()))?;
    let capacity = HEADER_LEN
        .checked_add(record_bytes)
        .and_then(|value| value.checked_add(TRAILER_LEN))
        .ok_or_else(|| BuildingSaveError::Invalid("building save size overflow".into()))?;
    let mut out = Vec::with_capacity(capacity);
    out.extend_from_slice(&MAGIC);
    push_u32(&mut out, BUILDING_FORMAT_VERSION);
    push_u32(&mut out, BUILDING_SCHEMA_REVISION);
    push_u64(&mut out, generation);
    push_u64(&mut out, world.params.seed);
    push_u32(&mut out, world.size.x);
    push_u32(&mut out, world.size.y);
    push_u32(&mut out, world.size.z);
    push_u32(&mut out, component_count);
    push_u64(&mut out, store.next_id());
    push_u64(&mut out, world.semantic_hash());
    push_u64(&mut out, objects.semantic_hash());
    push_u64(&mut out, store.semantic_hash());
    debug_assert_eq!(out.len(), HEADER_LEN);

    for record in records {
        push_u64(&mut out, record.id.0);
        out.extend_from_slice(&encode_type_id(record.kind));
        push_i32(&mut out, record.anchor.x);
        push_i32(&mut out, record.anchor.y);
        push_i32(&mut out, record.anchor.z);
        out.push(record.yaw_quarters);
        out.extend_from_slice(&[0, 0, 0]);
    }
    let checksum = checksum64(&out);
    push_u64(&mut out, checksum);
    Ok(out)
}

fn decode_bytes(bytes: &[u8]) -> Result<DecodedBuildingSave, BuildingSaveError> {
    if bytes.len() < HEADER_LEN + TRAILER_LEN {
        return Err(BuildingSaveError::Invalid("file is truncated".into()));
    }
    if bytes.len() as u64 > MAX_SAVE_BYTES {
        return Err(BuildingSaveError::Invalid(format!(
            "file size {} exceeds limit {MAX_SAVE_BYTES}",
            bytes.len()
        )));
    }
    let data_len = bytes.len() - TRAILER_LEN;
    let expected_checksum = u64::from_le_bytes(
        bytes[data_len..]
            .try_into()
            .map_err(|_| BuildingSaveError::Invalid("checksum trailer missing".into()))?,
    );
    let actual_checksum = checksum64(&bytes[..data_len]);
    if actual_checksum != expected_checksum {
        return Err(BuildingSaveError::Invalid(format!(
            "checksum mismatch: expected {expected_checksum:#x}, got {actual_checksum:#x}"
        )));
    }

    let mut reader = Reader::new(&bytes[..data_len]);
    if reader.take::<8>()? != MAGIC {
        return Err(BuildingSaveError::Invalid("bad magic".into()));
    }
    let format_version = reader.u32()?;
    if format_version != BUILDING_FORMAT_VERSION {
        return Err(BuildingSaveError::Invalid(format!(
            "unsupported building format version {format_version}"
        )));
    }
    let schema_revision = reader.u32()?;
    if schema_revision != BUILDING_SCHEMA_REVISION {
        return Err(BuildingSaveError::Invalid(format!(
            "unsupported building schema revision {schema_revision}"
        )));
    }
    let generation = reader.u64()?;
    let seed = reader.u64()?;
    let size = WorldSize::new(reader.u32()?, reader.u32()?, reader.u32()?);
    let component_count = reader.u32()?;
    if component_count > MAX_BUILDING_COUNT {
        return Err(BuildingSaveError::Invalid(format!(
            "building count {component_count} exceeds limit {MAX_BUILDING_COUNT}"
        )));
    }
    let next_id = reader.u64()?;
    let terrain_semantic_hash = reader.u64()?;
    let object_semantic_hash = reader.u64()?;
    let building_semantic_hash = reader.u64()?;

    let record_bytes = (component_count as usize)
        .checked_mul(RECORD_LEN)
        .ok_or_else(|| BuildingSaveError::Invalid("record length overflow".into()))?;
    let expected_len = HEADER_LEN
        .checked_add(record_bytes)
        .ok_or_else(|| BuildingSaveError::Invalid("record length overflow".into()))?;
    if expected_len != data_len {
        return Err(BuildingSaveError::Invalid(format!(
            "length mismatch: header says {component_count} components, bytes={}",
            bytes.len()
        )));
    }

    let mut records = Vec::with_capacity(component_count as usize);
    let mut previous_id = 0u64;
    for _ in 0..component_count {
        let id = reader.u64()?;
        if id == 0 || id <= previous_id {
            return Err(BuildingSaveError::Invalid(format!(
                "building ids must be strictly increasing and non-zero: {previous_id} -> {id}"
            )));
        }
        previous_id = id;
        let kind = decode_type_id(reader.take::<TYPE_ID_LEN>()?)?;
        let anchor = IVec3::new(reader.i32()?, reader.i32()?, reader.i32()?);
        let yaw_quarters = reader.u8()?;
        if yaw_quarters > 3 {
            return Err(BuildingSaveError::Invalid(format!(
                "invalid yaw quarter {yaw_quarters} for building {id}"
            )));
        }
        if reader.take::<3>()? != [0, 0, 0] {
            return Err(BuildingSaveError::Invalid("non-zero reserved bytes".into()));
        }
        records.push(PlacedBuilding {
            id: BuildingId(id),
            kind,
            anchor,
            yaw_quarters,
        });
    }
    let calculated_hash = semantic_hash_records(next_id, records.iter().copied());
    if calculated_hash != building_semantic_hash {
        return Err(BuildingSaveError::Invalid(format!(
            "building semantic hash mismatch: save={building_semantic_hash:#x}, decoded={calculated_hash:#x}"
        )));
    }

    Ok(DecodedBuildingSave {
        meta: BuildingSaveMeta {
            format_version,
            schema_revision,
            generation,
            seed,
            size,
            component_count,
            next_id,
            terrain_semantic_hash,
            object_semantic_hash,
            building_semantic_hash,
        },
        records,
    })
}

fn read_decoded(path: &Path) -> Result<DecodedBuildingSave, BuildingSaveError> {
    let mut file = File::open(path)?;
    let file_len = file.metadata()?.len();
    if file_len > MAX_SAVE_BYTES {
        return Err(BuildingSaveError::Invalid(format!(
            "file size {file_len} exceeds limit {MAX_SAVE_BYTES}"
        )));
    }
    let capacity = usize::try_from(file_len)
        .map_err(|_| BuildingSaveError::Invalid("file size does not fit memory index".into()))?;
    let mut bytes = Vec::with_capacity(capacity);
    file.read_to_end(&mut bytes)?;
    decode_bytes(&bytes)
}

fn materialize(
    decoded: DecodedBuildingSave,
    slot_path: PathBuf,
    world: &World,
    objects: &ObjectStore,
) -> Result<LoadedBuildings, BuildingSaveError> {
    if decoded.meta.seed != world.params.seed || decoded.meta.size != world.size {
        return Err(BuildingSaveError::Invalid(format!(
            "world identity mismatch: save seed={} size={:?}, current seed={} size={:?}",
            decoded.meta.seed, decoded.meta.size, world.params.seed, world.size
        )));
    }
    let terrain_hash = world.semantic_hash();
    if decoded.meta.terrain_semantic_hash != terrain_hash {
        return Err(BuildingSaveError::Invalid(format!(
            "terrain hash mismatch: save={:#x}, current={terrain_hash:#x}",
            decoded.meta.terrain_semantic_hash
        )));
    }
    let object_hash = objects.semantic_hash();
    if decoded.meta.object_semantic_hash != object_hash {
        return Err(BuildingSaveError::Invalid(format!(
            "object hash mismatch: save={:#x}, current={object_hash:#x}",
            decoded.meta.object_semantic_hash
        )));
    }
    let store =
        BuildingStore::from_persisted(world, objects, decoded.meta.next_id, &decoded.records)
            .map_err(|error| BuildingSaveError::Invalid(error.to_string()))?;
    let building_hash = store.semantic_hash();
    if building_hash != decoded.meta.building_semantic_hash {
        return Err(BuildingSaveError::Invalid(format!(
            "materialized building hash mismatch: save={:#x}, loaded={building_hash:#x}",
            decoded.meta.building_semantic_hash
        )));
    }
    Ok(LoadedBuildings {
        store,
        meta: decoded.meta,
        slot_path,
    })
}

/// 没有建筑槽时返回 `Ok(None)`，用于兼容 R4 及更早的地形/对象存档。
pub fn load_latest(
    logical: &Path,
    world: &World,
    objects: &ObjectStore,
) -> Result<Option<LoadedBuildings>, BuildingSaveError> {
    let paths = slot_paths(logical);
    if paths.iter().all(|path| !path.exists()) {
        return Ok(None);
    }
    let mut best: Option<LoadedBuildings> = None;
    let mut details = Vec::new();
    for path in paths {
        if !path.exists() {
            details.push(format!("{}: missing", path.display()));
            continue;
        }
        match read_decoded(&path)
            .and_then(|decoded| materialize(decoded, path.clone(), world, objects))
        {
            Ok(loaded) => {
                let replace = best
                    .as_ref()
                    .map(|current| loaded.meta.generation > current.meta.generation)
                    .unwrap_or(true);
                if replace {
                    best = Some(loaded);
                }
            }
            Err(error) => details.push(format!("{}: {error}", path.display())),
        }
    }
    best.map(Some)
        .ok_or_else(|| BuildingSaveError::NoValidSlot {
            logical: logical.to_path_buf(),
            details,
        })
}

/// 保存建筑槽。受保护配对是当前磁盘耐久地形及其对象层；与其匹配的完整有效建筑槽不得被覆盖。
pub fn save_atomic(
    logical: &Path,
    world: &World,
    objects: &ObjectStore,
    store: &BuildingStore,
    protected: Option<(&World, &ObjectStore)>,
) -> Result<BuildingSaveReceipt, BuildingSaveError> {
    let paths = slot_paths(logical);
    let mut decoded_slots: Vec<(u64, usize, DecodedBuildingSave)> = Vec::new();
    for (slot, path) in paths.iter().enumerate() {
        if let Ok(decoded) = read_decoded(path) {
            let current_ok = materialize(decoded.clone(), path.clone(), world, objects).is_ok();
            let protected_ok = protected
                .map(|(old_world, old_objects)| {
                    materialize(decoded.clone(), path.clone(), old_world, old_objects).is_ok()
                })
                .unwrap_or(false);
            if current_ok || protected_ok {
                decoded_slots.push((decoded.meta.generation, slot, decoded));
            }
        }
    }
    let next_generation = decoded_slots
        .iter()
        .map(|(generation, _, _)| *generation)
        .max()
        .map(|generation| {
            generation
                .checked_add(1)
                .ok_or_else(|| BuildingSaveError::Invalid("generation overflow".into()))
        })
        .transpose()?
        .unwrap_or(1);

    let protected_slot = protected.and_then(|(old_world, old_objects)| {
        decoded_slots
            .iter()
            .filter_map(|(generation, slot, decoded)| {
                materialize(
                    decoded.clone(),
                    paths[*slot].clone(),
                    old_world,
                    old_objects,
                )
                .ok()
                .map(|_| (*generation, *slot))
            })
            .max_by_key(|(generation, _)| *generation)
            .map(|(_, slot)| slot)
    });
    let latest_slot = decoded_slots
        .iter()
        .max_by_key(|(generation, _, _)| *generation)
        .map(|(_, slot, _)| *slot);
    let target_slot = protected_slot
        .map(|slot| 1usize - slot)
        .or_else(|| latest_slot.map(|slot| 1usize - slot))
        .unwrap_or(0);
    let target = &paths[target_slot];
    if let Some(parent) = target.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }

    let bytes = encode_store(world, objects, store, next_generation)?;
    let temp = temp_path(target);
    let _ = fs::remove_file(&temp);
    let write_result = (|| -> Result<(), BuildingSaveError> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        match fs::remove_file(target) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(BuildingSaveError::Io(error)),
        }
        fs::rename(&temp, target)?;
        #[cfg(unix)]
        if let Some(parent) = target.parent().filter(|path| !path.as_os_str().is_empty()) {
            File::open(parent)?.sync_all()?;
        }
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    write_result?;

    let verify = materialize(read_decoded(target)?, target.clone(), world, objects)?;
    if verify.meta.generation != next_generation
        || verify.meta.building_semantic_hash != store.semantic_hash()
    {
        return Err(BuildingSaveError::Invalid(
            "post-write building verification failed".into(),
        ));
    }
    Ok(BuildingSaveReceipt {
        generation: next_generation,
        slot_path: target.clone(),
        bytes: bytes.len() as u64,
        component_count: verify.meta.component_count as usize,
        terrain_semantic_hash: verify.meta.terrain_semantic_hash,
        object_semantic_hash: verify.meta.object_semantic_hash,
        building_semantic_hash: verify.meta.building_semantic_hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coords::WorldSize;
    use crate::generation::TerrainParams;
    use crate::persistence;
    use crate::voxel::BlockId;

    fn temp_logical(name: &str) -> PathBuf {
        let unique = format!(
            "craftsman_fortress_r5_{name}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        std::env::temp_dir().join(unique).join("world.cfsv")
    }

    fn flat_world() -> World {
        let mut world = World::generate_all(WorldSize::new(48, 32, 48), TerrainParams::new(66));
        for z in 0..48 {
            for x in 0..48 {
                world
                    .set_voxel(IVec3::new(x, 20, z), BlockId::Stone)
                    .unwrap();
                for y in 21..28 {
                    world.set_voxel(IVec3::new(x, y, z), BlockId::Air).unwrap();
                }
            }
        }
        world.clear_all_dirty_for_test();
        world
    }

    fn store(world: &World, objects: &ObjectStore) -> BuildingStore {
        let mut store = BuildingStore::new();
        store
            .place(
                world,
                objects,
                BuildingKind::FloorBasic,
                IVec3::new(5, 21, 5),
                0,
            )
            .unwrap();
        store
            .place(
                world,
                objects,
                BuildingKind::WallBasic,
                IVec3::new(5, 21, 5),
                0,
            )
            .unwrap();
        store
    }

    #[test]
    fn roundtrip_and_checksum_fallback() {
        let terrain = temp_logical("roundtrip");
        let building_path = building_logical_path(&terrain);
        let world = flat_world();
        let objects = ObjectStore::new();
        let store = store(&world, &objects);
        let first = save_atomic(&building_path, &world, &objects, &store, None).unwrap();
        let second = save_atomic(&building_path, &world, &objects, &store, None).unwrap();
        let mut bytes = fs::read(&second.slot_path).unwrap();
        bytes[HEADER_LEN + 3] ^= 0x5a;
        fs::write(&second.slot_path, bytes).unwrap();
        let loaded = load_latest(&building_path, &world, &objects)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.meta.generation, first.generation);
        assert_eq!(loaded.store.semantic_hash(), store.semantic_hash());
        let _ = fs::remove_dir_all(terrain.parent().unwrap());
    }

    #[test]
    fn protects_slot_matching_durable_terrain_until_commit() {
        let terrain = temp_logical("paired-fallback");
        let building_path = building_logical_path(&terrain);
        let mut world = flat_world();
        let objects = ObjectStore::new();
        let mut buildings = store(&world, &objects);

        save_atomic(&building_path, &world, &objects, &buildings, None).unwrap();
        persistence::save_atomic(&terrain, &world).unwrap();
        let durable = persistence::load_latest(&terrain).unwrap();
        let old_loaded = load_latest(&building_path, &durable.world, &objects)
            .unwrap()
            .unwrap();
        let old_hash = old_loaded.store.semantic_hash();
        let protected_path = old_loaded.slot_path.clone();
        let protected_bytes = fs::read(&protected_path).unwrap();

        world
            .try_user_edit(IVec3::new(30, 21, 30), BlockId::Stone)
            .unwrap();
        buildings
            .place(
                &world,
                &objects,
                BuildingKind::FloorBasic,
                IVec3::new(12, 21, 12),
                0,
            )
            .unwrap();

        save_atomic(
            &building_path,
            &world,
            &objects,
            &buildings,
            Some((&durable.world, &objects)),
        )
        .unwrap();
        save_atomic(
            &building_path,
            &world,
            &objects,
            &buildings,
            Some((&durable.world, &objects)),
        )
        .unwrap();
        assert_eq!(fs::read(&protected_path).unwrap(), protected_bytes);
        let old_again = load_latest(&building_path, &durable.world, &objects)
            .unwrap()
            .unwrap();
        assert_eq!(old_again.store.semantic_hash(), old_hash);

        persistence::save_atomic(&terrain, &world).unwrap();
        let committed = persistence::load_latest(&terrain).unwrap();
        let new_loaded = load_latest(&building_path, &committed.world, &objects)
            .unwrap()
            .unwrap();
        assert_eq!(new_loaded.store.semantic_hash(), buildings.semantic_hash());
        let _ = fs::remove_dir_all(terrain.parent().unwrap());
    }

    #[test]
    fn strict_decoder_rejects_unknown_type_reserved_order_and_checksum() {
        fn refresh_checksum(bytes: &mut [u8]) {
            let data_len = bytes.len() - TRAILER_LEN;
            let sum = checksum64(&bytes[..data_len]);
            bytes[data_len..].copy_from_slice(&sum.to_le_bytes());
        }

        let world = flat_world();
        let objects = ObjectStore::new();
        let store = store(&world, &objects);
        let encoded = encode_store(&world, &objects, &store, 1).unwrap();

        let mut unknown = encoded.clone();
        unknown[HEADER_LEN + 8..HEADER_LEN + 8 + TYPE_ID_LEN].fill(0);
        unknown[HEADER_LEN + 8..HEADER_LEN + 15].copy_from_slice(b"unknown");
        refresh_checksum(&mut unknown);
        assert!(decode_bytes(&unknown).is_err());

        let mut reserved = encoded.clone();
        reserved[HEADER_LEN + RECORD_LEN - 1] = 1;
        refresh_checksum(&mut reserved);
        assert!(decode_bytes(&reserved).is_err());

        let mut out_of_order = encoded.clone();
        let first = out_of_order[HEADER_LEN..HEADER_LEN + RECORD_LEN].to_vec();
        let second = out_of_order[HEADER_LEN + RECORD_LEN..HEADER_LEN + 2 * RECORD_LEN].to_vec();
        out_of_order[HEADER_LEN..HEADER_LEN + RECORD_LEN].copy_from_slice(&second);
        out_of_order[HEADER_LEN + RECORD_LEN..HEADER_LEN + 2 * RECORD_LEN].copy_from_slice(&first);
        refresh_checksum(&mut out_of_order);
        assert!(decode_bytes(&out_of_order).is_err());

        let mut checksum = encoded;
        checksum[HEADER_LEN + 3] ^= 0x5a;
        assert!(decode_bytes(&checksum).is_err());
    }

    #[test]
    fn decoder_rejects_excessive_count_and_oversized_file() {
        fn refresh_checksum(bytes: &mut [u8]) {
            let data_len = bytes.len() - TRAILER_LEN;
            let sum = checksum64(&bytes[..data_len]);
            bytes[data_len..].copy_from_slice(&sum.to_le_bytes());
        }

        let world = flat_world();
        let objects = ObjectStore::new();
        let store = store(&world, &objects);
        let mut excessive = encode_store(&world, &objects, &store, 1).unwrap();
        excessive[44..48].copy_from_slice(&(MAX_BUILDING_COUNT + 1).to_le_bytes());
        refresh_checksum(&mut excessive);
        assert!(decode_bytes(&excessive).is_err());

        let oversized = vec![0u8; MAX_SAVE_BYTES as usize + 1];
        assert!(decode_bytes(&oversized).is_err());
    }

    #[test]
    fn materialization_rejects_object_semantic_hash_mismatch() {
        let world = flat_world();
        let objects = ObjectStore::new();
        let store = store(&world, &objects);
        let decoded = decode_bytes(&encode_store(&world, &objects, &store, 1).unwrap()).unwrap();
        let mut changed_objects = ObjectStore::new();
        changed_objects
            .place(
                &world,
                crate::objects::ObjectKind::CampfireBasic,
                IVec3::new(30, 21, 30),
                0,
            )
            .unwrap();
        assert!(materialize(
            decoded,
            PathBuf::from("mismatch.slot0"),
            &world,
            &changed_objects,
        )
        .is_err());
    }

    #[test]
    fn older_save_without_building_companion_loads_empty() {
        let terrain = temp_logical("legacy-empty");
        let world = flat_world();
        let objects = ObjectStore::new();
        assert!(
            load_latest(&building_logical_path(&terrain), &world, &objects)
                .unwrap()
                .is_none()
        );
    }
}
