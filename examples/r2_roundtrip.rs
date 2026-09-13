use bevy::math::IVec3;
use craftsman_fortress::coords::{chunk_of_voxel, WorldSize};
use craftsman_fortress::generation::{generated_block_at, TerrainParams};
use craftsman_fortress::meshing::{build_chunk_mesh, DebugTint};
use craftsman_fortress::persistence::{load_latest, save_atomic};
use craftsman_fortress::voxel::BlockId;
use craftsman_fortress::world::World;
use std::error::Error;
use std::path::{Path, PathBuf};

const BASE_HASH_OFFSET: usize = 48;
const WORLD_HASH_OFFSET: usize = 56;
const TRAILER_LEN: usize = 8;

fn main() {
    if let Err(e) = run() {
        eprintln!("R2.1 acceptance failed: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let out = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("evidence_r2_roundtrip"));
    prepare_empty_dir(&out)?;
    let logical = out.join("roundtrip.cfsv");

    let size = WorldSize::new(64, 64, 64);
    let mut world = World::generate_all(size, TerrainParams::new(424_242));
    let base_hash = world.semantic_hash();
    assert_eq!(base_hash, world.generated_semantic_hash());

    let delete_voxel = find_voxel(&world, true)?;
    let place_voxel = find_voxel(&world, false)?;
    assert!(world.try_user_edit(delete_voxel, BlockId::Air)?);
    assert!(world.try_user_edit(place_voxel, BlockId::Stone)?);

    let boundary = IVec3::new(16, 20, 16);
    let boundary_base = world.voxel(boundary);
    let boundary_new = alternate(boundary_base);
    world.clear_all_dirty();
    assert!(world.try_user_edit(boundary, boundary_new)?);
    let dirty = world.dirty_chunks();
    let own = chunk_of_voxel(boundary);
    assert!(dirty.contains(&own));
    assert!(dirty.contains(&(own + IVec3::new(-1, 0, 0))));
    assert!(dirty.contains(&(own + IVec3::new(0, 0, -1))));
    world.clear_all_dirty();

    let normalize = find_distinct_edit_voxel(&world, &[delete_voxel, place_voxel, boundary])?;
    let normalize_base = generated_block_at(&world.params, normalize, world.size.y);
    let before = world.modification_count();
    world
        .set_voxel(normalize, alternate(normalize_base))
        .unwrap();
    assert_eq!(world.modification_count(), before + 1);
    world.set_voxel(normalize, normalize_base).unwrap();
    assert_eq!(world.modification_count(), before);

    assert!(world
        .try_user_edit(IVec3::new(10, 0, 10), BlockId::Air)
        .is_err());
    assert!(world
        .try_user_edit(IVec3::new(10, 63, 10), BlockId::Stone)
        .is_err());

    let modified_hash = world.semantic_hash();
    assert_ne!(modified_hash, base_hash);
    let edits_before = world.modifications_sorted();
    let affected_chunks = [own, own + IVec3::new(-1, 0, 0), own + IVec3::new(0, 0, -1)];
    let meshes_before: Vec<_> = affected_chunks
        .iter()
        .map(|&cc| build_chunk_mesh(&world, cc, DebugTint::Off))
        .collect();

    let first = save_atomic(&logical, &world)?;
    let loaded = load_latest(&logical)?;
    assert_eq!(loaded.meta.generation, first.generation);
    assert_eq!(loaded.world.semantic_hash(), modified_hash);
    assert_eq!(loaded.world.modifications_sorted(), edits_before);
    assert_eq!(loaded.world.params.seed, 424_242);
    assert_eq!(loaded.world.size, size);
    for (index, &cc) in affected_chunks.iter().enumerate() {
        let after = build_chunk_mesh(&loaded.world, cc, DebugTint::Off);
        assert_eq!(after.positions, meshes_before[index].positions);
        assert_eq!(after.normals, meshes_before[index].normals);
        assert_eq!(after.colors, meshes_before[index].colors);
        assert_eq!(after.indices, meshes_before[index].indices);
    }

    let mut reordered = World::generate_all(size, TerrainParams::new(424_242));
    for &(voxel, block) in edits_before.iter().rev() {
        assert!(reordered.try_user_edit(voxel, block)?);
    }
    assert_eq!(reordered.semantic_hash(), modified_hash);
    assert_eq!(reordered.modifications_sorted(), edits_before);
    let order_logical = out.join("order.cfsv");
    save_atomic(&order_logical, &reordered)?;
    assert_eq!(
        load_latest(&order_logical)?.world.semantic_hash(),
        modified_hash
    );

    let second = save_atomic(&logical, &loaded.world)?;
    assert!(second.generation > first.generation);
    assert_ne!(second.slot_path, first.slot_path, "连续保存必须轮换槽位");
    let first_slot_before = std::fs::read(&first.slot_path)?;

    // FNV 正确但最终语义哈希错误：必须回退第一槽。
    let mut semantic_bytes = std::fs::read(&second.slot_path)?;
    flip_u64(&mut semantic_bytes, WORLD_HASH_OFFSET);
    refresh_checksum(&mut semantic_bytes);
    std::fs::write(&second.slot_path, semantic_bytes)?;
    let semantic_fallback = load_latest(&logical)?;
    assert_eq!(semantic_fallback.meta.generation, first.generation);
    assert_eq!(semantic_fallback.world.semantic_hash(), modified_hash);

    // 保存时必须把语义无效槽当目标，唯一完整有效旧槽不得改变。
    let semantic_repair = save_atomic(&logical, &semantic_fallback.world)?;
    assert_eq!(std::fs::read(&first.slot_path)?, first_slot_before);
    assert_eq!(
        load_latest(&logical)?.meta.generation,
        semantic_repair.generation
    );

    // 普通 FNV 损坏仍须回退。
    let mut checksum_bytes = std::fs::read(&semantic_repair.slot_path)?;
    let flip = checksum_bytes.len() / 2;
    checksum_bytes[flip] ^= 0x5a;
    std::fs::write(&semantic_repair.slot_path, checksum_bytes)?;
    let checksum_fallback = load_latest(&logical)?;
    assert_eq!(checksum_fallback.meta.generation, first.generation);
    assert_eq!(checksum_fallback.world.semantic_hash(), modified_hash);

    let repaired = save_atomic(&logical, &checksum_fallback.world)?;
    let final_loaded = load_latest(&logical)?;
    assert_eq!(final_loaded.meta.generation, repaired.generation);
    assert_eq!(final_loaded.world.semantic_hash(), modified_hash);

    // 基础语义哈希错误但 FNV 正确：同样必须回退旧槽。
    let base_logical = out.join("base_hash_fallback.cfsv");
    let base_first = save_atomic(&base_logical, &world)?;
    let base_second = save_atomic(&base_logical, &world)?;
    let mut base_bytes = std::fs::read(&base_second.slot_path)?;
    flip_u64(&mut base_bytes, BASE_HASH_OFFSET);
    refresh_checksum(&mut base_bytes);
    std::fs::write(&base_second.slot_path, base_bytes)?;
    assert_eq!(
        load_latest(&base_logical)?.meta.generation,
        base_first.generation
    );

    // 给进程级 B11 使用：主程序应在创建 Bevy App 前以退出码 3 拒绝。
    std::fs::write(out.join("invalid_startup.cfsv.slot0"), b"not-a-valid-save")?;

    let report_json = format!(
        "{{\n  \"overall\": \"PASS\",\n  \"seed\": 424242,\n  \"size\": [64,64,64],\n  \"base_hash\": \"{base_hash:#x}\",\n  \"modified_hash\": \"{modified_hash:#x}\",\n  \"edit_count\": {},\n  \"first_generation\": {},\n  \"repaired_generation\": {},\n  \"semantic_hash_fallback\": true,\n  \"base_hash_fallback\": true,\n  \"checksum_fallback\": true,\n  \"old_valid_slot_preserved\": true\n}}\n",
        edits_before.len(),
        first.generation,
        repaired.generation
    );
    std::fs::write(out.join("roundtrip_report.json"), &report_json)?;
    std::fs::write(
        out.join("roundtrip_report.md"),
        format!(
            "# R2.1 roundtrip report\n\n- **PASS**\n- base hash: `{base_hash:#x}`\n- modified hash: `{modified_hash:#x}`\n- edits: {}\n- first generation: {}\n- repaired generation: {}\n- semantic/base/FNV fallback: PASS\n- unique old valid slot preserved: PASS\n",
            edits_before.len(),
            first.generation,
            repaired.generation
        ),
    )?;
    println!("R2.1 roundtrip PASS: {}", out.display());
    Ok(())
}

fn flip_u64(bytes: &mut [u8], offset: usize) {
    let mut value = u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
    value ^= 0x9e37_79b9_7f4a_7c15;
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn refresh_checksum(bytes: &mut [u8]) {
    let data_len = bytes.len() - TRAILER_LEN;
    let checksum = fnv1a64(&bytes[..data_len]);
    bytes[data_len..].copy_from_slice(&checksum.to_le_bytes());
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &byte in bytes {
        h ^= byte as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn alternate(block: BlockId) -> BlockId {
    if block.is_solid() && block != BlockId::Bedrock {
        BlockId::Air
    } else {
        BlockId::Stone
    }
}

fn find_voxel(world: &World, solid: bool) -> Result<IVec3, Box<dyn Error>> {
    for y in 6..world.size.y as i32 - 1 {
        for z in 4..world.size.z as i32 - 4 {
            for x in 4..world.size.x as i32 - 4 {
                let v = IVec3::new(x, y, z);
                let block = world.voxel(v);
                if block.is_solid() == solid && block != BlockId::Bedrock {
                    return Ok(v);
                }
            }
        }
    }
    Err(format!("no voxel found: solid={solid}").into())
}

fn find_distinct_edit_voxel(world: &World, excluded: &[IVec3]) -> Result<IVec3, Box<dyn Error>> {
    for y in 6..world.size.y as i32 - 1 {
        for z in 4..world.size.z as i32 - 4 {
            for x in 4..world.size.x as i32 - 4 {
                let v = IVec3::new(x, y, z);
                if excluded.contains(&v) || world.voxel(v) == BlockId::Bedrock {
                    continue;
                }
                return Ok(v);
            }
        }
    }
    Err("no distinct normalization voxel found".into())
}

fn prepare_empty_dir(path: &Path) -> Result<(), Box<dyn Error>> {
    if path.exists() && path.read_dir()?.next().is_some() {
        return Err(format!("evidence directory is not empty: {}", path.display()).into());
    }
    std::fs::create_dir_all(path)?;
    Ok(())
}
