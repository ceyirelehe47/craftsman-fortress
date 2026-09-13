# R1.1 独立复核报告

- **代码验收提交 S**：`73bdffc685d8a8a0c5ceaa258f3e7ae7f4aff44f`
- **实现方 run_id**：`1789302528-38968`
- **Reviewer run_id**：`1789304162-38776`
- **实现方命令**：`ACCEPTANCE_WATCHDOG_SEC=1200 bash scripts/acceptance.sh evidence_impl_73bdffc685d8`（全新克隆检出 S）
- **Reviewer 命令**：`ACCEPTANCE_WATCHDOG_SEC=1200 bash scripts/acceptance.sh evidence_review_73bdffc685d8`（独立全新克隆 `git clone --no-local /d/code/game` 后检出 S）
- **最终结论**：`PASS`

## 1. 提交与工作区

- 实现方从全新克隆检出 `S`，工作区：干净（`git status --porcelain` 为空）
- Reviewer 从独立全新克隆检出 `S`，工作区：干净（`git status --porcelain` 为空，`git rev-parse HEAD` 精确等于 `73bdffc685d8a8a0c5ceaa258f3e7ae7f4aff44f`）
- `S` 的 CI：https://github.com/ceyirelehe47/craftsman-fortress/actions/runs/34756462282

## 2. R1.1 回归修复

Reviewer 在自己验收运行的 `cargo test --release` 输出中逐条确认三个新测试真实执行（非补跑、非转述），单元测试总计 `test result: ok. 56 passed; 0 failed`。

| 检查 | 结论 | 证据 |
|---|---|---|
| 40.5m 高速位移不能穿一格厚墙 | PASS（`test camera::tests::high_speed_interactive_move_cannot_tunnel_through_thin_wall ... ok`） | `high_speed_interactive_move_cannot_tunnel_through_thin_wall` |
| 高速斜向移动可沿墙滑动 | PASS（`test camera::tests::high_speed_diagonal_move_slides_along_thin_wall ... ok`） | `high_speed_diagonal_move_slides_along_thin_wall` |
| H-2 实体列恢复到 H-1 空气层 | PASS（`test camera::tests::focus_recovery_can_use_top_air_layer ... ok`） | `focus_recovery_can_use_top_air_layer` |

## 3. A01–A14

Reviewer 证据目录 `evidence_review_73bdffc685d8/`，`report.json` 的 `overall` = `PASS`，`commit.txt` = `73bdffc685d8a8a0c5ceaa258f3e7ae7f4aff44f`，入口退出码 0（总耗时 1440s，含全新克隆全量编译），应用退出码 0。

| ID | 结论 | Reviewer 独立依据 |
|---|---|---|
| A01 | PASS | 验收入口四步（fmt/clippy --all-targets -D warnings/test --release/build --release）全部通过；build_checks.json 四项 true；日志错误扫描 0 panic/ERROR/FATAL、无 ≥50 次持续重复输出 |
| A02 | PASS | 我的 report.json：世界哈希 live/顺序重建/乱序重建 = `0x6714e0ea62871642` 三值一致（耗时 1.3s） |
| A03 | PASS | 我的 report.json：三方逐体素等价 24/24 样本一致（首差异 None）、跨界列对 300/300 符合纯函数定义（≥99%）；复算见第 4 节 |
| A04 | PASS | 八类特征数值齐备（平原高差 2m/丘陵 25m/山地峰值 115m/山谷/悬崖/洞口/地下洞穴/边界）；8 张机位截图逐张目检全部正常（见第 5 节） |
| A05 | PASS | 我的 report.json：编辑 [16,18,6] 后 6.0s 队列清空=true、新体素固化=true、双侧标脏=true；`A05_edit_rebuild.png` 目检可见边界整齐的竖直开挖沟槽且 dirty=0 |
| A06 | PASS | 我的 report.json：finite=0 clamp=0 speed=0；12 项覆盖全 true（低空横山实体信号 1355 帧、悬崖贴行/边界贴行均 true） |
| A07 | PASS | 复算 metrics.csv：visible_unready 全程最大 0（627 采样）；尾段 510.1s 起 1Hz 采样连续静默 119.3s（≥5s）；应用帧级统计最长连续静默 548.0s（≥5s）；收尾队列残留 0 |
| A08 | PASS | 我的 report.json：眼位实体内事件=0、焦点实体内事件=0、距离振荡事件=0；交互探针 checks=12 blocked=12 failures=0 |
| A09 | PASS | 我的 report.json：8/8 条射线通过（平原垂直/山顶/水平跨Chunk/斜角/空射线/最大缩放等价/陡峭侧壁/洞口井射）；`A09_picking_highlight.png` 目检可见黄色高亮框 + HUD `Pick: (38,35,6) face PosY` |
| A10 | PASS | 复算 metrics.csv（最小二乘）：正常窗斜率 20.0002 TPS、节流窗斜率 20.0003 TPS，均 20±2%；两窗 FPS 分离 5.40×（≥1.15×），见第 4 节 |
| A11 | PASS | 我的 report.json：运行 628.0s、panic=false、队列残留=0；应用进程在 watchdog 1200s 内正常退出（退出码 0） |
| A12 | PASS | 复算 metrics.csv：mesh_assets 首=尾=min=max=1020 恒定、materials 2 恒定、chunk_entities 1016 恒定；mesh_created(1016)−mesh_removed(0)=1016=在册 Chunk 实体；RSS ×1.017 ≤1.25 |
| A13 | PASS | 复算 metrics.csv（排除节流窗 77 样本）：fps 中位数 241.2 ≥60；p95 帧时 4.21ms ≤33.3ms；report 帧级统计（123683 帧）中位 FPS 240.5、p95 4.74ms 一致 |
| A14 | PASS | 本报告 |

## 4. 独立复算

Reviewer 用 Python 从自己的 `evidence_review_73bdffc685d8/metrics.csv`（627 个采样，t=1.24s..629.39s）与 `report.json` 独立计算，非复述实现方数据。

### A03

读取 Reviewer 自己的 `report.json`：三方逐体素等价（单 Chunk 纯函数 / 独立 World 邻域乱序 / 正式世界）样本 **24/24 一致**，`first_difference=None`；跨界列对 **300/300** 符合纯函数地形定义（门槛 ≥99%）。判定字段逐项核对为 PASS。

### A07

对 metrics.csv 的 `dirty` 与 `visible_unready` 列逐行扫描（1Hz 采样）：

- `visible_unready` 全程 627 个采样最大值 = **0**
- `dirty` 非零仅出现在编辑波次附近（全程最大 1216），波次结束后均归零
- **巡航尾段静默窗**：尾段最后 120s（120 个采样）dirty 与 visible_unready 恒为 0，最长连续静默窗 **119.3s**（起点 t=510.13s，1Hz 采样粒度），远超 ≥5s 门槛；全程最长连续静默窗 **579.8s**（起点 t=49.56s）
- 应用帧级统计（我的 report.json）：最长连续静默 **548.0s** ≥5s，收尾队列残留 0——与 CSV 复算结论一致（粒度差异）

### A10

用 `t_s` 与 `fixed_ticks` 列做最小二乘回归（`slope = (n·Σxy − Σx·Σy) / (n·Σx² − (Σx)²)`）：

| 窗口 | 样本数 | 回归斜率（Reviewer） | 偏差 | FPS 均值（Reviewer） | 我的 report.json 记录 |
|---|---|---|---|---|---|
| 正常窗 t_s 48–168 | 120 | **20.0002 TPS** | +0.001% | **240.3** | 20.000 TPS / 240.9 |
| 节流窗 t_s 410–478 | 68 | **20.0003 TPS** | +0.002% | **44.5** | 20.000 TPS / 44.5 |

- 两窗斜率均在 **20±2%** 内；FPS 均值分离比 **240.3/44.5 = 5.40×**（阈值 ≥1.15×）

### A12

对 metrics.csv 的资源列做首/尾/min/max 与守恒核对：

- `mesh_assets`：首 1020 / 尾 1020 / min 1020 / max 1020，**全程恒定**
- `materials`：首 2 / 尾 2 / min 2 / max 2，**全程恒定**
- `chunk_entities`：首 1016 / 尾 1016 / min 1016 / max 1016，**全程恒定**
- 生命周期守恒：`mesh_created`(1016) − `mesh_removed`(0) = **1016 = 在册 Chunk 实体数**；`mesh_replaced`(2034) 为编辑波次替换量；mesh_assets 与 chunk_entities 的恒定差 4 为平台内置资产常量，与 report.json「内置差恒定 4」一致
- RSS：首稳定循环（t≥180）均值 521.5MB，末段（t≥508）峰值 530.6MB，比值 **×1.017** ≤1.25

### A13

- 排除节流窗（t_s 405–483，共 77 个采样）后剩 **550** 个采样
- `fps` 列中位数 = **241.2** ≥ 60（达标）
- p95 帧时（1/fps 口径）= **4.21ms** ≤ 33.3ms（达标）；`frame_ms_ema` 口径 p95 同为 4.21ms，两口径一致
- 与我的 report.json 帧级统计（123683 帧：中位 FPS 240.5、p95 帧时 4.74ms、>200ms 帧 0）交叉一致

## 5. 截图与 GIF 目检

Reviewer 用图像查看工具逐张目检 `evidence_review_73bdffc685d8/screenshots/` 全部 10 张 PNG，另用脚本校验尺寸：

| 文件 | 目检结论 |
|---|---|
| A04_01_plains.png | 1280×720，正常。平原地平线视角：浅蓝天空 + 底部绿色平原与灰色岩石露头；HUD FPS 240.7 / TPS 20.0 / meshed 1016 / dirty 0 / visible-unready 0 |
| A04_02_hills.png | 1280×720，正常。绿色草被丘陵、棕色泥土层理、灰色岩壁，体素高差起伏清晰（Dist 46.9m / Pitch 34°） |
| A04_03_mountain.png | 1280×720，正常。连绵体素山地 + 距离雾，山脊与沟壑分明（Dist 103.8m / Pitch 43°） |
| A04_04_valley_management.png | 1280×720，正常。55° 高空俯瞰山谷全貌：绿色地表、棕色裸土带、灰色沟壑纹理（Dist 127.6m） |
| A04_05_cliff.png | 1280×720，正常。右侧陡峭断崖（灰岩 + 棕土裸露断面）与台地顶部草地，邻列高差明显（Dist 62.9m） |
| A04_06_cave_opening.png | 1280×720，正常。低机位视角，草坡断面与灰色岩壁间可见地表开口暗色结构 |
| A04_07_underground_cave.png | 1280×720，正常。相机位于封闭岩腔内部（Dist 1.5m），灰色岩腔内壁与阶梯几何清晰，右上淡蓝透光开口 |
| A04_08_chunk_boundaries.png | 1280×720，正常。72° 俯瞰，棋盘格 Chunk 调试着色覆盖全图，16³ Chunk 网格清晰可辨 |
| A05_edit_rebuild.png | 1280×720，正常。帧 #1804，画面中央边界整齐的灰色竖直开挖沟槽贯穿崖体，HUD dirty 0（编辑后重建完成） |
| A09_picking_highlight.png | 1280×720，正常。HUD `Pick: (38,35,6) face PosY`，画面中下方绿色地表上黄色高亮框勾选被拾取体素 |

全部 10 张：非全黑、非近单色、无花屏，内容与文件名场景一一相符。

**GIF 抽查**（`cruise_timelapse.gif`）：文件头为 `GIF89a`，大小 2,378,422 字节；用 PowerShell System.Drawing 实际加载解码成功，**64 帧、480×270**，与 report.json `gif_frames: 64` 一致，可正常解码。

## 6. 证据包

| 文件 | SHA-256 |
|---|---|
| `evidence_impl_73bdffc685d8.zip` | `2a19d4e2017f0e1cab6bed26adfc72dd8b126de505654bac3468bd07193cade7` |
| `evidence_review_73bdffc685d8.zip` | `6e31910f5f765e541d7e6f974f6ff977e5492949e424f806b5bbc60d2fe7a688` |

两个 ZIP 内的 `commit.txt` 均为 `73bdffc685d8a8a0c5ceaa258f3e7ae7f4aff44f`，且内部 `report.json` 均为 PASS。两包 SHA 不同源于两次独立运行的 run_id 与时间戳差异，属预期。

## 7. Release 预期

- 计划标签：`r1.1-baseline-73bdffc685d8`
- 标签必须解析到：`73bdffc685d8a8a0c5ceaa258f3e7ae7f4aff44f`
- 实际 Release URL 与发布复核结果在最终交付清单中记录；本报告不写入自身提交 SHA 或发布后才产生的 CI/URL，避免自引用。

## 8. 非阻断观察项

1. **CSV 1Hz 采样对帧级统计的粒度差**：A07 静默窗 CSV 复算值 579.8s 与应用帧级统计 548.0s 结论一致但数值有粒度差；如需 CSV 级精确复算，可考虑在 metrics.csv 增加帧级静默累计列。
2. **低机位截图画面构成**：A04_01（平原）、A04_06（洞口）、A04_07（洞穴内部）中天空/岩壁占画面比例较大，属机位设计使然，场景要素仍可辨认，非渲染缺陷。
3. **GIF 为缩略概览**：cruise_timelapse.gif 为 480×270 / 64 帧的巡航概览，精细判定以截图与 metrics.csv 为准（现流程即如此）。

---

*Reviewer：R1.1 独立验收（隔离工作区，`git clone --no-local` 全新克隆检出 S）；报告日期：2026-09-13*
