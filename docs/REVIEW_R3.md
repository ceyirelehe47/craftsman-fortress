# R3 独立复核报告

- 代码验收提交 S：`355344c10846f5ca63cac61a7fc9bbc3ba94f077`
- 基线：`r2.1-baseline-804df0d8513a`
- 实现方证据 / SHA-256：`D:/code/game/evidence_impl_355344c10846_20260914_111239`（ZIP `evidence_impl_355344c10846_20260914_111239.zip`，SHA-256 `ee2c4716aace9b5fb3a5af2fac2db1a93c1b40c7f1f26b264896e35727e8a588`）
- Reviewer 证据 / SHA-256：`D:/code/game_r3_review/evidence_review_355344c10846_20260914_113918`（ZIP `evidence_review_355344c10846_20260914_113918.zip`，SHA-256 `e118147c788ccec04176377fef34eb1e5f1fdaaefff2f035989e0d73177c53d0`）
- 结论：`PASS`

复核环境：Windows 11 Pro（内核 26200），AMD Ryzen 7 6800H，NVIDIA GeForce RTX 3060 Laptop GPU（驱动 581.15，Vulkan 后端），rustc/cargo 1.98.1，16 逻辑核。复核时间 2026-09-14。

## 隔离性与素材来源

- 全新克隆：`git clone --no-local D:/code/game D:/code/game_r3_review`，检出 S 后 `git rev-parse HEAD` = `355344c10846f5ca63cac61a7fc9bbc3ba94f077`，全程未接触实现方工作区。
- 干净工作区：`git status --porcelain` 输出为空；基线到 S 仅一个提交（`R3: add local GLB asset pipeline and scale validation`）。
- Bevy：`0.19.1`（依赖树审计仅出现 0.19.1，验收脚本三处审计均通过）。
- 原包名称与 SHA-256：`Free_Sample.rar`（PalmStudio Voxel Survival Pack 免费样本），Reviewer 独立拷贝后重算 SHA-256 = `ababc51f543ec06d07e68d95cdcc90d8ae878d6d12908dd90a0746a836e82fed`，1,913,239 字节，与实现方声明一致。
- 来源页面与取得日期：https://palmstudio.itch.io/voxel-survival-pack ，实现方取得于 2026-09-14T03:02:52Z（清单头记录）。
- 授权收据 SHA-256：`afebef41860a5f8648de003d0295fd7d1562e037db956f1706f7b4a94f5d2eec`（实现方清单头）；授权允许个人/商业使用与修改，禁止再分发。
- Git/Release 中第三方模型文件数量（必须 0）：`git ls-files | grep -Ei '\.(glb|gltf|fbx|obj|rar)$'` 在 S 检出下命中 0；实现方 ZIP 与 Reviewer ZIP（各 137 条目）经 `unzip -Z1` 全条目扫描，`.glb/.gltf/.fbx/.obj/.rar` 命中均为 0。

## R1 / R2.1 回归

- A01–A14：Reviewer 运行 `scripts/r3_acceptance.sh`（内嵌完整 R1 回归巡航，总耗时 654s，应用退出码 0），`r2_1/r1/report.json` overall=PASS，A01–A13 全 PASS；A14（独立复核）即本报告，结论 PASS。
- B01–B12：`r2_1/r2/report.json` overall=PASS，B01–B12 全 PASS；roundtrip 报告 overall=PASS（seed=424242，base_hash=`0x72629ae6a816d059`，语义/校验和/基线哈希三级回退与旧有效槽保留均 PASS）；损坏存档启动拒绝退出码 3；`r2_1/report.json` 聚合 PASS。
- CFSAVE02 magic / format / generator revision 未变化：`git diff r2.1-baseline-804df0d8513a S --stat -- src/persistence.rs` 无任何改动（`MAGIC = *b"CFSAVE02"` 所在文件零变更），R2.1 全量回归同时通过。

## R3 C01–C12

Reviewer 独立流程：克隆 → 拷贝原包并重算哈希 → `scripts/r3_prepare_sample.sh` → 用实现方 `manifest_snapshot.tsv` 覆盖占位清单（6 项：axe_stone=tool/0.80、bonefire=prop/0.90、grass_03=environment/0.40、stone_03=environment/0.40、tent=building/2.40、tree_11=environment/4.50，yaw 全 0）→ `cargo run --release --example r3_manifest_check` → `scripts/r3_acceptance.sh`（退出码 0，overall=PASS）→ Python 复算与逐张目检。以下"复算"均为 Reviewer 用 Python/PIL 从原始 JSON/PNG 重新计算，非转抄应用判定。

| ID | 结论 | Reviewer 独立依据 |
|---|---|---|
| C01 | PASS | 克隆内独立解包生成 `source_receipt.txt`（source_url=palmstudio.itch.io/voxel-survival-pack，license_redistribution=prohibited…）；验收内置边界检查通过；两份证据 ZIP 共 274 个条目扫描 `.glb/.gltf/.fbx/.obj/.rar` 命中 0。 |
| C02 | PASS | 清单 6 项 id 稳定（axe_stone/bonefire/grass_03/stone_03/tent/tree_11），路径全部位于 `vendor_local/voxel_survival_pack/v1.0/free_sample/` 下，无 `..`/绝对路径；r3_manifest_check PASS，entries=6，canonical_hash=`0x702aeabc60f713fd`，与实现方快照逐字节同源。 |
| C03 | PASS | Reviewer 两次 lab 运行 `app_stdout.log` 均记录 `queued 6 GLB files` → `all assets reached a terminal load state` → `R3 asset lab PASS`；4 份 asset_report（Reviewer lab_a/lab_b + 实现方 lab_a/lab_b）24 条资产记录 error 全为 null，每项 meshes=1、materials=1。 |
| C04 | PASS | 复算 raw 包围盒全部有限（axe [1,7,3]、bonefire [9,3,9]、grass_03 [30,1,30]、stone_03 [10,1,10]、tent [16,12,13]、tree_11 [15,26,17]，单位：体素）；归一化 scale 分别 0.114286/0.300000/0.400000/0.400000/0.200000/0.173077，四份报告完全一致。 |
| C05 | PASS | 复算全部 24 条资产记录 `|final_min[1]| = 0.000000 ≤ 0.02 m`，最低点精确落地，枢轴归一化成立。 |
| C06 | PASS | 复算 final 高度=final_max[1]−final_min[1]：0.800000/0.900000/0.400000/0.400000/2.400000/4.500000 m，与目标偏差均为 0.000000（容差 max(2%·target, 0.02) 分别为 0.02/0.02/0.02/0.02/0.048/0.09）；覆盖 tool/prop/environment/building 四类共 6 项，1 米参考杆在五张截图中可见。 |
| C07 | PASS | PIL 独立解码 5 张固定截图：全部 1280×720；亮度标准差 38.94/27.29/39.37/34.22/37.87；16 级量化颜色数 144/144/124/101/127（≥64）；文件 550,798–716,695 字节（>10KB）。 |
| C08 | PASS | R3_02（近景正面）与 R3_04（高空管理视角）均产出且统计达标（见 C07）；目检确认近景正面/侧面轮廓可辨、管理尺度下六模型仍可辨认（见"必查项目"）。 |
| C09 | PASS | 复算 `asset_lifecycle.csv`（lab_a/lab_b 相同）：baseline 与 final 均为 meshes=12、materials=9、images=16、entities=417；cycle_1–cycle_5 期间 entities=507、mesh/material/image 计数不变，5 轮 spawn/despawn 后全部回落基线，无泄漏。 |
| C10 | PASS | 四份 asset_report 的 manifest_hash 与 transform_fingerprint 四处一致（`0x702aeabc60f713fd` / `0x774f906ff0c110eb`）；Reviewer 与实现方（不同时间两次独立运行）5 张同名截图 sha256 逐字节全同（如 R3_01=`f4d548b1e6e1b527…`），确定性成立。 |
| C11 | PASS | Reviewer 运行内嵌 R1/R2.1 全量回归均 PASS（A01–A13、B01–B12）；`src/persistence.rs` 基线→S 零改动，CFSAVE02 未变化（见上节）。 |
| C12 | PASS | Reviewer 证据目录含 report.json/report.md、lab_a/lab_b（asset_report.json、asset_lifecycle.csv、app_stdout.log、manifest_snapshot.tsv、5 张截图）、manifest_check.log、r2_1/、SHA256SUMS.txt；ZIP 内部 SHA256SUMS 134 条目逐一校验全 OK；实现方 ZIP 同样 134 条目全 OK。 |

## 必查项目

- 所有清单 GLB 均成功加载：6/6 到达终态、无错误（两次 Reviewer lab + 两次实现方 lab 一致）。
- 原始/最终包围盒有限且高度正确：raw 尺寸全部有限（见 C04）；final 高度 0.80/0.90/0.40/0.40/2.40/4.50 m，偏差 0.000000（见 C06）。
- 最低 Y 落地误差 ≤0.02 m：全部 `|final_min[1]|=0.000000`（见 C05）。
- 五张固定截图逐张目检（Reviewer 侧 R3_01–R3_05，实现方侧同名 5 张与其逐字节一致，结论继承）：
  - R3_01 尺度队列：橙红色 1 米参考杆清晰可见（左侧）；六模型齐全——石斧（棕柄+深灰斧刃）、篝火（柴堆+橙红火焰）、草地砖（绿色草皮）、石头群（灰色聚集）、帐篷（红白 A 形）、树（棕干+深绿锥形冠）；高度关系树(4.5m)>帐篷(2.4m)>篝火(0.9m)>石斧(0.8m)，草/石贴地 0.4m；无纯黑/纯白/缺贴图。
  - R3_02 近景正面：参考杆可见；正面轮廓清晰可辨（帐篷三角入口、树冠层叠、篝火柴堆+火焰、石斧平放于地、石堆聚集、草地铺展）；渲染正常。
  - R3_03 侧面轮廓：参考杆可见；帐篷楔形侧翼、树圆锥层叠、篝火山字形柴堆、斧细长侧影均可辨；渲染正常。
  - R3_04 管理视角：中高空俯视，六模型仍逐可辨认，树>帐篷>小件的比例关系合理，画面可读。
  - R3_05 Chunk 网格语境：模型立于体素方块地形上，地形有台阶起伏、远山与水域，模型与方块尺度协调，Chunk 语境存在。
- 两次运行 manifest hash 一致：`0x702aeabc60f713fd`（Reviewer lab_a/lab_b、实现方 lab_a/lab_b 四处一致）。
- 两次运行 Transform 指纹一致：`0x774f906ff0c110eb`（同上四处一致）。
- 生命周期资源计数回到基线：meshes 12、materials 9、images 16、entities 417（cycle 期间 417→507→417 回落）。
- 证据 ZIP 不含 `.glb/.gltf/.fbx/.obj/.rar`：两 ZIP 各 137 条目扫描命中 0。

## Release

- 标签 `r3-baseline-<S短SHA>` 解引用到 S：由实现方在合并后执行，Reviewer 已核对证据 ZIP 与 SHA-256。
- Release 只含两套证据 ZIP 与校验清单，不含第三方模型：由实现方在合并后执行，Reviewer 已核对两套证据 ZIP（`ee2c4716…` 与 `e118147c…`）与其内部校验清单均不含第三方模型原件/贴图。

## 其他观察（不影响结论）

- lab 运行日志存在 Bevy `B0004` 层级 WARN（spawn/despawn 期间父实体缺组件提示），为非阻断警告：验收门槛仅拦截 panic/ERROR/FATAL，且资源计数证明生命周期无泄漏；建议后续迭代补齐父实体组件以消除该警告。
- r3_prepare_sample 生成的本地占位 `license_receipt_sha256`（f6567595…）与实现方清单头的收据哈希（afebef41…）不同属预期：收据文件含各自生成时间戳，且该头注释不参与 canonical_hash 计算（两份清单 canonical_hash 仍一致为 0x702aeabc60f713fd）。
