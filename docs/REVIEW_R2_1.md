# R2.1 独立复核报告

- **代码验收提交 S**：`804df0d8513abac88d0cf66f2bc2f33a9562331f`
- **实现方证据包 / SHA-256**：`evidence_impl_804df0d8513a.zip` / `eaf27178c3fc013405825fab854f5621c439addfd77cc4ada9ea1bf389d6d8e6`
- **Reviewer 证据包 / SHA-256**：`evidence_review_804df0d8513a.zip` / `767efdb659785b5a84af5daa028e882eec5457920cf22a0079859ab2f0adde28`
- **结论**：`PASS`

## 隔离性

- 全新克隆：`D:\code\craftsman-r21-review`（`git clone --no-local`，与实现方克隆 `D:\code\craftsman-r21-impl` 互不复用 target、证据目录与存档槽）
- 干净工作区：`git status --porcelain` 为空
- HEAD：`804df0d8513abac88d0cf66f2bc2f33a9562331f`（= S）
- Bevy：`0.19.1`（`Cargo.toml` 第 10 行 `bevy = "=0.19.1"`）

## R1 回归

A01–A14：**PASS**。Reviewer 证据 `r1/report.json` overall=PASS，A01–A13 逐项 PASS（A14 由本报告独立确认）；TPS 最小二乘拟合全程斜率 19.9999，巡航段 614 样本无一低于 19；`cruise_timelapse.gif` 结构化解析 64 帧（GIF89a、GCE 与图像描述符各 64）；10 张截图均为 1280×720 且字节数与报告摘录一致。

## R2.1 B01–B12

| ID | 结论 | Reviewer 独立依据 |
|---|---|---|
| B01 删除与放置 | PASS | roundtrip 证据双方一致（edit_count=3）；单测 `no_selection_edit_is_a_true_noop` 重跑通过 |
| B02 跨 Chunk 更新 | PASS | roundtrip 断言自身与 -x/-z 邻居同时标脏；R1 A05 证据同样覆盖 |
| B03 覆盖层规范化 | PASS | roundtrip 恢复原值后 modification_count 不变；Reviewer 解析槽记录仅 3 条有效覆盖 |
| B04 编辑安全边界 | PASS | 基岩/顶层拒绝断言通过；解码端 `validate_edit` 逐条复核（persistence.rs L189-220） |
| B05 格式严格性 | PASS | Python 独立解析 4 个槽：magic/版本/修订/长度/FNV 全部吻合，记录严格递增、reserved=0；乱序与超限 edit_count 拒绝单测重跑通过 |
| B06 精确往返 | PASS | 加载后语义哈希与 Mesh 逐字段等价断言在两侧证据中均 PASS |
| B07 顺序独立 | PASS | 反向编辑顺序产生相同语义哈希与规范化记录，两侧证据一致 |
| B08 双槽轮换 | PASS | generation 1→2、slot0→slot1 轮换；Reviewer 二进制解析 generation 字段吻合 |
| B09 完整有效槽损坏回退 | PASS | Reviewer 自建临时槽（非实现方文件）：base hash 与 world hash 分别 XOR 后重算 FNV，`--load` 退出码 3，stderr 同时含两槽失败原因（`base hash mismatch` / `world hash mismatch`），报错值与篡改值逐一吻合 |
| B10 槽位修复与 generation 溢出保护 | PASS | `save_preserves_only_fully_valid_old_slot`（L682-697，第 690 行旧槽字节断言）与 `generation_overflow_is_rejected_before_write`（L700-713）重跑通过；`save_atomic` L474-477 `checked_add` 溢出即拒 |
| B11 启动加载与错误拒绝 | PASS | 双方 `invalid_load_exit_code.txt`=3；日志含「存档加载失败」与逐槽原因、不含「启动：」；脚本 L71-83 断言齐备 |
| B12 R1 无回归及独立复核支持 | PASS | 本报告 + 双方 R1 全 PASS；`cargo test --release` 全量 74 通过（persistence 12 项含全部语义损坏场景） |

## 必查恢复场景

- base hash 错误且 FNV 已重算：**拒绝并回退**（Reviewer 独立篡改实验槽 A + 单测重跑）
- world hash 错误且 FNV 已重算：**拒绝并回退**（独立实验槽 B + 单测重跑）
- 记录合法篡改且 FNV 已重算：**拒绝并回退**（`semantically_changed_record_with_valid_checksum_falls_back` 重跑通过；与普通 FNV 翻转场景 `corrupted_latest_slot_falls_back` 分属两类，前者经 `refresh_checksum`）
- 两槽语义无效：**退出码 3，双槽双原因报错**（单测 `both_semantically_invalid_slots_report_both_failures` + 独立实验）
- 保存时保留唯一完整有效旧槽：**字节不变断言通过**；语义无效高槽因 `read_materialized` 失败不能成为 latest，覆盖目标落其上
- generation 溢出拒绝：**通过**（槽内实测 generation=1/2，远低于 u64::MAX；`encode u64::MAX` 后保存被拒）

## 进程与 watchdog

- 损坏存档主程序退出码：**3**（Reviewer 证据 `r2/invalid_load_exit_code.txt`）
- 是否进入游戏循环／创建窗口：**否**（日志无「启动：」横幅；退出发生在 `App::new` 之前，app.rs L45-49）
- headless watchdog：**存在且实际触发**（`R2_HEADLESS_WATCHDOG_SEC=0` 下脚本以 124 退出并生成 `headless_watchdog.txt`，含 `r2_roundtrip exceeded 0s`）

## Release

- 标签：`r2.1-baseline-804df0d8513a`
- 标签解引用目标：`804df0d8513abac88d0cf66f2bc2f33a9562331f`（= S）
- 历史 `r2-baseline-3b5ae690e349` 未移动：**是**（标签对象 `715313a8` 原样保留）
- CI：PR #2（run 34774953702）与 main 合并后均绿，含 R2.1 headless 恢复与启动拒绝步骤
- Release 三资产从空目录重新下载并 `sha256sum -c`：全部 OK

## 非阻断观察项

1. 参考补丁的 `scripts/r2_headless_checks.sh` 存在两处环境缺陷（不构建主程序二进制；日志重定向与往返程序「输出目录必须为空」守卫冲突），实现方已在 S 中修正并记录于 `docs/DECISIONS_R2.md` R2.1-04，修正不涉及存档协议语义。
2. Reviewer 与实现方的 roundtrip 数据逐字段一致（seed=424242、64³、base_hash=`0x72629ae6a816d059`、modified_hash=`0x8e347f5261e1aa1e`、edits=3、generation 1→2），确定性复核通过。
