//! R2 存档：确定性 Seed 世界 + 修改覆盖层。
//!
//! 存档逻辑路径对应两个轮换槽位 `<path>.slot0` / `<path>.slot1`。新存档总是写入
//! 非当前最新槽位：先写同目录临时文件并 `sync_all`，再重命名到已腾空的旧槽位。
//! 任一时刻至少保留一个已校验槽位，因此进程崩溃或断电不会同时破坏新旧状态。

use crate::coords::{WorldSize, CHUNK_SIZE};
use crate::generation::{generated_block_at, TerrainParams};
use crate::voxel::BlockId;
use crate::world::World;
use bevy::math::IVec3;
use std::collections::HashSet;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MAGIC: [u8; 8] = *b"CFSAVE02";
pub const SAVE_FORMAT_VERSION: u32 = 1;
pub const WORLD_GENERATOR_REVISION: u32 = 1;
const HEADER_LEN: usize = 64;
const RECORD_LEN: usize = 16;
const TRAILER_LEN: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveMeta {
    pub format_version: u32,
    pub generator_revision: u32,
    pub generation: u64,
    pub seed: u64,
    pub size: WorldSize,
    pub edit_count: u32,
    pub base_semantic_hash: u64,
    pub world_semantic_hash: u64,
}

pub struct LoadedWorld {
    pub world: World,
    pub meta: SaveMeta,
    pub slot_path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct SaveReceipt {
    pub generation: u64,
    pub slot_path: PathBuf,
    pub bytes: u64,
    pub edit_count: usize,
    pub world_semantic_hash: u64,
}

#[derive(Debug)]
pub enum SaveError {
    Io(std::io::Error),
    Invalid(String),
    NoValidSlot {
        logical: PathBuf,
        details: Vec<String>,
    },
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveError::Io(e) => write!(f, "I/O error: {e}"),
            SaveError::Invalid(e) => write!(f, "invalid save: {e}"),
            SaveError::NoValidSlot { logical, details } => {
                write!(f, "no valid save slot for {}", logical.display())?;
                if !details.is_empty() {
                    write!(f, ": {}", details.join(" | "))?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for SaveError {}

impl From<std::io::Error> for SaveError {
    fn from(value: std::io::Error) -> Self {
        SaveError::Io(value)
    }
}

#[derive(Clone, Debug)]
struct DecodedSave {
    meta: SaveMeta,
    edits: Vec<(IVec3, BlockId)>,
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take<const N: usize>(&mut self) -> Result<[u8; N], SaveError> {
        let end = self
            .pos
            .checked_add(N)
            .ok_or_else(|| SaveError::Invalid("offset overflow".into()))?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or_else(|| SaveError::Invalid("unexpected end of file".into()))?;
        self.pos = end;
        let mut out = [0u8; N];
        out.copy_from_slice(slice);
        Ok(out)
    }

    fn u32(&mut self) -> Result<u32, SaveError> {
        Ok(u32::from_le_bytes(self.take()?))
    }

    fn i32(&mut self) -> Result<i32, SaveError> {
        Ok(i32::from_le_bytes(self.take()?))
    }

    fn u64(&mut self) -> Result<u64, SaveError> {
        Ok(u64::from_le_bytes(self.take()?))
    }
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
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    append_suffix(slot, &format!(".tmp-{}-{stamp}", std::process::id()))
}

fn checksum64(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
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

fn validate_size(size: WorldSize) -> Result<(), SaveError> {
    let c = CHUNK_SIZE as u32;
    if size.x == 0 || size.y == 0 || size.z == 0 {
        return Err(SaveError::Invalid("zero world dimension".into()));
    }
    if !size.x.is_multiple_of(c) || !size.y.is_multiple_of(c) || !size.z.is_multiple_of(c) {
        return Err(SaveError::Invalid(format!(
            "world size {}x{}x{} is not chunk aligned",
            size.x, size.y, size.z
        )));
    }
    if size.x > 512 || size.y > 256 || size.z > 512 {
        return Err(SaveError::Invalid("world size exceeds R2 limit".into()));
    }
    Ok(())
}

fn validate_edit(
    size: WorldSize,
    params: &TerrainParams,
    voxel: IVec3,
    block: BlockId,
) -> Result<(), SaveError> {
    if !size.contains(voxel) {
        return Err(SaveError::Invalid(format!("edit out of bounds: {voxel}")));
    }
    if voxel.y <= params.bedrock_layers {
        return Err(SaveError::Invalid(format!(
            "edit touches protected bedrock: {voxel}"
        )));
    }
    if block == BlockId::Bedrock {
        return Err(SaveError::Invalid(format!(
            "persisted Bedrock placement: {voxel}"
        )));
    }
    if voxel.y as u32 == size.y - 1 && block.is_solid() {
        return Err(SaveError::Invalid(format!(
            "top safety layer must stay air: {voxel}"
        )));
    }
    let generated = generated_block_at(params, voxel, size.y);
    if generated == block {
        return Err(SaveError::Invalid(format!(
            "non-canonical edit equals generated block at {voxel}"
        )));
    }
    Ok(())
}

fn encode_world(world: &World, generation: u64) -> Result<Vec<u8>, SaveError> {
    let edits = world.modifications_sorted();
    let edit_count =
        u32::try_from(edits.len()).map_err(|_| SaveError::Invalid("too many edits".into()))?;
    for &(voxel, block) in &edits {
        validate_edit(world.size, &world.params, voxel, block)?;
    }

    let base_hash = world.generated_semantic_hash();
    let world_hash = world.semantic_hash();
    let capacity = HEADER_LEN
        .checked_add(edits.len().saturating_mul(RECORD_LEN))
        .and_then(|v| v.checked_add(TRAILER_LEN))
        .ok_or_else(|| SaveError::Invalid("save size overflow".into()))?;
    let mut out = Vec::with_capacity(capacity);
    out.extend_from_slice(&MAGIC);
    push_u32(&mut out, SAVE_FORMAT_VERSION);
    push_u32(&mut out, WORLD_GENERATOR_REVISION);
    push_u64(&mut out, generation);
    push_u64(&mut out, world.params.seed);
    push_u32(&mut out, world.size.x);
    push_u32(&mut out, world.size.y);
    push_u32(&mut out, world.size.z);
    push_u32(&mut out, edit_count);
    push_u64(&mut out, base_hash);
    push_u64(&mut out, world_hash);
    debug_assert_eq!(out.len(), HEADER_LEN);

    for (voxel, block) in edits {
        push_i32(&mut out, voxel.x);
        push_i32(&mut out, voxel.y);
        push_i32(&mut out, voxel.z);
        out.push(block as u8);
        out.extend_from_slice(&[0, 0, 0]);
    }
    let checksum = checksum64(&out);
    push_u64(&mut out, checksum);
    Ok(out)
}

fn decode_bytes(bytes: &[u8]) -> Result<DecodedSave, SaveError> {
    if bytes.len() < HEADER_LEN + TRAILER_LEN {
        return Err(SaveError::Invalid("file is truncated".into()));
    }
    let data_len = bytes.len() - TRAILER_LEN;
    let expected_checksum = u64::from_le_bytes(
        bytes[data_len..]
            .try_into()
            .map_err(|_| SaveError::Invalid("checksum trailer missing".into()))?,
    );
    let actual_checksum = checksum64(&bytes[..data_len]);
    if actual_checksum != expected_checksum {
        return Err(SaveError::Invalid(format!(
            "checksum mismatch: expected {expected_checksum:#x}, got {actual_checksum:#x}"
        )));
    }

    let mut r = Reader::new(&bytes[..data_len]);
    if r.take::<8>()? != MAGIC {
        return Err(SaveError::Invalid("bad magic".into()));
    }
    let format_version = r.u32()?;
    if format_version != SAVE_FORMAT_VERSION {
        return Err(SaveError::Invalid(format!(
            "unsupported format version {format_version}"
        )));
    }
    let generator_revision = r.u32()?;
    if generator_revision != WORLD_GENERATOR_REVISION {
        return Err(SaveError::Invalid(format!(
            "unsupported generator revision {generator_revision}"
        )));
    }
    let generation = r.u64()?;
    let seed = r.u64()?;
    let size = WorldSize::new(r.u32()?, r.u32()?, r.u32()?);
    validate_size(size)?;
    let edit_count = r.u32()?;
    let base_semantic_hash = r.u64()?;
    let world_semantic_hash = r.u64()?;

    let expected_len = HEADER_LEN
        .checked_add((edit_count as usize).saturating_mul(RECORD_LEN))
        .ok_or_else(|| SaveError::Invalid("record length overflow".into()))?;
    if expected_len != data_len {
        return Err(SaveError::Invalid(format!(
            "length mismatch: header says {edit_count} edits, bytes={}",
            bytes.len()
        )));
    }

    let params = TerrainParams::new(seed);
    let mut edits = Vec::with_capacity(edit_count as usize);
    let mut seen = HashSet::with_capacity(edit_count as usize);
    for _ in 0..edit_count {
        let voxel = IVec3::new(r.i32()?, r.i32()?, r.i32()?);
        let block_byte = r.take::<1>()?[0];
        let reserved = r.take::<3>()?;
        if reserved != [0, 0, 0] {
            return Err(SaveError::Invalid("non-zero reserved bytes".into()));
        }
        let block = BlockId::try_from_u8(block_byte)
            .ok_or_else(|| SaveError::Invalid(format!("unknown block id {block_byte}")))?;
        validate_edit(size, &params, voxel, block)?;
        if !seen.insert((voxel.x, voxel.y, voxel.z)) {
            return Err(SaveError::Invalid(format!("duplicate edit at {voxel}")));
        }
        edits.push((voxel, block));
    }

    Ok(DecodedSave {
        meta: SaveMeta {
            format_version,
            generator_revision,
            generation,
            seed,
            size,
            edit_count,
            base_semantic_hash,
            world_semantic_hash,
        },
        edits,
    })
}

fn read_decoded(path: &Path) -> Result<DecodedSave, SaveError> {
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    decode_bytes(&bytes)
}

fn materialize(decoded: DecodedSave, slot_path: PathBuf) -> Result<LoadedWorld, SaveError> {
    let mut world = World::generate_all(decoded.meta.size, TerrainParams::new(decoded.meta.seed));
    let actual_base = world.semantic_hash();
    if actual_base != decoded.meta.base_semantic_hash {
        return Err(SaveError::Invalid(format!(
            "base hash mismatch: save={:#x}, generated={actual_base:#x}",
            decoded.meta.base_semantic_hash
        )));
    }
    world
        .apply_persisted_edits(&decoded.edits)
        .map_err(SaveError::Invalid)?;
    let actual_world = world.semantic_hash();
    if actual_world != decoded.meta.world_semantic_hash {
        return Err(SaveError::Invalid(format!(
            "world hash mismatch: save={:#x}, loaded={actual_world:#x}",
            decoded.meta.world_semantic_hash
        )));
    }
    Ok(LoadedWorld {
        world,
        meta: decoded.meta,
        slot_path,
    })
}

/// 读取两个槽位，忽略损坏槽并选择 generation 最大的有效存档。
pub fn load_latest(logical: &Path) -> Result<LoadedWorld, SaveError> {
    let mut valid: Vec<(DecodedSave, PathBuf)> = Vec::new();
    let mut details = Vec::new();
    for path in slot_paths(logical) {
        if !path.exists() {
            details.push(format!("{}: missing", path.display()));
            continue;
        }
        match read_decoded(&path) {
            Ok(decoded) => valid.push((decoded, path)),
            Err(e) => details.push(format!("{}: {e}", path.display())),
        }
    }
    let (decoded, path) = valid
        .into_iter()
        .max_by_key(|(decoded, _)| decoded.meta.generation)
        .ok_or_else(|| SaveError::NoValidSlot {
            logical: logical.to_path_buf(),
            details,
        })?;
    materialize(decoded, path)
}

/// 双槽原子保存。不会原地覆盖当前最新有效槽位。
pub fn save_atomic(logical: &Path, world: &World) -> Result<SaveReceipt, SaveError> {
    let paths = slot_paths(logical);
    let mut valid_meta = Vec::new();
    for (slot, path) in paths.iter().enumerate() {
        if let Ok(decoded) = read_decoded(path) {
            valid_meta.push((decoded.meta.generation, slot));
        }
    }
    let latest = valid_meta
        .iter()
        .max_by_key(|(generation, _)| *generation)
        .copied();
    let next_generation = latest.map(|(g, _)| g.saturating_add(1)).unwrap_or(1);
    let target_slot = latest.map(|(_, slot)| 1usize - slot).unwrap_or(0);
    let target = &paths[target_slot];
    if let Some(parent) = target.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }

    let bytes = encode_world(world, next_generation)?;
    let temp = temp_path(target);
    let _ = fs::remove_file(&temp);
    let write_result = (|| -> Result<(), SaveError> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        match fs::remove_file(target) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(SaveError::Io(e)),
        }
        fs::rename(&temp, target)?;
        #[cfg(unix)]
        if let Some(parent) = target.parent().filter(|p| !p.as_os_str().is_empty()) {
            File::open(parent)?.sync_all()?;
        }
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    write_result?;

    let verify = read_decoded(target)?;
    if verify.meta.generation != next_generation
        || verify.meta.world_semantic_hash != world.semantic_hash()
    {
        return Err(SaveError::Invalid("post-write verification failed".into()));
    }
    Ok(SaveReceipt {
        generation: next_generation,
        slot_path: target.clone(),
        bytes: bytes.len() as u64,
        edit_count: verify.meta.edit_count as usize,
        world_semantic_hash: verify.meta.world_semantic_hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generation::generated_block_at;

    fn temp_logical(name: &str) -> PathBuf {
        let unique = format!(
            "craftsman_fortress_{name}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        std::env::temp_dir().join(unique).join("world.cfsv")
    }

    fn edited_world() -> World {
        let size = WorldSize::new(32, 32, 32);
        let mut world = World::generate_all(size, TerrainParams::new(77));
        for voxel in [IVec3::new(8, 12, 8), IVec3::new(16, 12, 16)] {
            let base = generated_block_at(&world.params, voxel, world.size.y);
            let block = if base.is_solid() {
                BlockId::Air
            } else {
                BlockId::Stone
            };
            world.try_user_edit(voxel, block).unwrap();
        }
        world
    }

    #[test]
    fn roundtrip_restores_semantic_world() {
        let logical = temp_logical("roundtrip");
        let world = edited_world();
        let receipt = save_atomic(&logical, &world).unwrap();
        assert_eq!(receipt.generation, 1);
        let loaded = load_latest(&logical).unwrap();
        assert_eq!(loaded.world.semantic_hash(), world.semantic_hash());
        assert_eq!(
            loaded.world.modifications_sorted(),
            world.modifications_sorted()
        );
        let _ = fs::remove_dir_all(logical.parent().unwrap());
    }

    #[test]
    fn corrupted_latest_slot_falls_back() {
        let logical = temp_logical("fallback");
        let world = edited_world();
        let first = save_atomic(&logical, &world).unwrap();
        let second = save_atomic(&logical, &world).unwrap();
        assert!(second.generation > first.generation);
        let mut bytes = fs::read(&second.slot_path).unwrap();
        bytes[HEADER_LEN + 1] ^= 0x55;
        fs::write(&second.slot_path, bytes).unwrap();
        let loaded = load_latest(&logical).unwrap();
        assert_eq!(loaded.meta.generation, first.generation);
        assert_eq!(loaded.world.semantic_hash(), world.semantic_hash());
        let _ = fs::remove_dir_all(logical.parent().unwrap());
    }

    fn refresh_checksum(bytes: &mut [u8]) {
        let data_len = bytes.len() - TRAILER_LEN;
        let checksum = checksum64(&bytes[..data_len]);
        bytes[data_len..].copy_from_slice(&checksum.to_le_bytes());
    }

    #[test]
    fn decoder_rejects_truncation_version_checksum_and_unknown_block() {
        let world = edited_world();
        let bytes = encode_world(&world, 1).unwrap();
        assert!(decode_bytes(&bytes[..bytes.len() - 3]).is_err());

        let mut bad_version = bytes.clone();
        bad_version[8..12].copy_from_slice(&99u32.to_le_bytes());
        refresh_checksum(&mut bad_version);
        assert!(decode_bytes(&bad_version).is_err());

        let mut bad_revision = bytes.clone();
        bad_revision[12..16].copy_from_slice(&99u32.to_le_bytes());
        refresh_checksum(&mut bad_revision);
        assert!(decode_bytes(&bad_revision).is_err());

        let mut bad_checksum = bytes.clone();
        bad_checksum[HEADER_LEN + 1] ^= 0x11;
        assert!(decode_bytes(&bad_checksum).is_err());

        let mut unknown_block = bytes.clone();
        unknown_block[HEADER_LEN + 12] = 255;
        refresh_checksum(&mut unknown_block);
        assert!(decode_bytes(&unknown_block).is_err());
    }

    #[test]
    fn decoder_rejects_duplicate_and_forbidden_records() {
        let world = edited_world();
        let bytes = encode_world(&world, 1).unwrap();

        let mut duplicate = bytes.clone();
        let first = duplicate[HEADER_LEN..HEADER_LEN + 12].to_vec();
        duplicate[HEADER_LEN + RECORD_LEN..HEADER_LEN + RECORD_LEN + 12].copy_from_slice(&first);
        refresh_checksum(&mut duplicate);
        assert!(decode_bytes(&duplicate).is_err());

        let mut reserved = bytes.clone();
        reserved[HEADER_LEN + 13] = 1;
        refresh_checksum(&mut reserved);
        assert!(decode_bytes(&reserved).is_err());

        let mut top_layer = bytes.clone();
        top_layer[HEADER_LEN + 4..HEADER_LEN + 8].copy_from_slice(&31i32.to_le_bytes());
        top_layer[HEADER_LEN + 12] = BlockId::Stone as u8;
        refresh_checksum(&mut top_layer);
        assert!(decode_bytes(&top_layer).is_err());

        let mut bedrock = bytes;
        bedrock[HEADER_LEN + 4..HEADER_LEN + 8].copy_from_slice(&0i32.to_le_bytes());
        refresh_checksum(&mut bedrock);
        assert!(decode_bytes(&bedrock).is_err());
    }

    #[test]
    fn missing_slots_are_an_explicit_error() {
        let logical = temp_logical("missing");
        assert!(matches!(
            load_latest(&logical),
            Err(SaveError::NoValidSlot { .. })
        ));
    }
}
