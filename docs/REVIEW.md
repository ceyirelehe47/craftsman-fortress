# R2 独立复核报告

- **代码验收提交 S**：`3b5ae690e34900a9d380cbf93d50a5e5a2177ef5`
- **实现方 run_id / 证据目录**：`evidence_impl_3b5ae690e349_20260914_000049`
- **Reviewer run_id / 证据目录**：`evidence_review_3b5ae690e349_20260914_002720`
- **实现方命令**：`ACCEPTANCE_WATCHDOG_SEC=1500 bash scripts/r2_acceptance.sh evidence_impl_3b5ae690e349_20260914_000049`（全新克隆检出 S，预热编译后运行，总耗时 652s）
- **Reviewer 命令**：`ACCEPTANCE_WATCHDOG_SEC=1500 bash scripts/r2_acceptance.sh evidence_review_3b5ae690e349_20260914_002720`（独立全新克隆 `git clone --no-local /d/code/game` 后检出 S，预热编译后运行，总耗时 656s）
- **最终结论**：`PASS`

## 1. 基线与工作区

- R2 起点标签：`r1.1-baseline-73bdffc685d8`（解析到 `73bdffc685d8a8a0c5ceaa258f3e7ae7f4aff44f`，不可移动，R1.1 标签与 Release 原样保留）
- 实现分支 `r2-mutable-world-save` 从该标签创建，S = 基线 + R2 参考补丁（16 文件）+ 唯一编译修正（`cargo fmt` 格式化 `examples/r2_roundtrip.rs` 三处换行）
- 实现方工作区：干净（`git status --porcelain` 为空，HEAD 精确等于 S）
- Reviewer 工作区：干净（独立 `--no-local` 全新克隆，`git rev-parse HEAD` 精确等于 S）
- Bevy：`0.19.1`（`scripts/audit_bevy.sh` 通过，0.19 系列锁定）；Rust `1.98.1`
- S 的 CI：PR #1（head = S）`checks` 通过，run https://github.com/ceyirehe47/craftsman-fortress/actions/runs/34766499136 ；随后以 merge commit 方式合入 main（S 的 SHA 原样保留在 main 历史）

## 2. R1 回归

| 项目 | 结论 | 独立依据 |
|---|---|---|
| A01–A14 | PASS | Reviewer 自己证据目录 `r1/report.json` overall=PASS（入口退出码 0、应用退出码 0、日志 0 panic/ERROR/FATAL）；以下逐项复核 |

Reviewer 对自己 `metrics.csv`（627 采样）与 `report.json` 的独立复算，与实现方同口径数据对比：

| 指标 | Reviewer 复算 | 实现方复算 | 门槛 |
|---|---|---|---|
| TPS（最小二乘回归，全程端点） | **20.00** | **20.00** | 20±2% |
| visible-unready 最大违规帧 | **0** | **0** | =0 |
| mesh_assets | **首=尾=1020 恒定** | **1020 恒定** | 守恒 |
| RSS 峰值 | 537MB | 533MB | 平稳 |
| 截图 | 10 张全 1280×720 | 10 张 | 一致 |
| GIF | GIF89a、64 帧、2,475,040B | 64 帧 | 可解码 |

截图目检：`A05_edit_rebuild.png` 中央竖直开挖沟槽边界整齐且 HUD dirty=0；`A09_picking_highlight.png` HUD `Pick: (38,35,6) face PosY` + 黄色高亮框；帮助行已显示 R2 新交互 `Delete remove · B place Stone · F5 save`。两轮截图逐张字节尺寸同量级、内容与 R1.1 场景一一相符，无回归。

## 3. R2 验收

| ID | 结论 | Reviewer 独立依据 |
|---|---|---|
| B01 删除与放置 | PASS | 我的 r2_roundtrip 输出 PASS：`try_user_edit` 删除实体/放置 Stone 均改变权威世界并计覆盖项；无命中时不动世界（`Remove/Place: no voxel selected` 路径） |
| B02 Chunk 边界更新 | PASS | 程序断言：边界编辑后 dirty 同时含本块与 (−1,0,0)、(0,0,−1) 邻块；重建后 Mesh 顶点/法线/颜色/索引与重算网格逐字节一致，队列清空 |
| B03 覆盖层规范化 | PASS | 程序断言：改动→恢复 Seed 原值后 `modification_count` 回落（记录自动删除）；单元测试 `edit_overlay_normalizes_back_to_generated_value` 在我的克隆真实执行通过 |
| B04 编辑安全边界 | PASS | 程序断言基岩/保护层拒绝、顶层 H-1 实体放置拒绝；`user_edit_rules_protect_bedrock_and_top_air` 执行通过；解码端同样拒绝基岩与顶层实体记录 |
| B05 格式严格性 | PASS | 单元测试 `decoder_rejects_truncation_version_checksum_and_unknown_block`、`decoder_rejects_duplicate_and_forbidden_records` 执行通过（截断/未知版本/未知修订/校验和/未知方块/重复/保留字节/顶层/基岩全拒绝）；我另用 Python 独立复算 FNV-1a：两槽 stored=calculated，翻转 1 字节后必不等于 stored（必拒收） |
| B06 精确往返 | PASS | 程序断言保存前后 `semantic_hash`、Seed、尺寸、`modifications_sorted` 完全一致；我独立解析槽文件头：fmt_v=1、gen_rev=1、seed=424242、64³、edits=3、base/world 哈希与 report.json 一致 |
| B07 顺序独立 | PASS | 程序断言：相同最终修改以相反顺序施加后 `semantic_hash` 与 `modifications_sorted` 与原顺序一致，单槽 `order.cfsv`（gen=1）加载回读一致；单元测试 `semantic_hash_ignores_palette_edit_order` 执行通过 |
| B08 双槽轮换 | PASS | 我用二进制偏移独立提取两轮证据槽文件 generation：slot0=1、slot1=2（impl 与 review 两轮同构）——第二次保存写入非最新槽、旧有效槽 gen=1 原样保留 |
| B09 损坏回退 | PASS | 程序断言（两轮 PASS）：翻转最新槽（gen2）字节后 `load_latest` 回退 gen1 旧槽且世界语义哈希一致；单元测试 `corrupted_latest_slot_falls_back` 执行通过；B05 的 FNV 独立复算证明翻转必被校验拒绝 |
| B10 槽位修复 | PASS | 程序断言（两轮 PASS）：回退后再次保存产生 gen=2 的新的有效最新槽，重新加载一致（`repaired_generation=2`） |
| B11 启动加载 | PASS | 单元测试 `load_becomes_default_save_target`（load 后 F5 回写加载路径）、`parse_rejects_bad`（`--acceptance --load` 拒绝）执行通过；`app.rs` 加载失败走 `exit(3)` 非零退出且不进入游戏循环 |
| B12 无回归与复核 | PASS | 第 2 节 R1 全回归 + 本报告；两轮聚合 report.json overall 均为 PASS，`commit.txt` 均为 S |

两轮 `cargo test --release` 均为 `66 passed; 0 failed`（R1.1 基线 56 项 + R2 新增 10 项：persistence 5 项、world 3 项、generation 点生成 1 项、config 3 项中新增 2 项、voxel 1 项，实际分类见源码）；我在自己克隆中单独过滤重跑 `persistence`（5/5）与 `semantic_hash`（1/1）确认真实执行。

## 4. 独立复算

Reviewer 用 Python 对自己证据目录的槽文件与 metrics.csv 独立计算，非复述实现方数据。

- Seed / 尺寸：`424242` / `64×64×64`（两轮 report.json 与槽文件二进制解析三方一致）
- 基础语义哈希：`0x72629ae6a816d059`
- 修改后语义哈希：`0x8e347f5261e1aa1e`
- 覆盖项数量：`3`
- generation 序列：`slot0=1 → slot1=2`（每轮相同；保存轮换关系 = 第二次写非最新槽）
- 损坏槽：最新槽 `slot1`（gen=2）翻转 1 字节——独立 FNV-1a 复算 stored≠calculated，必被校验拒绝
- 回退槽：`slot0`（gen=1）继续可加载且世界哈希 = `0x8e347f5261e1aa1e`；修复保存后新槽 gen=2 再次可加载
- FNV-1a 常量独立取 `0xcbf29ce484222325` / `0x100000001b3`，与 `src/persistence.rs` 实现一致，两槽全文件复算均 OK

## 5. 证据包

| 文件 | SHA-256 |
|---|---|
| `evidence_impl_3b5ae690e349.zip` | `4824554c20c9dae8a77b302ce923a6407dc7ee81229f9fcc708766f5a02f1ffd` |
| `evidence_review_3b5ae690e349.zip` | `9dcbbf851956a3f5d685e6325f8ea4defbbaa7cb172dd5c29d6753a970ed17c7` |

两个 ZIP 内 `commit.txt` 同为 `3b5ae690e34900a9d380cbf93d50a5e5a2177ef5`，根级、`r1/report.json`、`r2/report.json` 全部 PASS，各含 97 个证据文件（含双槽 `roundtrip.cfsv.slot0/.slot1` 与 `order.cfsv.slot0`、运行日志、SHA256SUMS 清单）。

## 6. 非阻断观察项

1. **r2_roundtrip report.json 的 checks ID 与 R2 验收门槛编号错位**：程序报告把双槽原子保存标为 B05、精确往返 B06、损坏回退 B07、槽位修复 B08，而 R2 验收门槛 B05–B10 分别对应格式严格性/精确往返/顺序独立/双槽轮换/损坏回退/槽位修复。行为断言本身全部存在且两轮通过（本表第 3 节按验收门槛 ID 逐项标注实际依据），仅自述标签偏移；不影响任何门槛判定。
2. **`cargo fmt` 是参考补丁唯一修正**：补丁未在本机制包环境编译过（PATCH_VALIDATION 已声明），实际仅 `examples/r2_roundtrip.rs` 三处换行与 rustfmt 期望不符，无逻辑修改；clippy/test/release 在补丁应用后一次通过。
3. **CI 触发路径**：`ci.yml` 仅在 push main 与 pull_request 触发，S 通过 PR #1（head=S）获得 CI 证据后以 merge commit 合入 main；两轮验收入口自身完整重跑 fmt/clippy/test/build，与 CI 等效。

---

*Reviewer：R2 独立验收（隔离工作区，`git clone --no-local` 全新克隆检出 S）；报告日期：2026-09-14*
