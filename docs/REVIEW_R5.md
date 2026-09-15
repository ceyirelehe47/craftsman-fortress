# R5 独立 Reviewer 复核报告

- 代码验收提交 S：`9ec9fa8d0fe2058b36ec50c01d5eafa4e79f7820`
- 基线：`r4-baseline-1c05cfafce2e`
- 实现方证据 / SHA-256：`82c811418fd54fe312ea7c6b4caf27b27aa60cf36df172c76610bfc6135fc195`
- Reviewer 证据 / SHA-256：`739ace88a647fdfe270c5d377d78fb44292ac3763e0dfb0c920a767b38bbc126`
  （`evidence_r5_review_9ec9fa8d0fe2_20260915_130118.zip`，Reviewer 独立克隆 `D:/code/game_r5_review` 产出）
- 结论：`PASS`

## 隔离性

- 全新克隆：`git clone https://github.com/ceyirelehe47/craftsman-fortress.git /d/code/game_r5_review`，
  未复用实现方工作区（`D:/code/game_r5_impl`）、其 target 缓存或任何 evidence 目录。
- 工作区干净：`git status --porcelain` 为空；验收全程 HEAD 钉在 S，
  证据 `commit.txt` 与根 `report.json` 的 `commit` 字段均等于 S。
- 冻结面 blob 核验（`git ls-tree HEAD`）：
  - `src/persistence.rs` → `a59541436c826ff11257cec7be1c4ec14cd03255` 一致；
  - `src/objects.rs` → `125a28aeba060a293be8ebb557163983314e4f1f` 一致；
  - `src/object_persistence.rs` → `08c03d6b76e91937dd697474c73a5624c663b158` 一致；
  - `Cargo.toml` → `871b23f431022e315f1b62680cacf5bad8bfdd06` 一致。
- `cargo metadata --locked --no-deps` 解析成功（exit 0），`Cargo.lock` 锁定 Bevy `0.19.1`。
- Bevy：`0.19.1`
- 私有素材缓存自主取得：通过 `R3_ASSET_CACHE_FILE=/d/code/game/third_party_raw/Free_Sample.rar`
  传入，验收脚本内部复制并 sha256 校验（缓存 `ababc51f543ec06d07e68d95cdcc90d8ae878d6d12908dd90a0746a836e82fed`），
  获取成功（`asset_set_sha256=45397ef449a2a767edd4bf6cee07308a404bc6554d5862932d46679f1c193aab`，
  `inventory_files=4`，`selected_assets=3`）。
- 用户交互：`none`（全程无用户参与；GPU 热负荷由桌面上既有主游戏进程 craftsman_fortress.exe 维持，未干预）。
- `src/persistence.rs`、`CFSAVE02`、`CFOBJ001` 兼容性：冻结文件 blob 与基线一致；
  R4 全回归（含 R3.1 私有缓存获取、R2.1 恢复、R1 巡航）在 S 上全部 PASS，
  `older_save_without_building_companion_loads_empty` 与 F11 证明旧存档加载为空建筑层。

### 验收运行状态说明（如实记录）

Reviewer 的后台 shell 在脚本最后收尾阶段被外力终止：第二次泄漏守卫（目录+ZIP）
输出 ZIP 解包 warning 之后、其成功回显与脚本末尾两行 `echo "R5 acceptance PASS"`
之前被杀，管道退出码未能落盘。判定依据：脚本为 `set -euo pipefail`，能执行到
第二次泄漏守卫即证明此前全部关卡（静态检查、R4 全回归、R5 headless、R5 visual
A/B、哈希与截图一致性、目录泄漏守卫、report.json、SHA256SUMS.txt、ZIP 打包）
均已通过；全部产物在终止前已生成且完整。被中断的 ZIP 泄漏守卫由 Reviewer 以
与 `scripts/r5_acceptance.sh` 完全相同的参数独立重跑，结果
`第三方素材授权边界检查通过` 且 exit 0。据此判定验收实质完整 PASS。

## 回归

- A01–A13（r1 验收巡航，13 项）：全 PASS（r4/r3_1/r2_1/r1/report.json，0 FAIL；
  模板所写 A14 在实际证据中不存在，R1 巡航共 13 项）。
- B01–B12（r2）：全 PASS（r4/r3_1/r2_1/r2/report.json）。
- C01–C12：模板中的 C 系列在实际证据链中的对应序列为 R3/R3.1 的 D01–D12，
  全 PASS（r4/r3_1/report.json，`r3=PASS`、`r3_1=PASS`）；证据内无名为 C 系列的检查。
- D01–D12（R3/R3.1 私有缓存获取与来源链）：全 PASS，含 `source_chain` 展开证据。
- E01–E12（r4 对象层）：全 PASS（r4/report.json `r4_headless=PASS`、`r4_visual=PASS`，
  `visual_object_hash=0x2d9c80c51bb5bc53`）。
- 全部历史层展开保留于 `evidence_.../r4/` 下，子轮 ZIP 与旧清单按 R5-11 删除，
  由 R5 根级 `SHA256SUMS.txt`（196 文件）重新覆盖，`sha256sum -c` 全部 OK。

## R5 F01–F12

| ID | 结论 | Reviewer 独立依据 |
|---|---|---|
| F01 | PASS | headless/report.json PASS；roundtrip L40-48 断言非零、自 1 起严格单调；`BuildingStore`（buildings.rs）以 `BTreeMap<BuildingId,_>` 与单调 `next_id` 分配（place L218-235），ID 与 ECS Entity 无关 |
| F02 | PASS | roundtrip L50-74：反向描述同一边得 `SlotOccupied`、`next_id` 不变、`direction(1)=Z`、薄墙视觉盒厚度 <0.2；规范化槽由 `slots_for`/`edge_endpoints` 统一 |
| F03 | PASS | roundtrip L84-104：删除梁端唯一支撑柱 (10,21,4) 整体失败，语义哈希不变、上层楼板与被删柱仍存在；`validate_records` 整体重算、失败无副作用（R5-13.5 修正后断言真实生效） |
| F04 | PASS | roundtrip L106-120：门/窗/楼梯均在场；楼梯低端高端必须精确落在楼板（`StairEndpointMissing`，buildings.rs validate_support L659-675） |
| F05 | PASS | roundtrip L122-137：穿越对象被拒 + 破坏支撑的地形编辑被拒；`validate_object_record`/`validate_terrain_edit`（edit_override 机制）与 editing.rs L89-94 双层拦截 |
| F06 | PASS | roundtrip L139-161：place/move(yaw=2)/失败移动回滚（哈希与记录逐字节不变）/remove；`move_component`/`remove` 候选快照验证后才提交（buildings.rs L237-295） |
| F07 | PASS | roundtrip L163-170：全量逆序 `from_persisted` 后语义哈希一致；`semantic_hash_records` 先按 id 排序再哈希（L416-440） |
| F08 | PASS | roundtrip L172-193：magic `CFBLD001`、排他 `CFSAVE02`/`CFOBJ001`、头部内嵌 object 语义哈希与对象收据一致；编码/解码常量（80/48/8 字节、FNV-1a）与 BUILDING_SAVE_FORMAT.md 逐项吻合 |
| F09 | PASS | roundtrip L195-212：terrain/object/building 三层语义哈希精确往返，建筑收据 object hash == 对象收据 hash |
| F10 | PASS | roundtrip L380-413 最新建筑槽字节损坏后回退到旧槽同哈希；L416-506 中断三层：两次建筑预写+对象预写后地形未提交时受保护槽字节不变、旧三层可完整重载，地形提交后新三层生效——与 editing.rs F5 顺序一一对应 |
| F11 | PASS | roundtrip L223-234 无 `.buildings.slot*` 时 `load_latest` 返回 `None`；单测 `older_save_without_building_companion_loads_empty`；存在槽但无完整匹配时 `NoValidSlot` 附详细原因（building_persistence.rs L476-481） |
| F12 | PASS | lab_a/lab_b 双跑 report 均 PASS，`building_hash=0x9282f7ed02b8eb20` 一致，四张截图 A/B sha256 逐张一致；`validate_png` 门槛与 R4 r4_object_lab.rs 逐字相同（mean∈[8,247]、stddev≥8.0、colors≥64、精确窗口尺寸、>10KB） |

## 必查实验

- 八种稳定构件类型均可出现，稳定 ID 与 ECS Entity 无关：F12 检查 `BuildingKind::ALL` 八类全部在场且 type_id 非空（roundtrip L236-244）；catalog 全场景由 `BuildingStore` 权威层驱动。
- 同一墙边正反方向描述归一化并互斥：F02 反向放置 `SlotOccupied`（roundtrip L51-57）。
- 门洞/窗与墙共享槽，楼梯低高端必须连接楼板：F04 + `slots_for` 对 Wall/Doorway/Window 同一边槽；`StairEndpointMissing` 强制两端楼板。
- 抬高楼板四角支撑，删除任一必要支撑失败且无副作用：F03（删柱后整体失败、哈希不变、构件保留）。
- 失败移动保持原记录、ID、next_id、revision 与语义哈希：F06 rollback_ok（roundtrip L148-154）。
- 地形编辑、独立对象和建筑互相阻挡正确：F05 双向验证 + editing.rs `apply_edit_action` 先过对象层与建筑层验证再改地形。
- 相同构件集合不同输入顺序，语义哈希一致：F07 逆序重建哈希相等。
- `CFBLD001` 字节级严格解码与资源上限：`decode_bytes` 拒绝清单逐条有实现——magic/版本/schema（L292-306）、截断（L269-271）、长度与记录数精确一致含尾随字节（L321-332）、校验和（L279-289）、构件数>100,000（L311-315）、文件>4,800,088 字节（L272-277、L389-393）、ID 0/重复/乱序（L337-342 严格递增 + buildings.rs L489-494）、`next_id==0 或 ≤max_id`（buildings.rs L500-502）、类型未知及 NUL 后非零/非 UTF-8（L197-211）、旋转>3（L346-351）、保留字节非零（L352-354）、seed/尺寸（L407-412）、terrain/object hash（L413-426）、物化后越界/触顶/穿地形/穿对象/槽位冲突/楼梯冲突/支撑无效（from_persisted→validate_records L479-535 + validate_terrain L680-738 + validate_support L604-678）、解码级与物化级双重 building hash 重算（L362-367、L430-436）；单测覆盖未知类型/保留字节/乱序/校验和/超限/对象哈希不匹配。
- terrain/object/building 精确往返：F09。
- 最新建筑槽损坏后回退：roundtrip L380-413（损坏槽被拒后另一槽同哈希胜出，generation 择优见 load_latest L454-481）。
- 建筑预写、对象预写后地形未提交，旧三层仍恢复；提交后新三层生效：roundtrip L416-506 与 editing.rs L217-320（顺序：durable 物化→建筑预写→对象预写→地形提交；受保护配对槽写对面槽，save_atomic L518-541；单测 `protects_slot_matching_durable_terrain_until_commit` 双预写下受保护槽字节不变）。
- R4 terrain/object 存档无建筑槽时加载为空建筑层：F11 + 兼容单测。
- 两次 visual lab 的 building hash 和四张截图 SHA 一致：`0x9282f7ed02b8eb20`；R5_01 `c759294086734026f765a2e573030b1d2d27c5b175a825c14b33d0128c94a53e`、R5_02 `a4625fb13516410a52154914b0cef6ae272b7855b5351ce13841eb15937ae671`、R5_03 `aa5b985cd315c673825c4cc1df5196359b5378b9b42195275dc854bd36e86d40`、R5_04 `768352349c1236b12ea3feb8c694c733ab051a5590dde75ee24b478a77330282`（A=B 逐张）。
- 最终证据无递归 ZIP 套娃、无第三方模型/贴图/原包字节：子轮 ZIP 已按 R5-11 展开删除；泄漏守卫（路径/格式/内容哈希/嵌套深度≤3/Git 跟踪文件哈希）在 R5 层对目录通过（日志 L1531），对 ZIP 的检查因后台 shell 收尾被终止，Reviewer 以相同参数独立重跑通过（exit 0）；R4 内部各层守卫成功消息共 14 次于日志。

## 对 R5-13/R5-14 等价修正的独立判断

1. rustfmt 括号（buildings.rs L927/L974 两处 `(edge.start.z as f32) < max.z`）：纯语法消歧，语义不变。
2. 测试世界 40→48：48 为 CHUNK_SIZE=16 的倍数，覆盖全部测试锚点，与 objects.rs/building_runtime.rs 既有惯例一致；属修正测试自身缺陷，未触碰实现。
3. 按键顺序 L→H→J：与 R4 对象层 M 键"进入移动恢复当前朝向"语义同构，测试验证同一契约。
4. watchdog 证据对齐 R4 模式：只增强失败诊断（watchdog.txt + app_stdout.log），未改任何通过门槛。
5. F03 改柱：原断言在冗余支撑围栏下永假（测试形同虚设）；修正后真正验证"删必要支撑整体失败"且新增构件保留断言——门槛由虚变实，不降反升。
6. 中断恢复坐标挪位：原场景 Stone(30,21,30) 与 campfire(28,21,28) 的 3×3 footprint [28,31)² 必然冲突、自相矛盾；挪至 (20,21,20) 后断言仍只比较语义哈希，"新地形≠旧耐久地形"语义不变。
7. lab 截图时机/构图：推迟 1.5s 避免拍到清屏色、拉近相机避免天空占比；**构图参数不在 building_hash 内**（hash=store.semantic_hash()，仅 next_id+排序记录），**validate_png 阈值与 R4 逐字相同**（Reviewer 独立 diff 确认）——不构成门槛放宽。
- R5-14（durable 读取失败阻塞保存）：相比 R4 的 `.ok()` 静默降级是有意收紧，与 R5-09 的旧配对保护承诺一致（editing.rs L223-241，地形或对象物化失败即 "Save blocked"，三层均不写）。

结论：全部等价修正均有据，未发现降低验收门槛之处。

## Reviewer 证据要点

- 证据目录：`evidence_r5_review_9ec9fa8d0fe2_20260915_130118`；ZIP：
  `evidence_r5_review_9ec9fa8d0fe2_20260915_130118.zip`（22,085,895 字节），
  SHA-256 `739ace88a647fdfe270c5d377d78fb44292ac3763e0dfb0c920a767b38bbc126`。
- 根 report.json：`commit=9ec9fa8d0fe2058b36ec50c01d5eafa4e79f7820`、`overall=PASS`、
  `r4=PASS`、`r5_headless=PASS`、`r5_visual=PASS`、`visual_building_hash=0x9282f7ed02b8eb20`、F01-F12 全 PASS。
- 验收时长：约 58 分钟（全新克隆冷编译占大部分；静态检查、R4 全回归含 R1 巡航 628s、
  R5 headless、R5 visual A/B 依次执行）。

## Release

- 标签 `r5-baseline-<S短SHA>` 解析引用到 S：发布后复验（由实现方执行 verify_r5_release.sh）
- 历史标签 `r4-baseline-1c05cfafce2e` 未移动：发布后复验（由实现方执行 verify_r5_release.sh）
- Release 只含实现方证据、Reviewer 证据和校验清单：发布后复验（由实现方执行 verify_r5_release.sh）
- 发布后下载复验：发布后复验（由实现方执行 verify_r5_release.sh）
