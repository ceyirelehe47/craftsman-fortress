# R3.1 独立复核报告

- 代码验收提交 S：`562c2dfdc8b40c8196aad698e9af91e7fc685f06`（分支 `r3-1-source-chain-seal` 顶端，本地与 `origin/r3-1-source-chain-seal` 均指向该提交）
- 基线：`r3-baseline-355344c10846`
- 实现方证据 / SHA-256：`evidence_impl_562c2dfdc8b4_20260914_150012.zip` / `9366e5095250a06a61d8e2cdfaa11c0abc344b71d157185ce76d0149f55ee82e`
- Reviewer 证据 / SHA-256：`evidence_review_562c2dfdc8b4_20260914_152148.zip` / `ff6202f138e302a0444a62492d4abeab3f023d6d40701e6e58984b1191375f7a`
- 结论：`PASS`

## 隔离性与自主取得

- Reviewer 全新克隆：是（`git clone --no-local` 自 GitHub 远端至 `D:/code/game_r3_1_review`，检出 S；Rust 1.98.1 / cargo 1.98.1）
- 工作区干净：是（`git status --porcelain` 为空；`git log --oneline -1` 为 `562c2df`）
- Bevy：`0.19.1`（`Cargo.toml` 中 `bevy = "=0.19.1"`）
- 素材来源：`private_cache`（`R3_ASSET_CACHE_FILE` 指向受控只读缓存，未使用 R3_SOURCE_CHAIN_FIXTURE）
- 用户交互：必须 `none`（acquisition_receipt 与 source_chain 报告均为 `user_interaction=none`）
- 原包 SHA-256 / 字节数：`ababc51f543ec06d07e68d95cdcc90d8ae878d6d12908dd90a0746a836e82fed` / 1913239（缓存原件与克隆内 `third_party_raw/Free_Sample.rar` 均独立重算一致）
- 实现方与 Reviewer 是否从各自克隆运行 acquire + prepare：是（Reviewer 在自己的全新克隆内运行 `scripts/r3_1_accept_from_cache.sh`，完整执行 acquire → prepare → R3.1 验收一条链；未复制实现方的素材、target 或证据目录）

## 来源链

- actual source_receipt SHA 等于 selection / lock：是（实际 `source_receipt.txt` 重算 `ed164156d4aa5e68c3bbf9dd8674f22dec598e4f6da9d5559df3369d3817c571`，与 `selection.tsv` 元数据及 lock 记录一致）
- inventory SHA 等于 lock：是（`inventory.tsv` 重算 `6917813129b02857e4db26fcefdeb303ef14cb76d63e19eeab4e7f618b74220f`）
- selection SHA 等于 lock：是（`selection.tsv` 重算 `cc90d48359bee48a93d9638683cbd30d432d3350ed5eb0773c2bd3e4e38f9529`）
- inventory 每个 GLB/PNG 重新计算通过：是（python 逐行重算 24/24 项 sha256 与字节数全部匹配，0 失败）
- selection.lock 每项与 selection + inventory + 实际文件一致：是（`r3_verify_local_assets.sh` PASS；`selection.lock.tsv` 重算 sha256 `79f4c6b58b7a6c855f1d90f1f174ee9acd150e4a052852a8220ec4673b716da0` 即 asset_set_sha256）
- asset_set_sha256（实现方 / Reviewer）：`79f4c6b58b7a6c855f1d90f1f174ee9acd150e4a052852a8220ec4673b716da0` / `79f4c6b58b7a6c855f1d90f1f174ee9acd150e4a052852a8220ec4673b716da0`（一致）

## 回归

- A01–A14：全 PASS（A01–A13 为 Reviewer 克隆内运行时判定，见 `evidence_review_.../r2_1/r1/report.md`；A14 即本独立复核报告，结论 PASS）
- B01–B12：全 PASS（`evidence_review_.../r2_1/r2/report.json`，B01–B12 逐项 PASS，overall PASS）
- C01–C12：内嵌 R3 回归 PASS（report.json `r3=PASS`）；关键指标独立复核一致——transform_fingerprint `0x774f906ff0c110eb` 与 R3 轮完全相同；manifest_hash 由 R3 轮 `0x702aeabc60f713fd` 更新为 `0xa8bab23f45af6275`，原因是 R3.1 将 `license_receipt_sha256` 纳入清单规范哈希输入（`src/asset_manifest.rs`），两方双跑四处一致
- CFSAVE02 未变化：`git diff r3-baseline-355344c10846 -- src/persistence.rs` 输出 0 行（magic `CFSAVE02` 所在文件基线→S 零改动）

## R3.1 D01–D12

| ID | 结论 | Reviewer 独立依据 |
|---|---|---|
| D01 | PASS | Reviewer 全新克隆自主取得：acquisition_receipt 记录 `source_kind=private_cache`、`user_interaction=none`，无人工下载/转交 |
| D02 | PASS | 原包重算 sha256 `ababc51f…fed`、1913239 字节，与受控缓存及脚本内置期望值一致 |
| D03 | PASS | `source_receipt.txt` 重算 `ed164156…c571`，与 selection/lock 记录一致；负向追加 1 行后 verify 以 exit 1 拒绝 |
| D04 | PASS | `inventory.tsv` 重算 `691781…220f`；python 逐行重算 24/24 文件 sha256+字节数全对 |
| D05 | PASS | `selection.lock.tsv` 重算 `79f4c6…6da0` = asset_set_sha256；`r3_verify_local_assets.sh` 全链 PASS（inventory_files=24、selected_assets=6） |
| D06 | PASS | Reviewer 与实现方各自从独立克隆运行 acquire+prepare+验收，四链 SHA、asset_set、manifest_hash、transform_fingerprint、inventory/selection 计数全部一致 |
| D07 | PASS | 两方证据 ZIP 经 `check_vendor_assets.sh` 均通过（142 条目、0 个 `.glb/.gltf/.fbx/.obj/.rar`、0 条 `vendor_local/third_party_raw` 路径）；改名贴图 ZIP 与嵌套 ZIP 负向均被拒，白噪声 PNG 对照通过 |
| D08 | PASS | lab_a/lab_b `app_stdout.log` 中 B0004/panic/ERROR/FATAL 计数均为 0；层级可见性完整（验收 D08 判定 PASS） |
| D09 | PASS | lab_a/lab_b `asset_lifecycle.csv`：baseline、cycle_1–cycle_5、final 全部为 12,9,16,417，5 轮延迟 despawn 后回落基线 |
| D10 | PASS | manifest_hash `0xa8bab23f45af6275`、transform_fingerprint `0x774f906ff0c110eb`：Reviewer 双跑一致且与实现方一致 |
| D11 | PASS | 内嵌 R1 回归 A01–A13 全 PASS、R2.1 回归 B01–B12 全 PASS；`src/persistence.rs` 基线→S 零改动 |
| D12 | PASS | 两方证据 ZIP 结构完整（report.json/report.md、lab_a/lab_b、manifest_check.log、r2_1/、SHA256SUMS.txt），python 逐条目重算内部 SHA256SUMS 各 139 条全部通过；泄漏扫描通过 |

## 必查实验

- 修改选中 GLB 1 字节后来源链失败：是（`/tmp/neg_assets` 副本中 `axe_stone.glb` 追加 1 字节（3796→3797），`r3_verify_local_assets.sh` exit 1「inventory 文件内容不匹配」；复原后 exit 0）
- 修改 source_receipt 后来源链失败：是（追加 1 行后 exit 1「manifest/license-receipt SHA-256 不一致」expected `ed164156…` actual `f0eb07bf…`；复原后 exit 0）
- 把第三方 PNG 改名放入 ZIP 后泄漏守卫失败：是（`axe_stone.png` 字节改名 `screenshot.png` 打入 `/tmp/leak.zip`，exit 1「与第三方模型/贴图/原包字节完全一致」）
- 嵌套 ZIP 泄漏失败：是（外层含 `inner.zip`、内含改名 `presentation.png`，exit 1 解嵌套后命中）
- 对照：自造白噪声 PNG 独立 ZIP 通过（exit 0），证明守卫按内容哈希判定而非一刀切禁 PNG
- 两次 lab 日志 B0004 命中 0：是（lab_a/lab_b 均为 0，panic/ERROR/FATAL 同为 0）
- cycle_1..cycle_5 每轮都回到基线：是（两 lab 的 5 个 cycle 行均为 12,9,16,417）
- 实现方与 Reviewer manifest / transform / asset-set 三指纹一致：是（`0xa8bab23f45af6275` / `0x774f906ff0c110eb` / `79f4c6…6da0`）

## 截图独立统计（PIL 复核）

- lab_a/lab_b 各 5 张共 10 张 PNG：全部 1280×720；64 色量化调色板满（非空）；平均亮度 140.0–151.9（>80）；亮度标准差 26.2–39.6（>5）
- 双跑同名截图字节数逐张相同（550798/603298/613605/716695/575872），确定性渲染成立；目检 R3_01（lab_a）与 R3_02（lab_b）确认 3D 场景与约 5 个模型可见、无空白或渲染异常

## Release

- 标签 `r3.1-baseline-<S短SHA>` 解引用到 S：尚未创建（按 `publish_r3_1_release.sh` 时序，发布以本 Reviewer 报告为输入，在复核 PASS 后打标；`git ls-remote --tags` 确认当前无 `r3.1-baseline-*` 标签，无已存在标签/Release 被移动或覆盖）
- 历史 `r3-baseline-355344c10846` 未移动：是（本地与远程均解引用 `355344c10846f5ca63cac61a7fc9bbc3ba94f077`）
- Release 仅含实现方证据、Reviewer 证据与 SHA256SUMS：Release 尚未发布；`verify_r3_1_release.sh` 强制校验恰好两套 `evidence_*.zip` 且 `SHA256SUMS.txt` 逐一通过
- `verify_r3_1_release.sh` 从空目录下载复验通过：待发布后执行（本报告即发布前置输入；发布时序为 Reviewer PASS → publish → 空目录复验）
