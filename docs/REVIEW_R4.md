# R4 独立 Reviewer 复核报告

- 代码验收提交 S：`20680a1aa96f4c5005b29330373021bc728d5101`
- 基线：`r3.1-baseline-bbcfdb8b8bcd`（父提交核对一致：`bbcfdb8b8bcd4eda845a7f002accecd0289e1ac3`）
- 实现方证据 / SHA-256：`evidence_impl_20680a1aa96f_20260915_001349.zip` / 公布值 `76caac417b9d308fa90ce6b45b406bbcb8d3241ecfc0eae6a8d7908a675c1a4e`（Reviewer 未获得该文件，未能独立复算；其公布的关键运行值已比对，见下）
- Reviewer 证据 / SHA-256：`evidence_review_20680a1aa96f_20260915_032758.zip` / `d9d4815760b5487335219460f713698e6c2a5cf810fa3cf9d7fa50181dd76a01`（两套 zip 为各自独立运行产物，不要求逐字节一致）
- 结论：`PASS`

## 隔离性

- 全新克隆：`git clone` 至独立目录后 `git checkout 20680a1aa96f…`，HEAD==S，`git rev-parse HEAD^`==`bbcfdb8b8bcd…`（R3.1 冻结基线）；验收运行时 HEAD 恒为 S（有效轮 commit.txt==S 已核）
- 工作区干净：`git status --porcelain` 为空（每轮验收前复查）
- Bevy：`0.19.1`（`Cargo.toml` 锁定 `=0.19.1`，`audit_bevy.sh` 通过）
- 私有素材缓存自主取得：受控私有缓存 locator 的 RAR SHA-256 核对一致（`ababc51f…e82fed`），验收日志记录「R3.1 素材自主取得完成」，selection profile 应用 6 项、inventory 与字节锁生成；未复制实现方任何已解包素材
- `src/persistence.rs` / CFSAVE02 未变化：`git diff bbcfdb8b8bcd..HEAD --stat -- src/persistence.rs` 输出为空
- 用户交互：`none`
- 环境说明与四轮失败根因（如实记录）：首轮四轮完整验收均在 R3.1 回归的 R1 巡航 A13（`>200ms 帧 <= 2`）失败，长帧 3/12/10/16、中位 FPS 恒 231.7–235.8、p95 恒 ~17ms。主控方实验确诊根因：夜间桌面空闲使 GPU 停留深度空闲低时钟档（约 210MHz），验收场景负载太轻无法触发升频，p95 由历史热态的 ~4.5ms 恶化到 ~17ms，时钟切换间隙产生孤立 >200ms 长帧；700 秒对照探针证实 GPU 热负荷下 A02–A13 全部实际通过（A13=124005 帧/中位 241.0/p95 4.66ms/0 长帧，与历史通过轮逐位吻合）。主控方随后以显示器常亮、鼠标微动及一个常驻于实现克隆的游戏窗口（屏幕左带 640×620，与验收窗口零重叠）提供约 30% GPU 负荷，把时钟稳定在高性能档——这是恢复历史基线测量时的硬件热态条件，不触碰被测应用、阈值与证据。第五轮在此环境下通过，但因 Reviewer 此前在克隆内提交过 FAIL 版报告使 HEAD 偏离 S（该轮 commit.txt=241695c，不合规，作废；该轮视觉对象哈希 `0x2d9c80c51bb5bc53` 已与实现方一致，为环境恢复旁证）。Reviewer 随后把 HEAD 重置回 S（FAIL 版报告保留于本地分支 `review-fail-backup`）重跑第六轮，全程通过，即本报告依据。第六轮 A13=124031 帧/中位 241.3/p95 4.71ms/0 长帧/0 相邻对，与历史热态通过轮吻合。
- 有效轮验收退出码：`0`

## 回归

- A01–A14：A01（干净构建 fmt/clippy/test/Release）PASS；A02–A12 全 PASS（世界哈希 `0x6714e0ea62871642`，与实现方 R1 轮一致）；A13 PASS（124031 帧、中位 FPS 241.3、p95 4.71ms、0 长帧、0 相邻对）；A14 为本报告
- B01–B12：R2.1 子回归 PASS（`r3_1/r2_1` 静态与运行时随 R3.1 入口通过）
- C01–C12：R3 子回归随 R3.1 入口通过
- D01–D12：R3.1 全回归 PASS（`r3_1/report.json` overall=PASS，含素材自主取得、manifest/字节锁、source chain 与泄漏守卫）
- 静态检查全部通过：`bash -n`、`cargo fmt --check`、`cargo clippy --all-targets -D warnings`、`cargo test --release`（86 passed / 0 failed）、`cargo build --release`、`audit_bevy.sh`、`check_naming.sh`、`test_r3_source_chain.sh`
- `src/persistence.rs` / CFSAVE02 未变化：空 diff

## R4 E01–E12

| ID | 结论 | Reviewer 独立依据 |
|---|---|---|
| E01 | PASS | 有效轮 headless report.json E01=PASS（稳定单调 ID 1/2/3）；源码断言 `campfire==1 && tent==2 && stone==3` 已逐行核读 |
| E02 | PASS | headless E02 两条 PASS（缺支撑 MissingSupport、重叠 ObjectOverlap）；单测含越界/顶层 H-1/净空实体负例（`placement_rejects_out_of_bounds_top_layer_and_terrain`） |
| E03 | PASS | headless E03=PASS（旋转后 footprint 3×4）；`rotated_footprint` 奇数四分数交换宽深 |
| E04 | PASS | headless E04=PASS；`validate_terrain_edit` 拒绝移除支撑与向体积放 Stone，单测 `terrain_edits_cannot_break_object_volume_or_support` |
| E05 | PASS | headless E05=PASS（移动后锚点更新、删除后 `get` 为 None、ID 不变） |
| E06 | PASS | headless E06=PASS（记录倒序 `from_persisted` 后语义哈希一致）；`semantic_hash_records` 顺序无关 |
| E07 | PASS | headless E07=PASS（对象文件以 `CFOBJ001` 开头且非 `CFSAVE02`）；`decode_bytes` 严格校验 magic/版本/校验和/长度/ID 严格递增/保留字节/未知类型/yaw≤3/语义哈希重算，4 类字节级篡改单测真实 |
| E08 | PASS | headless E08=PASS（地形+对象保存后 `load_latest` 语义哈希一致） |
| E09 | PASS | headless E09=PASS（最新槽中点字节翻转后回退旧槽，哈希一致）；单测 `roundtrip_and_checksum_fallback` 字节级篡改同项 |
| E10 | PASS | headless E10=PASS（两次预写后旧耐久地形仍加载旧对象哈希、地形提交后新槽生效）；单测断言保护槽字节逐字节未变；Reviewer 另以 xxd 独立解析配对（见"必查实验"） |
| E11 | PASS | headless E11=PASS（无伴随对象槽时 `load_latest` 返回 None）；单测 `older_terrain_without_object_companion_loads_empty` |
| E12 | PASS | 视觉 lab_a/lab_b 双跑 overall=PASS，object_hash 一致（`0x2d9c80c51bb5bc53`），4 张截图双跑逐字节一致（SHA256 见下）；实际 GLB 映射（bonefire/tent/stone_03）与对象原型一致 |

截图双跑 SHA-256（lab_a==lab_b）：
`R4_01_object_lineup.png` = `e815c3b679213cfe3a6aaef7605dcc7698dc6be69fb143502355afe70ff484b0`；
`R4_02_rotated_footprints.png` = `143bda56df06a77b17128f4f0d5934757e73e1b091357d02adf568146faf48b6`；
`R4_03_management_view.png` = `c846a32135671b7fe5eeeb34a796b8d736086746fd93245e091d9566754212ed`；
`R4_04_voxel_context.png` = `ec21129ca5899963d57319390e4cc1be0ae09b5c7c66940e94d5a9ad36bd4e12`。

与实现方关键值比对：
- 视觉双跑 object_hash：Reviewer `0x2d9c80c51bb5bc53` == 实现方 `0x2d9c80c51bb5bc53`（一致）
- headless object_hash：Reviewer `0x30ba3f3db511bec2`、object_count=2 == 实现方 `0x30ba3f3db511bec2`（2 个对象）（一致）
- 实现方 zip SHA-256 公布值 `76caac417b9d308fa90ce6b45b406bbcb8d3241ecfc0eae6a8d7908a675c1a4e`：Reviewer 未持有该文件，未能独立复算

## 必查实验

- 相同对象集合不同插入顺序，语义哈希一致：PASS（headless E06 运行时 + `semantic_hash_records` 逐记录混合实现核读）
- 缺支撑、地形相交、对象重叠、越界和顶层占用均拒绝：PASS（headless E02 运行时 + `objects.rs` 四负例单测 `matches!` 精确匹配错误变体，含 R4 要求新增的 `placement_rejects_out_of_bounds_top_layer_and_terrain`）
- 移动失败不改变旧对象：实现级确认（`move_object` 失败路径 `insert_indexes(old)` 回滚旧记录；未另设显式负例单测，如实记录）
- 移除支撑／向对象体积放置 Stone 均拒绝：PASS（headless E04 运行时 + `validate_terrain_edit` 与单测）
- 对象保存加载后 ID、类型、锚点、旋转和 next_id 完全一致：PASS（headless E08 运行时 + `materialize` 对 seed/尺寸/地形哈希/next_id/对象哈希五重校验核读）
- 最新对象槽损坏后回退：PASS（headless E09 运行时 + 单测 `bytes[HEADER_LEN+3] ^= 0x5a` 字节级篡改）
- 对象预写两次但地形未提交，旧地形仍加载旧对象槽：PASS（headless E10 运行时 + 单测断言保护槽字节未变）
- 地形提交后新对象槽生效：PASS（headless E10 后半运行时 + 单测）
- terrain-only 历史存档加载为空对象层：PASS（headless E11 运行时 + 单测）
- 两次视觉 lab 的对象哈希与截图 SHA 一致：PASS（双跑 hash 相等且 4 张截图 sha256 逐字节相等，已列上节）
- 证据与 Release 中第三方模型／贴图／原包字节为 0：PASS（守卫在有效轮对证据目录与 zip 各跑一次通过；Reviewer 另做改名贴图反证，见下）

Reviewer 泄漏守卫反证（独立实验）：将证据 zip 副本解包后，把素材包中 `bonefire.png`（SHA-256 `34cfd921…90e68`，在 inventory 黑名单内）改名为 `tex_renamed.dat` 置于证据目录顶层（路径与扩展名均不命中禁止模式，仅内容哈希可拦截），重新打包后运行 `check_vendor_assets.sh --manifest … --inventory … <篡改zip>`：退出码 1，报错明确指向该文件「与第三方模型/贴图/原包字节完全一致」；同守卫对未篡改 zip 退出码 0 通过。守卫对改名贴图有效得到反证。

存档槽字节级独立解析（xxd）：`paired.cfsv.slot0`（CFSAVE02，gen1）world_semantic_hash=`0x292280fba62d7682` 与 `paired.cfsv.objects.slot0`（CFOBJ001，gen1，72 字节头）terrain_semantic_hash 逐字节一致；`paired.cfsv.slot1`（gen2）world_semantic_hash=`0xec2e54a08e5194c1` 与 `paired.cfsv.objects.slot1`（gen3，两次预写使对象槽 generation 走到 3）terrain_semantic_hash 一致；两配对地形哈希不同、object_semantic_hash 不同。对象记录区（48 字节）：slot0 = id 1 / `campfire_basic`（NUL 填充）/ 锚点 (6,21,6) / yaw 0 / 保留字节 0；slot1 = 同 ID / 锚点 (8,21,6) / yaw 1 / 保留字节 0。配对关系独立确认。

另记：严格解码的长度上限（`MAX_SAVE_BYTES`/`MAX_OBJECT_COUNT`）在编码与读取路径均有实现检查，但无直接构造超限文件的负例单测；此项为记录性观察，不构成失败。

## Release

- 标签 `r4-baseline-<S短SHA>` 解引用到 S：未创建（Review 时点仅有 r1.1–r3.1 基线标签；Release 流程在复核之后，不在本次验收范围）
- Release 只含两套证据 ZIP 与校验清单：未执行（同上，Review 时点无 R4 Release）
