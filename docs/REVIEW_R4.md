# R4 独立 Reviewer 复核报告（S2 守卫修复轮最终版）

- 代码验收提交 S2：`1c05cfafce2e9dabf46eb5853cd58ece1bf0bbdc`（分支 `r4-guard-dir-depth` 头，PR #7）
- 父提交（S）：`20680a1aa96f4c5005b29330373021bc728d5101`；基线：`r3.1-baseline-bbcfdb8b8bcd`（S 的父提交核对一致）
- S..S2 变更面核对：仅 3 个文件（`scripts/check_vendor_assets.sh` +8/-1、`scripts/test_r3_source_chain.sh` +44、`docs/DECISIONS_R4.md` +6，即 R4-10 决策）；`src/` 零改动、`src/persistence.rs` 零改动、CFSAVE02 未触碰
- 实现方 S2 证据 / SHA-256：`evidence_impl_1c05cfafce2e_20260915_051009.zip` / 公布值 `e66988cea614aa6697e5f7b7f959cef4a2449b0dd939ce0029f7d89949b92634`（Reviewer 未持有该文件，未能独立复算；其公布运行值已比对，见下）
- Reviewer S2 证据 / SHA-256：`evidence_review_1c05cfafce2e_20260915_055114.zip` / `517cb27e8455c47559559b3d7471a127ad3e94b1967aa1ded92946f9a00bfab2`
- Reviewer S 轮证据 / SHA-256（历史留档）：`evidence_review_20680a1aa96f_20260915_032758.zip` / `d9d4815760b5487335219460f713698e6c2a5cf810fa3cf9d7fa50181dd76a01`
- 结论：`PASS`

## 隔离性

- 全新克隆（S 轮）：`git clone` 后 `git checkout 20680a1aa96f…`；S2 轮：`git fetch origin` 后在 `r4-s2-review` 分支检出 S2。两轮 HEAD 分别==S 与==S2，父提交核对一致；S2 轮验收全程 HEAD 保持 S2（commit.txt==S2 已核），报告提交置于验收结束之后
- 工作区干净：`git status --porcelain` 为空（每轮验收前复查）
- Bevy：`0.19.1`（`Cargo.toml` 锁定 `=0.19.1`，`audit_bevy.sh` 通过）
- 私有素材缓存自主取得：受控私有缓存 locator 的 RAR SHA-256 核对一致（`ababc51f…e82fed`），验收日志记录素材自主取得完成、selection profile 应用与 inventory/字节锁生成；未复制实现方任何已解包素材
- `src/persistence.rs` / CFSAVE02 未变化：`git diff bbcfdb8b8bcd..S` 与 `git diff S..S2` 对该文件均为空 diff
- 用户交互：`none`
- 环境说明与多轮失败根因（如实记录）：S 轮前四轮完整验收均在 R3.1 回归的 R1 巡航 A13（`>200ms 帧 <= 2`）失败，长帧 3/12/10/16、中位 FPS 恒 231.7–235.8、p95 恒 ~17ms。主控方实验确诊根因：夜间桌面空闲使 GPU 停留深度空闲低时钟档（约 210MHz），验收场景负载太轻无法触发升频，p95 由历史热态的 ~4.5ms 恶化到 ~17ms，时钟切换间隙产生孤立 >200ms 长帧；700 秒对照探针证实 GPU 热负荷下 A02–A13 全部实际通过。主控方随后以显示器常亮、鼠标微动及一个常驻于实现克隆的游戏窗口（屏幕左带 640×620，与验收窗口零重叠）提供约 30% GPU 负荷稳定时钟——这是恢复历史基线测量时的硬件热态条件，不触碰被测应用、阈值与证据。S 轮第五轮在此环境下通过但因当时克隆 HEAD 已含 Reviewer 报告提交而偏离 S（commit.txt 不符，作废；其视觉对象哈希与实现方一致，为旁证）；第六轮在 HEAD 重置回 S 后通过。S2 轮一次通过，A13=123971 帧/中位 FPS 241.2/p95 4.81ms/0 长帧/0 相邻对，与历史热态通过轮吻合。
- S2 轮验收退出码：`0`（一次通过）

## S2 修复背景（R4-10）

S 首次发布时 `verify_r4_release.sh`（下载目录扫描）对实现方证据 zip 误报「嵌套 ZIP 深度超过 3」并自动回滚。根因是泄漏守卫深度记账不一致：zip 文件目标的内容从 depth 0 扫描（zip 直扫四层包裹链合法），目录入口却从 depth 0 起（目录内的 zip 被多记一层幻影层）——同一字节两种判定。R4 证据天然四层包裹（r4.zip ⊃ r3_1.zip ⊃ r2_1.zip ⊃ r1.zip）。S2 修正：目录目标以 depth -1 进入（目录不贡献嵌套层），两入口统一「包裹层数=发现深度+1，发现深度 3 即拒绝」，拒绝边界不变（五层包裹两入口同拒）。本 Reviewer 独立复核：

- `test_r3_source_chain.sh`（含新增四层合法目录回归 + 五层包裹双向拒绝）：exit 0 通过
- Reviewer 反证（不改仓库文件）：将仓库守卫复制到 /tmp 临时目录树（假仓库根 + git init + .gitignore），把 `scan_dir "$target" -1` 临时改回旧语义 `0`，用 python 构造四层包裹链夹具（evidence ⊃ level3 ⊃ level2 ⊃ level1 ⊃ note）：旧语义副本扫目录 exit 1，报错精确复现首次发布症状（`失败：嵌套 ZIP 深度超过 3：…level1.zip`）；S2 守卫对同一目录 exit 0、对同一 zip 直扫 exit 0，两入口判定一致。回归用例真能抓到旧 bug 得证
- 发布前演练（无需真发布）：把 Reviewer 的 S2 证据 zip 放入临时目录，用仓库守卫扫描该目录（模拟 verify 下载目录），exit 0 通过——即首次发布误拒的目标场景

## 回归

- A01–A14：A01（干净构建 fmt/clippy/test/Release）PASS；A02–A12 全 PASS（世界哈希 `0x6714e0ea62871642`，与实现方一致）；A13 PASS（123971 帧、中位 FPS 241.2、p95 4.81ms、0 长帧、0 相邻对）；A14 为本报告
- B01–B12：R2.1 子回归随 R3.1 入口 PASS
- C01–C12：R3 子回归随 R3.1 入口 PASS
- D01–D12：R3.1 全回归 PASS（`r3_1/report.json` overall=PASS，含素材自主取得、manifest/字节锁、source chain 与泄漏守卫）
- 静态检查全部通过：`bash -n`、`cargo fmt --check`、`cargo clippy --all-targets -D warnings`、`cargo test --release`（86 passed / 0 failed）、`cargo build --release`、`audit_bevy.sh`、`check_naming.sh`、`test_r3_source_chain.sh`
- SHA256SUMS：`sha256sum -c` 自仓库根 168 个文件全 OK

## R4 E01–E12（S2 有效轮，全部 PASS）

| ID | 结论 | Reviewer 独立依据 |
|---|---|---|
| E01 | PASS | headless report E01=PASS（稳定单调 ID 1/2/3）；源码断言逐行核读 |
| E02 | PASS | headless E02 两条 PASS（MissingSupport/ObjectOverlap）；四类放置负例单测齐备 |
| E03 | PASS | headless E03=PASS（旋转后 footprint 3×4）；奇数四分数交换宽深 |
| E04 | PASS | headless E04=PASS；`validate_terrain_edit` 拒绝移除支撑与向体积放 Stone |
| E05 | PASS | headless E05=PASS（移动/删除后 ID 不变） |
| E06 | PASS | headless E06=PASS（倒序重建语义哈希一致）；哈希顺序无关 |
| E07 | PASS | headless E07=PASS（CFOBJ001 区分 CFSAVE02）；strict decoder 全项校验 + 4 类字节级篡改单测 |
| E08 | PASS | headless E08=PASS（地形+对象精确往返） |
| E09 | PASS | headless E09=PASS（最新槽字节损坏回退） |
| E10 | PASS | headless E10=PASS（两次预写保护 + 提交后生效）；xxd 独立解析配对（见必查实验） |
| E11 | PASS | headless E11=PASS（terrain-only 返回 None） |
| E12 | PASS | 视觉 lab_a/lab_b 双跑 PASS、object_hash 一致（`0x2d9c80c51bb5bc53`），4 张截图双跑逐字节一致 |

截图双跑 SHA-256（lab_a==lab_b，且与 S 轮逐字节相同，确定性渲染）：
`R4_01_object_lineup.png`=`e815c3b679213cfe3a6aaef7605dcc7698dc6be69fb143502355afe70ff484b0`；
`R4_02_rotated_footprints.png`=`143bda56df06a77b17128f4f0d5934757e73e1b091357d02adf568146faf48b6`；
`R4_03_management_view.png`=`c846a32135671b7fe5eeeb34a796b8d736086746fd93245e091d9566754212ed`；
`R4_04_voxel_context.png`=`ec21129ca5899963d57319390e4cc1be0ae09b5c7c66940e94d5a9ad36bd4e12`。

与实现方关键值比对：
- 视觉双跑 object_hash：Reviewer `0x2d9c80c51bb5bc53` == 实现方（S 轮与 S2 轮均一致）
- headless object_hash：Reviewer `0x30ba3f3db511bec2`（object_count=2）== 实现方（一致）
- 实现方 S2 zip SHA-256 公布值 `e66988cea614aa6697e5f7b7f959cef4a2449b0dd939ce0029f7d89949b92634`：Reviewer 未持有该文件，未能独立复算

## 必查实验

- 相同对象集合不同插入顺序，语义哈希一致：PASS（headless E06 运行时 + 实现核读）
- 缺支撑、地形相交、对象重叠、越界和顶层占用均拒绝：PASS（headless E02 运行时 + `objects.rs` 四负例单测，含 R4-09 新增 `placement_rejects_out_of_bounds_top_layer_and_terrain`）
- 移动失败不改变旧对象：实现级确认（`move_object` 失败路径回滚旧索引；无显式负例单测，如实记录）
- 移除支撑／向对象体积放置 Stone 均拒绝：PASS（headless E04 运行时 + 单测）
- 对象保存加载后 ID、类型、锚点、旋转和 next_id 完全一致：PASS（headless E08 运行时 + `materialize` 五重校验核读）
- 最新对象槽损坏后回退：PASS（headless E09 运行时 + 字节级篡改单测）
- 对象预写两次但地形未提交，旧地形仍加载旧对象槽：PASS（headless E10 运行时 + 保护槽字节不变单测）
- 地形提交后新对象槽生效：PASS（headless E10 后半运行时 + 单测）
- terrain-only 历史存档加载为空对象层：PASS（headless E11 运行时 + 单测）
- 两次视觉 lab 的对象哈希与截图 SHA 一致：PASS（双跑 hash 相等、4 截图逐字节相等且跨轮相同）
- 证据与 Release 中第三方模型／贴图／原包字节为 0：PASS（守卫对证据目录与 zip 通过；S 轮 Reviewer 改名贴图反证：`bonefire.png`→`tex_renamed.dat` 置于非禁止路径，守卫以内容哈希拦截 exit 1，未篡改对照 exit 0；S2 守卫仅改深度记账，内容哈希逻辑未触碰）

存档槽字节级独立解析（S 轮 headless 证据，xxd）：`paired.cfsv.slot0`（CFSAVE02，gen1）world_semantic_hash=`0x292280fba62d7682` 与 `paired.cfsv.objects.slot0`（CFOBJ001，gen1，72 字节头）terrain_semantic_hash 逐字节一致；`paired.cfsv.slot1`（gen2）world_semantic_hash=`0xec2e54a08e5194c1` 与 `paired.cfsv.objects.slot1`（gen3，两次预写）terrain_semantic_hash 一致；两配对地形哈希不同、object_semantic_hash 不同；对象记录（48 字节）锚点 (6,21,6)→(8,21,6)、yaw 0→1、保留字节 0。配对关系独立确认（S2 未改 `src/`，该层结论直接传承）。

另记（记录性观察，不构成失败）：严格解码的长度上限（`MAX_SAVE_BYTES`/`MAX_OBJECT_COUNT`）在编码与读取路径均有实现检查，但无直接构造超限文件的负例单测。

## Release

- 首次发布（S）：`verify_r4_release.sh` 下载目录扫描误拒（守卫目录入口深度记账缺陷），自动回滚清理标签与 Release——S2 修复动因
- S2 发布就绪：守卫四层包裹链目录扫描与 zip 直扫判定一致（Reviewer 反证 + 发布前演练均通过），五层包裹双向拒绝，边界未放宽
- 标签与 Release 的最终发布由主控方在两份 S2 复核报告（实现方 S2 验收 + 本报告）齐备后执行，不在本 Reviewer 验收范围
