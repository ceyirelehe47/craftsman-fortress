# R3.1 独立复核报告

> 本报告取代前一轮针对提交 `562c2dfdc8b4` 的作废复核（该提交的泄漏守卫存在递归深度污染 bug，其发布已撤销）。本轮为第二次独立重跑，对象为修复提交 S2。

- 代码验收提交 S：`bbcfdb8b8bcd4eda845a7f002accecd0289e1ac3`（S2，分支 `r3-1-guard-recursion-fix` 顶端，基于 main）
- 基线：`r3-baseline-355344c10846`
- 实现方证据 / SHA-256：`evidence_impl_bbcfdb8b8bcd_20260914_165454.zip` / `6832e72bcc95b9192f8d321cb9191408aceff07cb33aaa3ba4985819569da60f`（独立重算一致）
- Reviewer 证据 / SHA-256：`evidence_review_bbcfdb8b8bcd_20260914_172631.zip` / `d11516ce36d9a54fdd2e9d7bdbfc68dae63230baaf52e1cbea7b5cb1d2c10584`
- 结论：`PASS`

## 隔离性与自主取得

- Reviewer 全新克隆：是（`git clone --no-local` 自 GitHub 远端至 `D:/code/game_r3_1_review`，检出 S2；Rust 1.98.1 / cargo 1.98.1；未复用任何实现方目录、素材、target 或证据）
- 工作区干净：是（`git status --porcelain` 为空；`git rev-parse HEAD` = `bbcfdb8b8bcd4eda845a7f002accecd0289e1ac3`）
- Bevy：`0.19.1`（`Cargo.toml` 中 `bevy = "=0.19.1"`）
- 素材来源：`private_cache`（`R3_ASSET_CACHE_FILE` 指向受控只读缓存，未使用 R3_SOURCE_CHAIN_FIXTURE）
- 用户交互：必须 `none`（acquisition_receipt 与 source_chain 报告均为 `user_interaction=none`）
- 原包 SHA-256 / 字节数：`ababc51f543ec06d07e68d95cdcc90d8ae878d6d12908dd90a0746a836e82fed` / 1913239（缓存原件 sha256sum 与 python 双口径独立重算一致）
- 实现方与 Reviewer 是否从各自克隆运行 acquire + prepare：是（Reviewer 在自己的全新克隆内运行 `scripts/r3_1_accept_from_cache.sh`，完整执行 acquire → prepare → R3.1 验收一条链，总时长约 17 分钟，退出码 0，输出「R3.1 acceptance PASS」）

## 来源链

- actual source_receipt SHA 等于 selection / lock：是（实际 `source_receipt.txt` python 重算 `ed164156d4aa5e68c3bbf9dd8674f22dec598e4f6da9d5559df3369d3817c571`，与 `selection.tsv` 元数据及 lock 记录一致）
- inventory SHA 等于 lock：是（`inventory.tsv` python 重算 `6917813129b02857e4db26fcefdeb303ef14cb76d63e19eeab4e7f618b74220f`）
- selection SHA 等于 lock：是（`selection.tsv` python 重算 `cc90d48359bee48a93d9638683cbd30d432d3350ed5eb0773c2bd3e4e38f9529`）
- inventory 每个 GLB/PNG 重新计算通过：是（python 逐行重算 24/24 项 sha256、字节数与类型扩展名全部匹配，0 失败）
- selection.lock 每项与 selection + inventory + 实际文件一致：是（`r3_verify_local_assets.sh` PASS；`selection.lock.tsv` 重算 sha256 `79f4c6b58b7a6c855f1d90f1f174ee9acd150e4a052852a8220ec4673b716da0` 即 asset_set_sha256，repo 与证据副本一致）
- asset_set_sha256（实现方 / Reviewer）：`79f4c6b58b7a6c855f1d90f1f174ee9acd150e4a052852a8220ec4673b716da0` / `79f4c6b58b7a6c855f1d90f1f174ee9acd150e4a052852a8220ec4673b716da0`（一致）

## 回归

- A01–A14：全 PASS（A01–A13 于 Reviewer 克隆内运行判定，`evidence_review_.../r2_1/r1/report.json` 13/13 PASS；A14 即本独立复核报告，结论 PASS）
- B01–B12：全 PASS（`evidence_review_.../r2_1/r2/report.json` 12/12 PASS；顶层 `r2_1/report.json` overall=PASS）
- C01–C12：内嵌 R3 回归 PASS（report.json `r3=PASS`）；关键指标独立复核一致——transform_fingerprint `0x774f906ff0c110eb`、manifest_hash `0xa8bab23f45af6275`（= manifest_check.log canonical_hash；R3.1 将 `license_receipt_sha256` 纳入清单规范哈希输入，故与 R3 轮的 `0x702aeabc60f713fd` 不同），两方双跑四处一致
- CFSAVE02 未变化：`git diff r3-baseline-355344c10846 -- src/persistence.rs` 输出 0 行（基线→S2 零改动）

## R3.1 D01–D12

| ID | 结论 | Reviewer 独立依据 |
|---|---|---|
| D01 | PASS | Reviewer 全新克隆自主取得：acquisition_receipt 记录 `source_kind=private_cache`、`user_interaction=none`、`acquired_at=2026-09-14T09:26:32Z`，无人工下载/转交 |
| D02 | PASS | 原包双口径重算 sha256 `ababc51f…fed`、1913239 字节，与受控缓存、脚本内置期望值及 selection 元数据一致 |
| D03 | PASS | `source_receipt.txt` 重算 `ed164156…c571`，与 selection/lock 记录一致；负向追加 1 行后 verify 以 exit 1 拒绝 |
| D04 | PASS | `inventory.tsv` 重算 `691781…220f`；python 逐行重算 24/24 文件 sha256+字节数+扩展名全对 |
| D05 | PASS | `selection.lock.tsv` 重算 `79f4c6…6da0` = asset_set_sha256；`r3_verify_local_assets.sh` 全链 PASS（inventory_files=24、selected_assets=6） |
| D06 | PASS | Reviewer 与实现方各自从独立克隆运行 acquire+prepare+验收，四链 SHA、asset_set、manifest_hash、transform_fingerprint、inventory/selection 计数全部一致 |
| D07 | PASS | 两方证据 ZIP 经 S2 修复后的 `check_vendor_assets.sh` 均通过（各 142 条目、0 个 `.glb/.gltf/.fbx/.obj/.rar`、0 条 `vendor_local/third_party_raw` 路径）；证据内同层并列嵌套（`r2_1.zip` 与 `r2_1/r1.zip`）被正确放行；改名贴图 ZIP 与嵌套 ZIP 负向均被拒，白噪声 PNG 对照通过 |
| D08 | PASS | lab_a/lab_b `app_stdout.log` 中 B0004/panic/ERROR/FATAL 计数均为 0 |
| D09 | PASS | lab_a/lab_b `asset_lifecycle.csv`：baseline、cycle_1–cycle_5、final 全部为 12,9,16,417，5 轮延迟 despawn 后回落基线 |
| D10 | PASS | manifest_hash `0xa8bab23f45af6275`、transform_fingerprint `0x774f906ff0c110eb`：Reviewer 双跑一致且与实现方一致；双方 10 张同名截图逐字节全同（R3_01 sha256 `f4d548b1e6e1b527…` 等） |
| D11 | PASS | 内嵌 R1 回归 A01–A13 全 PASS、R2.1 回归 B01–B12 全 PASS；`src/persistence.rs` 基线→S2 零改动 |
| D12 | PASS | 双方证据 ZIP 结构完整（report.json/report.md、lab_a/lab_b、manifest_check.log、r2_1/、source_chain/、SHA256SUMS.txt）；python 逐条重算 ZIP 内部 SHA256SUMS 各 139 条全部通过；泄漏扫描通过 |

## 本轮修复专项验证（S2 相对作废提交的差异）

- S2 变更范围核查：`git show --stat` 仅 3 个文件——`scripts/check_vendor_assets.sh`（scan_dir 改用局部变量）、`scripts/test_r3_source_chain.sh`（新增回归用例）、`docs/DECISIONS_R3.md`（决策记录 R3.1-11），与提交说明一致，无越权改动
- `local` 声明确认：已直接阅读 `scripts/check_vendor_assets.sh` 第 106 行，`scan_dir` 内 `root`/`depth` 为 `local` 局部变量
- 回归用例：`bash scripts/test_r3_source_chain.sh` 在 Reviewer 克隆内 PASS（含新用例：合法三层嵌套 + 同层并列 ZIP 必须通过）
- 反证实验：Reviewer 在 `/tmp` 临时副本中将该行还原为全局变量赋值，同一合法三层嵌套+同层并列结构立即被误报「嵌套 ZIP 深度超过 3」exit 1——证明旧 bug 真实可复现、S2 修复有效、回归用例确实覆盖该 bug
- 触发场景实证：本轮双方证据 ZIP 均含同层并列嵌套结构（顶层 `r2_1.zip` 与解包后同层 `r2_1/r1.zip`），S2 修复后的守卫对两方 ZIP 均正确放行（旧代码下此结构会被误杀）

## 必查实验

- 修改选中 GLB 1 字节后来源链失败：是（`/tmp/neg_assets` 副本中 `tent.glb` 中部翻转 1 字节，第三参数传副本根，`r3_verify_local_assets.sh` exit 1「inventory 文件内容不匹配：…tent.glb」）
- 修改 source_receipt 后来源链失败：是（副本中追加 1 行，manifest/lock 均改指副本，exit 1「manifest/license-receipt SHA-256 不一致」expected `ed164156…` actual `f0eb07bf…`）
- 把第三方 PNG 改名放入 ZIP 后泄漏守卫失败：是（`tent.png` 字节改名 `screenshot.png` 打入 ZIP，exit 1「与第三方模型/贴图/原包字节完全一致」）
- 嵌套 ZIP 泄漏失败：是（外层含 `inner.zip`、内含改名 `presentation.png`，exit 1 解嵌套后命中内容哈希）
- 对照：自造白噪声 PNG 独立 ZIP 通过（exit 0），证明守卫按内容哈希判定而非一刀切禁 PNG
- 防护语义：`R3_SOURCE_CHAIN_FIXTURE=1 bash scripts/r3_acceptance.sh /tmp/x` 立即 exit 1「正式 R3.1 验收禁止 R3_SOURCE_CHAIN_FIXTURE」；`R3_ASSET_CACHE_FILE=/nonexistent.rar R3_ASSET_DOWNLOAD_URL= bash scripts/r3_acquire_sample.sh /tmp/y.rar` exit 78「缺少 R3 私有素材缓存…不得要求用户手工下载/转交文件」
- 两次 lab 日志 B0004 命中 0：是（lab_a/lab_b 均为 0，panic/ERROR/FATAL 同为 0）
- cycle_1..cycle_5 每轮都回到基线：是（两 lab 的 5 个 cycle 行均为 12,9,16,417）
- 实现方与 Reviewer manifest / transform / asset-set 三指纹一致：是（`0xa8bab23f45af6275` / `0x774f906ff0c110eb` / `79f4c6…6da0`）

## 截图独立统计（PIL 复核）

- lab_a/lab_b 各 5 张共 10 张 PNG：全部 1280×720 RGBA；160×90 缩略后独立颜色数 853–1232（≥100，场景非空）
- 双跑同名截图统计逐张相同，且与实现方证据内同名截图逐字节全同（550798/603298/613605/716695/575872 字节），确定性渲染成立
- 目检 R3_01（lab_a）与 R3_04（lab_b）：体素场景正常渲染——体素帐篷/树木/篝火/房屋等约 5–6 个模型可见，无黑屏、破面或模型缺失；R1 巡航截图 10 张亦全部 1280×720

## Release

- 标签 `r3.1-baseline-<S短SHA>` 解引用到 S：尚未创建（`git ls-remote --tags origin` 确认远端当前无任何 `r3.1-baseline-*` 标签——前一轮针对 `562c2dfdc8b4` 的无效标签与 Release 已按决策记录 R3.1-11 撤销；发布以本 Reviewer 报告为前置输入，在复核 PASS 后打标发布）
- 历史 `r3-baseline-355344c10846` 未移动：是（本地与远程均解引用 `355344c10846f5ca63cac61a7fc9bbc3ba94f077`；r1/r1.1/r2/r2.1 各基线标签亦完好）
- Release 仅含实现方证据、Reviewer 证据与 SHA256SUMS：Release 尚未发布，待发布后由 `verify_r3_1_release.sh` 强制校验
- `verify_r3_1_release.sh` 从空目录下载复验通过：待发布后执行（发布时序为 Reviewer PASS → publish → 空目录复验）

## 结论

- D01–D12 全部 PASS；四链 SHA、asset_set_sha256、manifest_hash、transform_fingerprint、inventory/selection 计数与实现方证据逐项一致
- 负向复验 6 项全部符合预期；本轮修复（scan_dir 局部变量声明 + 回归用例）经正向、反证双方向验证有效
- **S2 = `bbcfdb8b8bcd4eda845a7f002accecd0289e1ac3` 复核结论：PASS，可进入发布流程**
