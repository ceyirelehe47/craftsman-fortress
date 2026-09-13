# R2 存档格式与恢复规则

## 存储模型

存档不复制整个体素世界。文件保存：

- Seed；
- 世界尺寸；
- 生成器修订号；
- 相对确定性 Seed 世界的最终体素修改覆盖层；
- 基础世界与修改后世界的语义哈希；
- 整体校验值。

加载流程始终先用 Seed 和尺寸重新生成基础世界，再应用覆盖层，最后核对两个语义哈希。

## 双槽原子写入

逻辑路径 `saves/quicksave.cfsv` 对应：

```text
saves/quicksave.cfsv.slot0
saves/quicksave.cfsv.slot1
```

每次保存写入非当前最新槽位：

1. 在同目录创建唯一临时文件；
2. 写完整内容并 `sync_all`；
3. 删除待复用的旧槽（另一最新有效槽仍保留）；
4. 将临时文件重命名为目标槽；
5. 重新解码并核对 generation 与世界哈希。

加载时分别验证两个槽，忽略损坏槽并选择 generation 最大的有效槽。这个协议不依赖 Windows 的“覆盖式 rename”语义。

## 二进制布局（小端序）

```text
magic[8]             = "CFSAVE02"
format_version u32   = 1
generator_revision u32
generation u64
seed u64
size_x u32
size_y u32
size_z u32
edit_count u32
base_semantic_hash u64
world_semantic_hash u64
records[edit_count]
checksum u64         = FNV-1a 64 over all previous bytes
```

每条记录固定 16 字节：

```text
x i32
y i32
z i32
block_id u8
reserved[3] = 0
```

记录必须按坐标规范化，不允许重复，不允许保存与生成基础值相同的无效覆盖项。

## R2 编辑边界

- 基岩及其保护层不可删除或替换；
- `Bedrock` 不可作为玩家放置材料；
- 世界最顶层 `H-1` 保持为空气，继续作为 R1.1 相机恢复安全层；
- 本轮放置材料固定为 `Stone`，不实现材料选择 UI；
- 损坏、截断、未知格式版本、未知生成器修订、未知方块 ID、越界坐标、重复记录均必须拒绝，不得猜测修复。
