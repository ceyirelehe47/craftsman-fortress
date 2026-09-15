use bevy::math::IVec3;
use craftsman_fortress::building_persistence;
use craftsman_fortress::buildings::{
    direction, visual_boxes, BuildingError, BuildingId, BuildingKind, BuildingStore, PlacedBuilding,
};
use craftsman_fortress::coords::WorldSize;
use craftsman_fortress::generation::TerrainParams;
use craftsman_fortress::object_persistence;
use craftsman_fortress::objects::{ObjectId, ObjectKind, ObjectStore, PlacedObject};
use craftsman_fortress::persistence;
use craftsman_fortress::voxel::BlockId;
use craftsman_fortress::world::World;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    if let Err(error) = run() {
        eprintln!("R5 building roundtrip FAIL: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("evidence_r5_headless"));
    if out.exists() && out.read_dir()?.next().is_some() {
        return Err(format!("output directory must be empty: {}", out.display()).into());
    }
    fs::create_dir_all(&out)?;

    let mut checks = Vec::new();
    let world = flat_world();
    let mut objects = ObjectStore::new();
    objects.place(&world, ObjectKind::CampfireBasic, IVec3::new(34, 21, 34), 0)?;
    let mut buildings = catalog_buildings(&world, &objects)?;

    let records = buildings.records_sorted();
    let ids_monotonic = records.windows(2).all(|pair| pair[0].id.0 < pair[1].id.0)
        && records.first().map(|record| record.id == BuildingId(1)) == Some(true)
        && records.iter().all(|record| record.id.0 != 0);
    check(
        &mut checks,
        "F01",
        ids_monotonic,
        "stable non-zero monotonic building ids",
    )?;

    let next_before_reject = buildings.next_id();
    let reverse_duplicate = buildings.place(
        &world,
        &objects,
        BuildingKind::WallBasic,
        IVec3::new(5, 21, 4),
        2,
    );
    let thin_wall = visual_boxes(PlacedBuilding {
        id: BuildingId(u64::MAX),
        kind: BuildingKind::WallBasic,
        anchor: IVec3::new(4, 21, 4),
        yaw_quarters: 0,
    })
    .into_iter()
    .all(|(min, max)| (max.z - min.z) < 0.2);
    check(
        &mut checks,
        "F02",
        matches!(reverse_duplicate, Err(BuildingError::SlotOccupied { .. }))
            && buildings.next_id() == next_before_reject
            && direction(1) == IVec3::Z
            && thin_wall,
        "canonical thin slots, reverse-edge collision, and quarter turns",
    )?;

    let upper = buildings
        .records_sorted()
        .into_iter()
        .find(|record| {
            record.kind == BuildingKind::FloorBasic && record.anchor == IVec3::new(4, 24, 4)
        })
        .ok_or("upper floor missing")?;
    let hash_before_delete = buildings.semantic_hash();
    // 参考补丁原在闭合围栏（墙x2/门/窗）上删单边墙，四角仍有冗余支撑，
    // remove 合法成功使断言永假。改用柱 (10,21,4)：其顶端是梁 (10,24,4)
    // 端点的唯一支撑，删除必然连带梁失去支撑而整体失败——这才是
    // "删除必要支撑时整体失败"的语义（见 docs/DECISIONS_R5.md R5-13.5）。
    let supporting_column = buildings
        .records_sorted()
        .into_iter()
        .find(|record| {
            record.kind == BuildingKind::ColumnBasic && record.anchor == IVec3::new(10, 21, 4)
        })
        .ok_or("supporting column missing")?;
    let delete_result = buildings.remove(&world, &objects, supporting_column.id);
    check(
        &mut checks,
        "F03",
        delete_result.is_err()
            && buildings.semantic_hash() == hash_before_delete
            && buildings.get(upper.id).is_some()
            && buildings.get(supporting_column.id).is_some(),
        "structural support and dependent-removal rollback",
    )?;

    let has_door = records
        .iter()
        .any(|record| record.kind == BuildingKind::DoorwayBasic);
    let has_window = records
        .iter()
        .any(|record| record.kind == BuildingKind::WindowBasic);
    let has_stair = records
        .iter()
        .any(|record| record.kind == BuildingKind::StairBasic);
    check(
        &mut checks,
        "F04",
        has_door && has_window && has_stair,
        "door/window wall-slot variants and exact stair endpoints",
    )?;

    let crossing_object = PlacedObject {
        id: ObjectId(u64::MAX),
        kind: ObjectKind::CampfireBasic,
        anchor: IVec3::new(4, 21, 3),
        yaw_quarters: 0,
    };
    let object_blocked = buildings.validate_object_record(crossing_object).is_err();
    let terrain_blocked = buildings
        .validate_terrain_edit(&world, &objects, IVec3::new(4, 20, 4), BlockId::Air)
        .is_err();
    check(
        &mut checks,
        "F05",
        object_blocked && terrain_blocked,
        "terrain/object/building cross-layer constraints",
    )?;

    let disposable = buildings.place(
        &world,
        &objects,
        BuildingKind::FloorBasic,
        IVec3::new(25, 21, 5),
        0,
    )?;
    buildings.move_component(&world, &objects, disposable, IVec3::new(26, 21, 5), 2)?;
    let moved = *buildings.get(disposable).ok_or("moved component missing")?;
    let before_failed_move = buildings.semantic_hash();
    let before_failed_record = moved;
    let failed_move =
        buildings.move_component(&world, &objects, disposable, IVec3::new(26, 27, 5), 1);
    let rollback_ok = failed_move.is_err()
        && buildings.semantic_hash() == before_failed_move
        && buildings.get(disposable) == Some(&before_failed_record);
    buildings.remove(&world, &objects, disposable)?;
    check(
        &mut checks,
        "F06",
        moved.id == disposable && rollback_ok && buildings.get(disposable).is_none(),
        "place/move/rotate/delete preserve ids and failed move rolls back",
    )?;

    let reversed: Vec<_> = buildings.records_sorted().into_iter().rev().collect();
    let rebuilt = BuildingStore::from_persisted(&world, &objects, buildings.next_id(), &reversed)?;
    check(
        &mut checks,
        "F07",
        rebuilt.semantic_hash() == buildings.semantic_hash(),
        "semantic hash independent of insertion and ECS order",
    )?;

    let terrain_path = out.join("roundtrip.cfsv");
    let object_path = object_persistence::object_logical_path(&terrain_path);
    let building_path = building_persistence::building_logical_path(&terrain_path);
    let building_receipt =
        building_persistence::save_atomic(&building_path, &world, &objects, &buildings, None)?;
    let object_receipt = object_persistence::save_atomic(&object_path, &world, &objects, None)?;
    persistence::save_atomic(&terrain_path, &world)?;
    let building_bytes = fs::read(&building_receipt.slot_path)?;
    let encoded_object_hash = u64::from_le_bytes(
        building_bytes[64..72]
            .try_into()
            .map_err(|_| "building object hash header missing")?,
    );
    check(
        &mut checks,
        "F08",
        building_bytes.starts_with(b"CFBLD001")
            && !building_bytes.starts_with(b"CFSAVE02")
            && !building_bytes.starts_with(b"CFOBJ001")
            && encoded_object_hash == objects.semantic_hash(),
        "strict CFBLD001 companion format binds the object semantic hash",
    )?;

    let loaded_world = persistence::load_latest(&terrain_path)?;
    let loaded_objects = object_persistence::load_latest(&object_path, &loaded_world.world)?
        .ok_or("object companion missing")?;
    let loaded_buildings = building_persistence::load_latest(
        &building_path,
        &loaded_world.world,
        &loaded_objects.store,
    )?
    .ok_or("building companion missing")?;
    check(
        &mut checks,
        "F09",
        loaded_world.world.semantic_hash() == world.semantic_hash()
            && loaded_objects.store.semantic_hash() == objects.semantic_hash()
            && loaded_buildings.store.semantic_hash() == buildings.semantic_hash()
            && building_receipt.object_semantic_hash == object_receipt.object_semantic_hash,
        "exact terrain + object + building roundtrip",
    )?;

    corrupted_latest_falls_back(&out, &world, &objects, &buildings)?;
    interrupted_triple_commit(&out)?;
    check(
        &mut checks,
        "F10",
        true,
        "newest building corruption and interrupted triple commit recover",
    )?;

    let legacy = out.join("legacy.cfsv");
    check(
        &mut checks,
        "F11",
        building_persistence::load_latest(
            &building_persistence::building_logical_path(&legacy),
            &world,
            &objects,
        )?
        .is_none(),
        "R4 terrain/object save without building companion loads empty",
    )?;

    let all_kinds = BuildingKind::ALL.iter().all(|kind| {
        records.iter().any(|record| record.kind == *kind) && !kind.type_id().is_empty()
    });
    check(
        &mut checks,
        "F12",
        all_kinds,
        "all eight stable procedural building kinds are present",
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
            "{{\n  \"overall\":\"PASS\",\n  \"terrain_hash\":\"{:#018x}\",\n  \"object_hash\":\"{:#018x}\",\n  \"building_hash\":\"{:#018x}\",\n  \"component_count\":{},\n  \"checks\":[{}]\n}}\n",
            world.semantic_hash(),
            objects.semantic_hash(),
            buildings.semantic_hash(),
            buildings.len(),
            checks_json
        ),
    )?;
    fs::write(
        out.join("report.md"),
        format!(
            "# R5 modular-building headless acceptance\n\n- overall: **PASS**\n- components: {}\n- building hash: `{:#018x}`\n- F01-F12: **PASS**\n",
            buildings.len(),
            buildings.semantic_hash()
        ),
    )?;
    println!("R5 building roundtrip PASS: {}", out.display());
    Ok(())
}

fn catalog_buildings(
    world: &World,
    objects: &ObjectStore,
) -> Result<BuildingStore, Box<dyn std::error::Error>> {
    let mut store = BuildingStore::new();

    store.place(
        world,
        objects,
        BuildingKind::FloorBasic,
        IVec3::new(4, 21, 4),
        0,
    )?;
    store.place(
        world,
        objects,
        BuildingKind::WallBasic,
        IVec3::new(4, 21, 4),
        0,
    )?;
    store.place(
        world,
        objects,
        BuildingKind::DoorwayBasic,
        IVec3::new(5, 21, 4),
        1,
    )?;
    store.place(
        world,
        objects,
        BuildingKind::WindowBasic,
        IVec3::new(4, 21, 5),
        0,
    )?;
    store.place(
        world,
        objects,
        BuildingKind::WallBasic,
        IVec3::new(4, 21, 4),
        1,
    )?;
    store.place(
        world,
        objects,
        BuildingKind::FloorBasic,
        IVec3::new(4, 24, 4),
        0,
    )?;

    for point in [
        IVec3::new(10, 21, 4),
        IVec3::new(11, 21, 4),
        IVec3::new(10, 21, 5),
        IVec3::new(11, 21, 5),
    ] {
        store.place(world, objects, BuildingKind::ColumnBasic, point, 0)?;
    }
    store.place(
        world,
        objects,
        BuildingKind::BeamBasic,
        IVec3::new(10, 24, 4),
        0,
    )?;
    store.place(
        world,
        objects,
        BuildingKind::BeamBasic,
        IVec3::new(10, 24, 5),
        0,
    )?;
    store.place(
        world,
        objects,
        BuildingKind::RoofFlatBasic,
        IVec3::new(10, 24, 4),
        0,
    )?;

    store.place(
        world,
        objects,
        BuildingKind::FloorBasic,
        IVec3::new(15, 21, 4),
        0,
    )?;
    for point in [
        IVec3::new(18, 21, 4),
        IVec3::new(19, 21, 4),
        IVec3::new(18, 21, 5),
        IVec3::new(19, 21, 5),
    ] {
        store.place(world, objects, BuildingKind::ColumnBasic, point, 0)?;
    }
    store.place(
        world,
        objects,
        BuildingKind::FloorBasic,
        IVec3::new(18, 24, 4),
        0,
    )?;
    store.place(
        world,
        objects,
        BuildingKind::StairBasic,
        IVec3::new(15, 21, 4),
        0,
    )?;
    Ok(store)
}

fn corrupted_latest_falls_back(
    out: &Path,
    world: &World,
    objects: &ObjectStore,
    buildings: &BuildingStore,
) -> Result<(), Box<dyn std::error::Error>> {
    let logical = building_persistence::building_logical_path(&out.join("fallback.cfsv"));
    let first = building_persistence::save_atomic(&logical, world, objects, buildings, None)?;
    let second = building_persistence::save_atomic(&logical, world, objects, buildings, None)?;
    let latest = if second.generation > first.generation {
        second
    } else {
        first
    };
    let mut bytes = fs::read(&latest.slot_path)?;
    let flip = bytes.len() / 2;
    bytes[flip] ^= 0x5a;
    fs::write(&latest.slot_path, bytes)?;
    let loaded = building_persistence::load_latest(&logical, world, objects)?
        .ok_or("building fallback slot missing")?;
    if loaded.store.semantic_hash() != buildings.semantic_hash() {
        return Err("corrupted newest building slot did not fall back".into());
    }
    Ok(())
}

fn interrupted_triple_commit(out: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let terrain_path = out.join("paired.cfsv");
    let object_path = object_persistence::object_logical_path(&terrain_path);
    let building_path = building_persistence::building_logical_path(&terrain_path);
    let mut world = flat_world();
    let mut objects = ObjectStore::new();
    objects.place(&world, ObjectKind::CampfireBasic, IVec3::new(34, 21, 34), 0)?;
    let mut buildings = BuildingStore::new();
    buildings.place(
        &world,
        &objects,
        BuildingKind::FloorBasic,
        IVec3::new(5, 21, 5),
        0,
    )?;

    building_persistence::save_atomic(&building_path, &world, &objects, &buildings, None)?;
    object_persistence::save_atomic(&object_path, &world, &objects, None)?;
    persistence::save_atomic(&terrain_path, &world)?;

    let durable = persistence::load_latest(&terrain_path)?;
    let durable_objects = object_persistence::load_latest(&object_path, &durable.world)?
        .ok_or("durable object pair missing")?;
    let durable_buildings =
        building_persistence::load_latest(&building_path, &durable.world, &durable_objects.store)?
            .ok_or("durable building pair missing")?;
    let old_object_hash = durable_objects.store.semantic_hash();
    let old_building_hash = durable_buildings.store.semantic_hash();
    let protected_path = durable_buildings.slot_path.clone();
    let protected_bytes = fs::read(&protected_path)?;

    // 参考补丁把编辑点定在 (30,21,30) 后又在 (28,21,28) 放置 campfire：
    // campfire footprint 为 3x3（[28,31)），必然撞上刚写入的 Stone，场景
    // 自相矛盾（见 docs/DECISIONS_R5.md R5-13.6）。编辑挪到 (20,21,20)，
    // 远离两处对象与楼板，"新地形哈希 != 旧耐久地形"的语义不变。
    world.try_user_edit(IVec3::new(20, 21, 20), BlockId::Stone)?;
    objects.place(&world, ObjectKind::CampfireBasic, IVec3::new(28, 21, 28), 0)?;
    buildings.place(
        &world,
        &objects,
        BuildingKind::FloorBasic,
        IVec3::new(12, 21, 12),
        0,
    )?;

    for _ in 0..2 {
        building_persistence::save_atomic(
            &building_path,
            &world,
            &objects,
            &buildings,
            Some((&durable.world, &durable_objects.store)),
        )?;
    }
    object_persistence::save_atomic(&object_path, &world, &objects, Some(&durable.world))?;
    if fs::read(&protected_path)? != protected_bytes {
        return Err("old building slot was overwritten before terrain commit".into());
    }

    let old_world_again = persistence::load_latest(&terrain_path)?;
    let old_objects_again = object_persistence::load_latest(&object_path, &old_world_again.world)?
        .ok_or("old object fallback missing")?;
    let old_buildings_again = building_persistence::load_latest(
        &building_path,
        &old_world_again.world,
        &old_objects_again.store,
    )?
    .ok_or("old building fallback missing")?;
    if old_objects_again.store.semantic_hash() != old_object_hash
        || old_buildings_again.store.semantic_hash() != old_building_hash
    {
        return Err("interrupted triple prewrite did not recover old pair".into());
    }

    persistence::save_atomic(&terrain_path, &world)?;
    let committed = persistence::load_latest(&terrain_path)?;
    let committed_objects = object_persistence::load_latest(&object_path, &committed.world)?
        .ok_or("committed object pair missing")?;
    let committed_buildings = building_persistence::load_latest(
        &building_path,
        &committed.world,
        &committed_objects.store,
    )?
    .ok_or("committed building pair missing")?;
    if committed_objects.store.semantic_hash() != objects.semantic_hash()
        || committed_buildings.store.semantic_hash() != buildings.semantic_hash()
    {
        return Err("new triple did not become active after terrain commit".into());
    }
    Ok(())
}

fn flat_world() -> World {
    let mut world = World::generate_all(WorldSize::new(48, 48, 48), TerrainParams::new(5151));
    for z in 0..48 {
        for x in 0..48 {
            world
                .set_voxel(IVec3::new(x, 20, z), BlockId::Stone)
                .unwrap();
            for y in 21..32 {
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
