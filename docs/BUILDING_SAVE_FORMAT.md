# `CFBLD001` 建筑构件伴随存档

地形和对象格式保持不变。建筑逻辑路径从地形逻辑路径派生：

```text
saves/colony.cfsv
saves/colony.cfsv.objects.slot0
saves/colony.cfsv.objects.slot1
saves/colony.cfsv.buildings.slot0
saves/colony.cfsv.buildings.slot1
```

## 头部（80 字节，小端序）

```text
magic[8]                  = CFBLD001
format_version u32        = 1
schema_revision u32       = 1
generation u64
seed u64
world_size_x/y/z u32
component_count u32
next_building_id u64
terrain_semantic_hash u64
object_semantic_hash u64
building_semantic_hash u64
```

## 构件记录（48 字节）

```text
building_id u64
type_id[24]               NUL 填充的稳定 ASCII 类型 ID
anchor_x/y/z i32
yaw_quarters u8           0..3
reserved[3]               必须为 0
```

记录必须按 `building_id` 严格递增。文件尾为前面全部字节的 64 位 FNV-1a 校验。

## 严格加载

以下任一情况都必须拒绝该槽：

- magic、版本或 schema revision 未知；
- 截断、尾随字节、长度与记录数不一致；
- 校验和不一致；
- 构件数超过 100,000 或文件超过由格式常量推导的上限；
- ID 为 0、重复、乱序，或 `next_building_id` 不大于最大 ID；
- 类型未知、旋转超范围、保留字节非零；
- Seed、世界尺寸、terrain hash 或 object hash 不匹配；
- 物化后构件越界、触顶、穿地形、穿对象、槽位冲突、楼梯冲突或结构支撑无效；
- 重算后的 building semantic hash 不一致。

加载只选择与当前地形和对象层同时匹配、generation 最大的完整有效槽。

## 原子写入与三层提交

建筑文件沿用双槽、同目录临时文件、`sync_all`、原子重命名和写后重读验证。保存时不得覆盖与磁盘旧耐久地形和旧对象层匹配的最新完整建筑槽。

三层提交顺序：

1. 建筑预写；
2. 对象预写；
3. 地形提交。

地形未提交时，新建筑槽或新对象槽均不会和旧地形形成完整匹配；旧三层状态仍可恢复。地形提交后，新对象槽与新建筑槽一起成为当前状态。

## 历史兼容

完全没有建筑槽时返回空建筑层，用于加载 R4 或更早的 terrain/object 存档。只要任一建筑槽文件存在但没有完整有效匹配槽，就必须报告详细失败原因。
