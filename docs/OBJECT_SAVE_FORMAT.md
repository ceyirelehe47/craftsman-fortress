# CFOBJ001 对象伴随存档

地形文件继续使用 `CFSAVE02`，R4 不修改其 magic、版本或记录布局。对象逻辑路径由地形逻辑路径派生：

```text
saves/colony.cfsv
saves/colony.cfsv.objects.slot0
saves/colony.cfsv.objects.slot1
```

## 头部（72 字节，小端序）

```text
magic[8]                 = CFOBJ001
format_version u32       = 1
schema_revision u32      = 1
generation u64
seed u64
world_size_x/y/z u32
object_count u32
next_object_id u64
terrain_semantic_hash u64
object_semantic_hash u64
```

## 对象记录（48 字节）

```text
object_id u64
type_id[24]              NUL 填充的稳定 ASCII 类型 ID
anchor_x/y/z i32
yaw_quarters u8          0..3
reserved[3]              必须为 0
```

记录必须按 `object_id` 严格递增；ID 0、重复 ID、未知类型、非法旋转、非零保留字节、越界放置、地形相交、缺少支撑、对象重叠或语义哈希不一致均拒绝加载。

文件尾为 64 位 FNV-1a 校验。最大对象数为 100,000，读取前检查文件 metadata 长度，禁止无界分配。

## 与地形提交的关系

1. F5 前读取当前磁盘最新完整地形，得到“旧耐久地形”。
2. 对象层先写新槽，但绝不覆盖与旧耐久地形哈希匹配的完整有效对象槽。
3. 再原子保存地形。
4. 加载时只选择 `terrain_semantic_hash` 与已加载地形完全一致的最大 generation 对象槽。

因此对象预写后、地形提交前崩溃时，旧地形仍可加载旧对象槽；地形提交成功后，新对象槽才会自然生效。R4 不引入跨文件事务日志，但通过哈希配对和受保护旧槽保证可恢复性。
