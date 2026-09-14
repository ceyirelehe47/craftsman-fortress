use bevy::math::IVec3;
use craftsman_fortress::coords::WorldSize;
use craftsman_fortress::generation::TerrainParams;
use craftsman_fortress::object_persistence;
use craftsman_fortress::objects::{ObjectId, ObjectKind, ObjectStore, PlacementError};
use craftsman_fortress::persistence;
use craftsman_fortress::voxel::BlockId;
use craftsman_fortress::world::World;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    if let Err(error) = run() {
        eprintln!("R4 object roundtrip FAIL: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("evidence_r4_headless"));
    if out.exists() && out.read_dir()?.next().is_some() {
        return Err(format!("output directory must be empty: {}", out.display()).into());
    }
    fs::create_dir_all(&out)?;

    let mut checks = Vec::new();
    let world = flat_world();
    let mut store = ObjectStore::new();

    let campfire = store.place(&world, ObjectKind::CampfireBasic, IVec3::new(4, 21, 4), 0)?;
    let tent = store.place(&world, ObjectKind::TentBasic, IVec3::new(10, 21, 9), 0)?;
    let stone = store.place(&world, ObjectKind::StonePileSmall, IVec3::new(20, 21, 8), 0)?;
    check(
        &mut checks,
        "E01",
        campfire == ObjectId(1) && tent == ObjectId(2) && stone == ObjectId(3),
        "stable monotonic ids",
    )?;

    check(
        &mut checks,
        "E02",
        matches!(
            store.place(&world, ObjectKind::CampfireBasic, IVec3::new(3, 24, 3), 0),
            Err(PlacementError::MissingSupport(_))
        ),
        "support and clearance rules",
    )?;
    check(
        &mut checks,
        "E02",
        matches!(
            store.place(&world, ObjectKind::CampfireBasic, IVec3::new(10, 21, 9), 0),
            Err(PlacementError::ObjectOverlap { .. })
        ),
        "overlap rejection",
    )?;
    store.rotate_object(&world, tent, 1)?;
    check(
        &mut checks,
        "E03",
        store.get(tent).unwrap().rotated_footprint().x == 3
            && store.get(tent).unwrap().rotated_footprint().y == 4,
        "quarter-turn footprint",
    )?;
    check(
        &mut checks,
        "E04",
        store
            .validate_terrain_edit(IVec3::new(4, 20, 4), BlockId::Air)
            .is_err()
            && store
                .validate_terrain_edit(IVec3::new(4, 21, 4), BlockId::Stone)
                .is_err(),
        "terrain cannot remove support or intersect volume",
    )?;

    store.move_object(&world, stone, IVec3::new(20, 21, 16), 2)?;
    store.remove(campfire).ok_or("campfire removal failed")?;
    check(
        &mut checks,
        "E05",
        store.get(campfire).is_none() && store.get(stone).unwrap().anchor == IVec3::new(20, 21, 16),
        "move and delete preserve stable ids",
    )?;

    let reversed: Vec<_> = store.records_sorted().into_iter().rev().collect();
    let rebuilt = ObjectStore::from_persisted(&world, store.next_id(), &reversed)?;
    check(
        &mut checks,
        "E06",
        rebuilt.semantic_hash() == store.semantic_hash(),
        "semantic hash independent of insertion order",
    )?;

    let terrain_path = out.join("roundtrip.cfsv");
    let object_path = object_persistence::object_logical_path(&terrain_path);
    let first_object_receipt = object_persistence::save_atomic(&object_path, &world, &store, None)?;
    let object_bytes = fs::read(&first_object_receipt.slot_path)?;
    check(
        &mut checks,
        "E07",
        object_bytes.starts_with(b"CFOBJ001") && !object_bytes.starts_with(b"CFSAVE02"),
        "strict object companion format is distinct from terrain format",
    )?;
    persistence::save_atomic(&terrain_path, &world)?;
    let loaded_world = persistence::load_latest(&terrain_path)?;
    let loaded_objects = object_persistence::load_latest(&object_path, &loaded_world.world)?
        .ok_or("object companion missing after save")?;
    check(
        &mut checks,
        "E08",
        loaded_objects.store.semantic_hash() == store.semantic_hash(),
        "exact terrain + object roundtrip",
    )?;

    let first = object_persistence::save_atomic(&object_path, &world, &store, Some(&world))?;
    let second = object_persistence::save_atomic(&object_path, &world, &store, Some(&world))?;
    let latest = if second.generation > first.generation {
        second
    } else {
        first
    };
    let mut bytes = fs::read(&latest.slot_path)?;
    let flip = bytes.len() / 2;
    bytes[flip] ^= 0x5a;
    fs::write(&latest.slot_path, bytes)?;
    let fallback = object_persistence::load_latest(&object_path, &world)?
        .ok_or("fallback object slot missing")?;
    check(
        &mut checks,
        "E09",
        fallback.store.semantic_hash() == store.semantic_hash(),
        "corrupted newest object slot falls back",
    )?;

    paired_crash_fallback(&out, &mut checks)?;

    let legacy = out.join("legacy.cfsv");
    check(
        &mut checks,
        "E11",
        object_persistence::load_latest(&object_persistence::object_logical_path(&legacy), &world)?
            .is_none(),
        "terrain saves without object companion load as empty",
    )?;

    check(
        &mut checks,
        "E12",
        ObjectKind::ALL
            .iter()
            .all(|kind| !kind.type_id().is_empty() && !kind.asset_id().is_empty()),
        "every object type has stable logical and visual ids",
    )?;

    let checks_json = checks
        .iter()
        .map(|(id, detail)| {
            format!(
                "{{\"id\":\"{id}\",\"status\":\"PASS\",\"detail\":\"{}\"}}",
                detail.replace('"', "\\\"")
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    fs::write(
        out.join("report.json"),
        format!(
            "{{\n  \"overall\":\"PASS\",\n  \"object_hash\":\"{:#018x}\",\n  \"object_count\":{},\n  \"checks\":[{}]\n}}\n",
            store.semantic_hash(),
            store.len(),
            checks_json
        ),
    )?;
    fs::write(
        out.join("report.md"),
        format!(
            "# R4 object-layer headless acceptance\n\n- overall: **PASS**\n- object count: {}\n- object hash: `{:#018x}`\n- E01-E12: **PASS**\n",
            store.len(),
            store.semantic_hash()
        ),
    )?;
    println!("R4 object roundtrip PASS: {}", out.display());
    Ok(())
}

fn paired_crash_fallback(
    out: &Path,
    checks: &mut Vec<(&'static str, String)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let terrain_path = out.join("paired.cfsv");
    let object_path = object_persistence::object_logical_path(&terrain_path);
    let mut old_world = flat_world();
    let mut old_store = ObjectStore::new();
    let id = old_store.place(
        &old_world,
        ObjectKind::CampfireBasic,
        IVec3::new(6, 21, 6),
        0,
    )?;
    object_persistence::save_atomic(&object_path, &old_world, &old_store, None)?;
    persistence::save_atomic(&terrain_path, &old_world)?;
    let durable = persistence::load_latest(&terrain_path)?;
    let old_hash = old_store.semantic_hash();

    old_world.try_user_edit(IVec3::new(20, 21, 20), BlockId::Stone)?;
    old_store.move_object(&old_world, id, IVec3::new(8, 21, 6), 1)?;
    object_persistence::save_atomic(&object_path, &old_world, &old_store, Some(&durable.world))?;
    object_persistence::save_atomic(&object_path, &old_world, &old_store, Some(&durable.world))?;
    let before_terrain_commit = object_persistence::load_latest(&object_path, &durable.world)?
        .ok_or("old object fallback missing")?;
    check(
        checks,
        "E10",
        before_terrain_commit.store.semantic_hash() == old_hash,
        "repeated object prewrite preserves slot matching durable terrain",
    )?;

    persistence::save_atomic(&terrain_path, &old_world)?;
    let committed = persistence::load_latest(&terrain_path)?;
    let after_commit = object_persistence::load_latest(&object_path, &committed.world)?
        .ok_or("new paired object slot missing")?;
    if after_commit.store.semantic_hash() != old_store.semantic_hash() {
        return Err("new object state did not become active after terrain commit".into());
    }
    Ok(())
}

fn flat_world() -> World {
    let mut world = World::generate_all(WorldSize::new(32, 32, 32), TerrainParams::new(4242));
    for z in 0..32 {
        for x in 0..32 {
            world
                .set_voxel(IVec3::new(x, 20, z), BlockId::Stone)
                .unwrap();
            for y in 21..26 {
                world.set_voxel(IVec3::new(x, y, z), BlockId::Air).unwrap();
            }
        }
    }
    world.clear_all_dirty_for_test();
    world
}

fn check(
    checks: &mut Vec<(&'static str, String)>,
    id: &'static str,
    pass: bool,
    detail: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if !pass {
        return Err(format!("{id} failed: {detail}").into());
    }
    checks.push((id, detail.to_string()));
    Ok(())
}
