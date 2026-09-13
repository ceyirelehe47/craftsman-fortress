# A14 独立复核报告（Reviewer Agent，R1 返修轮）

- **复核对象**：commit `4e8c9479a943f5687d62f3b3a6a55efa0b514114`（main HEAD，`git status --porcelain` 为空）
- **复核方式**：隔离 Reviewer 从独立全新工作区重跑单一验收入口。Reviewer 自行执行
  `git clone --no-local /d/code/game game_review` 并确认 HEAD 与工作区干净，不接受实现方口头结论；
  全部验收证据、独立复算数字、截图目检结论均由 Reviewer 本轮亲自产生
- **重跑命令**：`bash scripts/acceptance.sh evidence_review_4e8c947`（在全新克隆的 `D:\code\game_review` 内执行）
- **重跑结果**：**退出码 0**（A01-A13 运行时判定全部 PASS），入口总耗时 1477s（含全新编译），证据目录 `evidence_review_4e8c947/`
- **最终判定**：**通过**（A01-A13 全 PASS + 本报告 A14 复核通过）

## 一、前置核验（Reviewer 亲自执行）

- `git clone --no-local /d/code/game game_review` 后 `git rev-parse HEAD` = `4e8c9479a943f5687d62f3b3a6a55efa0b514114`
- `git status --porcelain` 输出为空
- 确认该提交包含 A06 断言修复：`src/acceptance.rs` 中 `MAX_EYE_SPEED: f32 = 150.0`（显式速度判据，替换旧提交 `6123b37` 的固定位移阈值 `MAX_EYE_STEP = 4.0`），发现过程见第六节附注
- `evidence_review_4e8c947/commit.txt` 内容 = `4e8c9479a943f5687d62f3b3a6a55efa0b514114`（与 HEAD 一致）

## 二、逐项复核表（Reviewer 重跑结果，run_id `1789293676-37072`）

| ID | 项目 | 结论 | 独立验证依据 |
|---|---|---|---|
| A01 | 干净构建（fmt/clippy/test/Release） | PASS | 入口前置四步全过；build_checks.json 四项全 true；日志错误扫描 0 panic/ERROR/FATAL |
| A02 | 生成确定性 | PASS | 世界哈希 live/顺序重建/乱序重建 = `0x6714e0ea62871642` 三值一致（耗时 1.3s） |
| A03 | Chunk 独立性与跨界连续 | PASS | report 运行时判定：三方逐体素等价 24/24 样本一致（首差异 None）、跨界列对 300/300 符合纯函数定义；单元测试 `r1_checks::tests::chunk_equivalence_compares_decoded_voxels` 通过（交叉印证） |
| A04 | 地形覆盖（八类特征 + 证据机位） | PASS | 八类特征数值齐备（平原/丘陵/山地/山谷/悬崖/洞口/地下洞穴/高差 112m）；8 张机位截图逐张目检全部正常（见第四节） |
| A05 | Mesh 正确性与边界重建 | PASS | 运行时：编辑 [16,18,6] 后 6.0s 队列清空/新体素固化/双侧标脏全 true；`A05_edit_rebuild.png` 目检可见边界整齐的竖直开挖沟槽且 dirty 已归零 |
| A06 | 相机全路径（含低空普通控制路径） | PASS | finite=0 clamp=0 speed=0（新速度判据 MAX_EYE_SPEED=150 m/s）；12 项覆盖全 true，低空横山实体信号 1359 帧 |
| A07 | 渲染准备 | PASS | 独立复算 metrics.csv：visible_unready 全程最大 0；dirty 波次后均归零；收尾段（t≥500）130 个采样 dirty 恒 0（详见第三节） |
| A08 | 相机不进入实体 / 无振荡 / 交互探针 | PASS | report 运行时判定：眼位实体内事件=0、焦点实体内事件=0、距离振荡事件=0、交互探针 12/12 blocked（failures=0）；单元测试 `r1_checks::tests::interactive_focus_probe_passes_on_generated_world` 通过（交叉印证） |
| A09 | 拾取（8 条固定射线） | PASS | 8/8 通过（平原垂直/山顶/水平跨Chunk/斜角/空射线/最大缩放等价/陡峭侧壁/洞口井射）；`A09_picking_highlight.png` 目检可见黄色高亮框 + HUD `Pick: (58,35,6) face PosY` |
| A10 | 20 TPS 固定步与渲染分离 | PASS | 独立最小二乘回归：正常窗斜率 20.0006 TPS、节流窗斜率 20.0003 TPS，均 20±2%；帧率分离 5.41×（详见第三节） |
| A11 | 稳定性（≥10 分钟巡航） | PASS | 运行 628s，panic=false，队列残留=0 |
| A12 | 资源稳定（RSS + 生命周期守恒） | PASS | 独立复算：mesh_assets 1020 恒定、materials 2 恒定、chunk_entities 1016 恒定（首尾一致）；RSS ×1.027 ≤ 1.25 |
| A13 | 性能基线 | PASS | 独立复算 metrics.csv：fps 中位数 240.9 ≥ 60（详见第三节） |
| **A14** | **独立 Reviewer 复核** | **PASS** | 本报告：隔离工作区重跑 + 独立复算 + 截图目检 + 单元测试交叉印证 + 证据打包上传 |

## 三、独立复算数字（Reviewer 用 Python 从 metrics.csv 自行计算，非复述 report.json）

### A07 渲染准备

- `visible_unready` 列全程最大值 = **0**（627 个采样）
- `dirty` 非零区间仅 2 个采样点：t=42.67s（max=1344）、t=48.69s（max=1216）。CSV 为 1Hz 采样，应用帧级统计的 3 个 Mesh 修改波次（验收时间轴约 41s/47s/80s，CSV 时间轴约 +1.3s 偏移）中，前两波被采样捕获，第三波（约 81.3s）在采样间隔内已清空故未捕获——与 report"波次=3"不矛盾（粒度差异）
- 每个波次后的首个采样 dirty 均已归零（1344→0、1216→0）
- 收尾段 t≥500：130 个采样 dirty 恒 0
- 最长连续静默（Reviewer 1Hz 采样粒度）：**579.6s**（49.7s → 629.3s）；report 帧级统计 548.0s。两者均远超 ≥5s 门槛，结论一致

### A10 双帧率阶段（最小二乘回归，与验收同算法）

| 窗口 | 样本数 | 回归斜率（Reviewer） | 偏差 | FPS 均值（Reviewer） | report 记录 |
|---|---|---|---|---|---|
| 正常窗 t_s 48–168 | 120 | **20.0006 TPS** | +0.003% | **242.5** | 20.000 TPS / 242.5 |
| 节流窗 t_s 410–478 | 68 | **20.0003 TPS** | +0.001% | **44.8** | 20.000 TPS / 44.8 |

- 两窗均满足 20±2%；帧率分离比 **5.41×**（阈值 ≥1.15×）
- 正常窗 FPS 242.5 > 150、节流窗 FPS 44.8 落在 40–50 区间，双窗口帧率显著不同

### A12 资源生命周期

- `mesh_assets`：首 1020 / 尾 1020 / min 1020 / max 1020，**全程恒定**
- `materials`：首 2 / 尾 2，**全程恒定**
- `chunk_entities`：首 1016 / 尾 1016 / min 1016 / max 1016，**全程恒定**

### A13 性能基线

- fps 列中位数（Reviewer 独立统计）= **240.9**（≥60 达标，627 个采样）；report 帧级统计（124087 帧）中位数 240.3，粒度差异属正常，结论一致

### A03/A08 交叉印证

- `cargo test --release r1_checks`：**2 passed, 0 failed**（`chunk_equivalence_compares_decoded_voxels`、`interactive_focus_probe_passes_on_generated_world`）
- 运行时数字（从 report.md 读取）：A03 三方一致 **24/24**、跨界 300/300；A08 实体内事件 **0/0**、振荡事件 **0**、交互探针 **12/12**（blocked=12，failures=0）

## 四、截图目检清单（Reviewer 逐张查看）

| 文件 | 目检结论 |
|---|---|
| A04_01_plains.png | 正常。浅蓝天空 + 底部绿色平原与灰色岩石露头；HUD FPS 246.6 / TPS 20.0 / meshed 1016 / dirty 0 |
| A04_02_hills.png | 正常。绿色草被丘陵、棕色泥土层理、灰色岩壁，体素高差起伏清晰 |
| A04_03_mountain.png | 正常。连绵体素山地 + 距离雾，山脊与沟壑分明（Dist 103.8m / Pitch 43°） |
| A04_04_valley_management.png | 正常。55° 高空俯瞰山谷全貌，绿色地表、棕色裸土带、灰色河道纹理 |
| A04_05_cliff.png | 正常。右下陡峭断崖（灰岩+棕土裸露断面）与台地顶部草地，邻列高差明显 |
| A04_06_cave_opening.png | 正常。低机位视角，草坡断面与岩壁间可见地表开口暗色结构 |
| A04_07_underground_cave.png | 正常。相机位于封闭腔内部（Cam y=9.8 / Dist 1.5m），灰色岩腔内壁阶梯结构清晰，右上淡蓝透光开口 |
| A04_08_chunk_boundaries.png | 正常。72° 俯瞰，棋盘格状 Chunk 调试着色覆盖全图，16³ Chunk 网格清晰可辨 |
| A05_edit_rebuild.png | 正常。帧号 #1809，画面中央边界整齐的灰色竖直开挖沟槽贯穿崖体，编辑后 mesh 重建完成（dirty 0） |
| A09_picking_highlight.png | 正常。HUD 显示 `Pick: (58,35,6) face PosY`，画面中下方黄色高亮框勾选被拾取体素 |

全部 10 张：非全黑/全白，1280×720，地形/Chunk 调试色/编辑重建/拾取高亮均可见。

**GIF 帧抽查**（cruise_timelapse.gif 由 gif_frames/ 64 帧组装）：

- `0005.png`（Frame #460）：山地远景巡航帧，绿色山体 + 雾效，正常
- `0010.png`（Frame #2960，t=296s）：低空贴行世界边界段，灰色岩壁低视角贴行画面，正常
- `0020.png`（Frame #3340）：高空俯瞰巡航帧，地形完整，正常

## 五、两次运行环境对比与证据包哈希

### 机器与 GPU

| 字段 | 实现方轮（evidence_impl_6123b37） | Reviewer 轮（evidence_review_4e8c947） |
|---|---|---|
| os | MINGW64_NT-10.0-26200 3.6.9 x86_64 | MINGW64_NT-10.0-26200 3.6.9 x86_64 |
| cpu | AMD Ryzen 7 6800H with Radeon Graphics（16 逻辑核） | 同左 |
| rustc / cargo | rustc 1.98.1 (48a229cea 2026-09-01) / cargo 1.98.1 | 同左 |
| gpu_name | NVIDIA GeForce RTX 3060 Laptop GPU | 同左 |
| gpu_driver | NVIDIA / 581.15 | 同左 |
| gpu_backend | Vulkan | Vulkan |
| window_mode | Windowed | Windowed |
| run_id | `1789289454-28744` | `1789293676-37072` |
| git_head | `6123b378bb094fbc548a3140731d07f367fb6180` | `4e8c9479a943f5687d62f3b3a6a55efa0b514114` |

两轮同机同 GPU 同驱动，差异仅在提交与 run_id，运行环境可比。

### 证据包 SHA-256

| 包 | SHA-256 |
|---|---|
| `evidence_review_4e8c947.zip`（Reviewer 本轮，12,136,386 B） | `dde4bd3f93b5a88e3c3098e05f19a2fea9c385b1acf503907c881871bab4e359` |
| `evidence_impl_6123b37.zip`（实现方 6123b37 轮，Release 现存资产，只读参考） | `172ff535fd9ce9e4c12c29dfe9c70e3ede221c982f0fcee5d54d1db7fbe3ba71` |

- `evidence_review_4e8c947/SHA256SUMS.txt`：86 项全部校验 OK（含 zip 自身哈希追加项）
- 实现方针对 `4e8c947` 的新包（`evidence_impl_4e8c947.zip`）由实现方并行重跑后另行上传；**Release 实际资产清单以 GitHub Release `r1-baseline-6123b37` 页面为准**（本报告撰写时点为 `evidence_impl_6123b37.zip` + `evidence_review_4e8c947.zip`）

### 证据获取方式

GitHub Release tag **`r1-baseline-6123b37`**（仓库 `ceyirelehe47/craftsman-fortress`）：

```
gh release download r1-baseline-6123b37 --repo ceyirelehe47/craftsman-fortress
```

Reviewer 包 `evidence_review_4e8c947.zip` 已于本轮复核完成后上传至该 Release。

## 六、附注：6123b37 轮 A06 断言缺陷的发现与修复过程（非游戏代码缺陷）

Reviewer 在对原被验收提交 `6123b378bb094fbc548a3140731d07f367fb6180` 的独立重跑中（本轮之前，证据目录曾命名 `evidence_review_6123b37`，已作废留档）得到 **A06 FAIL**：`speed=1`，唯一违规样本 t=228.51s、期望眼位帧间位移 4.2m（阈值 4.0m），12 项覆盖全 true、finite=0、clamp=0。Reviewer 复算发现违规时刻紧邻 CSV t=229.89 采样 fps 骤降（89.7，正常约 244），判定为帧间隔停顿叠加段过渡合法峰值速度击穿固定位移阈值：4.2m ≈ 合法峰值速度 ~80 m/s × ~76ms 帧间隔，该阈值隐含速度上限仅 40 m/s，低于合法运动峰值——属**断言工程缺陷**（测量方法问题），非相机控制流缺陷。实现方据该发现将断言改为显式速度判据 `MAX_EYE_SPEED = 150 m/s`（对合法峰值约一倍余量、对真瞬移 >300 m/s 约一倍余量），即本次复核对象 `4e8c947`。本轮重跑 A06 PASS（speed=0），缺陷修复得到验证。

## 七、非阻断观察项

1. **A06 速度断言的余量依赖经验峰值估计**：`MAX_EYE_SPEED = 150 m/s` 相对合法峰值 ~80 m/s 约一倍余量，若未来加入更高速的合法运动（如新的脚本段），需同步校准该阈值。
2. **CSV 1Hz 采样粒度对 A07 波次的欠采样**：第三编辑波次在 CSV 中未体现（采样间隔内已清空），帧级判定依赖应用内统计；如需 CSV 级可复核性，可考虑在波次附近加密采样或记录波次计数列。
3. **验收运行存在环境敏感的毛刺暴露面**：6123b37 轮一次后台调度停顿即触发旧阈值误报；现阈值已修复，但 `watchdog` 900s 与 628s 巡航的余量约 43%，慢机上裕度有限。
4. **Release 资产时点性**：本报告撰写时 Release 中实现方包仍为 6123b37 版（哈希见第五节），4e8c947 版实现方包待实现方上传；最终以 Release 页面资产与其随附 SHA256SUMS 为准。

---

*Reviewer：A14 独立验收（隔离工作区 D:\code\game_review）；报告日期：2026-09-13*
