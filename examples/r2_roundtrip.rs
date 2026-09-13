use bevy::math::IVec3;
use craftsman_fortress::coords::{chunk_of_voxel, WorldSize};
use craftsman_fortress::generation::{generated_block_at, TerrainParams};
use craftsman_fortress::meshing::{build_chunk_mesh, DebugTint};
use craftsman_fortress::persistence::{load_latest, save_atomic};
use craftsman_fortress::voxel::BlockId;
use craftsman_fortress::world::World;
use std::error::Error;
use std::path::{Path, PathBuf};

fn main() {
    if let Err(e) = run() {
        eprintln!("R2 acceptance failed: {e}");
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

    // Chunk (1,1,1) 的 x/z 负边界：一次修改必须同时标脏自身、-x 与 -z 邻居。
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

    // 覆盖层规范化：改动后恢复 Seed 原值，记录必须自动消失。
    let normalize = find_distinct_edit_voxel(&world, &[delete_voxel, place_voxel, boundary])?;
    let normalize_base = generated_block_at(&world.params, normalize, world.size.y);
    let before = world.modification_count();
    world
        .set_voxel(normalize, alternate(normalize_base))
        .unwrap();
    assert_eq!(world.modification_count(), before + 1);
    world.set_voxel(normalize, normalize_base).unwrap();
    assert_eq!(world.modification_count(), before);

    // R1.1 的顶层空气安全层与基岩保护继续成立。
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

    // 相同最终覆盖层以反向编辑顺序形成时，语义状态与规范化记录一致。
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

    // 第二次写入另一个槽位；破坏最新槽后必须自动回退到上一有效槽。
    let second = save_atomic(&logical, &loaded.world)?;
    assert!(second.generation > first.generation);
    assert_ne!(second.slot_path, first.slot_path, "连续保存必须轮换槽位");
    let mut latest_bytes = std::fs::read(&second.slot_path)?;
    let flip = latest_bytes.len() / 2;
    latest_bytes[flip] ^= 0x5a;
    std::fs::write(&second.slot_path, latest_bytes)?;
    let fallback = load_latest(&logical)?;
    assert_eq!(fallback.meta.generation, first.generation);
    assert_eq!(fallback.world.semantic_hash(), modified_hash);

    // 再保存会覆盖损坏的旧槽并恢复双槽可用状态。
    let repaired = save_atomic(&logical, &fallback.world)?;
    let final_loaded = load_latest(&logical)?;
    assert_eq!(final_loaded.meta.generation, repaired.generation);
    assert_eq!(final_loaded.world.semantic_hash(), modified_hash);

    let report_json = format!(
        "{{\n  \"overall\": \"PASS\",\n  \"seed\": 424242,\n  \"size\": [64,64,64],\n  \"base_hash\": \"{base_hash:#x}\",\n  \"modified_hash\": \"{modified_hash:#x}\",\n  \"edit_count\": {},\n  \"first_generation\": {},\n  \"repaired_generation\": {},\n  \"checks\": [\n    {{\"id\":\"B01\",\"status\":\"PASS\",\"name\":\"delete/place overlay\"}},\n    {{\"id\":\"B02\",\"status\":\"PASS\",\"name\":\"cross-chunk dirty propagation\"}},\n    {{\"id\":\"B03\",\"status\":\"PASS\",\"name\":\"overlay normalization\"}},\n    {{\"id\":\"B04\",\"status\":\"PASS\",\"name\":\"protected bedrock/top layer\"}},\n    {{\"id\":\"B05\",\"status\":\"PASS\",\"name\":\"atomic two-slot save\"}},\n    {{\"id\":\"B06\",\"status\":\"PASS\",\"name\":\"load exact semantic hash\"}},\n    {{\"id\":\"B07\",\"status\":\"PASS\",\"name\":\"corruption fallback\"}},\n    {{\"id\":\"B08\",\"status\":\"PASS\",\"name\":\"slot repair\"}}\n  ]\n}}\n",
        edits_before.len(),
        first.generation,
        repaired.generation
    );
    std::fs::write(out.join("report.json"), &report_json)?;
    std::fs::write(
        out.join("report.md"),
        format!(
            "# R2 roundtrip report\n\n- **PASS**\n- base hash: `{base_hash:#x}`\n- modified hash: `{modified_hash:#x}`\n- edits: {}\n- first generation: {}\n- repaired generation: {}\n- logical path: `{}`\n",
            edits_before.len(),
            first.generation,
            repaired.generation,
            logical.display()
        ),
    )?;
    println!("R2 roundtrip PASS: {}", out.display());
    Ok(())
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
