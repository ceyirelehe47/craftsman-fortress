# A14 独立复核报告（Reviewer Agent）

- **复核对象**：commit `9010810`（main，工作区干净）
- **复核方式**：Reviewer 只读取提交版本与本证据包，自行从干净状态重跑单一验收入口并检查原始证据，不接受实现方口头结论
- **重跑命令**：`bash scripts/acceptance.sh evidence_review`
- **重跑结果**：**退出码 0**（A01-A13 全部 PASS），耗时约 10.6 分钟，证据目录 `evidence_review/`
- **最终判定**：**通过**

## 重跑证据

- HEAD = `9010810`，`git status` 干净，`evidence_*/` 经 `git check-ignore` 验证不入库
- 依赖：`Cargo.toml` 声明 `bevy = "=0.19.1"`，`Cargo.lock` 解析 0.19.1，依赖树审计无非 0.19 系列 bevy crate
- 世界哈希 `0x6714e0ea62871642`（live/顺序重建/乱序重建一致），与实现方 `evidence_run7` 完全相同；八类特征探测数值逐字一致；同帧号截图画面一致——确定性可复现
- `cruise_timelapse.gif` 为完整 GIF89a（64 帧）；`app_stdout.log` 无 panic/ERROR/FATAL

## 逐项复核表（Reviewer 重跑结果）

| ID | 项目 | 结论 | 独立验证依据 |
|---|---|---|---|
| A01 | 干净构建与版本基线 | PASS | build_checks.json 四项全 true；bevy 依赖树 0.19.1 |
| A02 | 生成确定性 | PASS | 哈希三值一致且与 run7 相同 |
| A03 | Chunk 独立性 | PASS | 跨界列对 300/300 符合纯函数定义 |
| A04 | 地形覆盖 | PASS | 八类特征齐全 + 8 张机位截图逐一目检 |
| A05 | Mesh 正确性 | PASS | 编辑 [16,18,6] 后 6s 队列清空/固化/双侧标脏；截图无接缝 |
| A06 | 相机全路径 | PASS | finite=0 / clamp=0 / speed=0；metrics.csv 全列 NaN 扫描为 false |
| A07 | 渲染准备 | PASS | 独立复算 metrics.csv：visible_unready 全程最大 0 |
| A08 | 相机实体冲突 | PASS | 实体内事件=0、距离符号交替=0 |
| A09 | 拾取 | PASS | 8/8 射线通过（含空射线未命中）+ 高亮截图可见 |
| A10 | 20 TPS 分离 | PASS | 独立复算：t=200–320s 增量 2405 = 20.042 TPS；9 个 60s 分段 19.717–20.050 全在 20±2% |
| A11 | 稳定性 | PASS | 628s；日志 0 panic/ERROR/FATAL；末段队列清空 |
| A12 | 资源稳定 | PASS | 独立复算：末段峰值/首稳定均值 = ×1.032（阈值 1.25）；meshed 恒 1016 |
| A13 | 性能基线 | PASS | 中位 240.2 FPS、p95 4.55ms、>200ms 帧 0 |
| A14 | 独立复核 | PASS | 本报告 |

## 截图目检（10/10 亲自查看）

1. `A04_01_plains.png` — 草/石/土三色可辨，HUD 完整，无裂缝破洞
2. `A04_02_hills.png` — 起伏轮廓与草/土露头层次分明，无接缝伪影
3. `A04_03_mountain.png` — 山体规模与脊线清晰，远景为雾衰减（预期）而非破洞
4. `A04_04_valley_management.png` — 沟壑谷地可识别，谷底浅色沙带可见
5. `A04_05_cliff.png` — 草/土/石层理与陡坎清晰，Chunk 间无错位裂缝
6. `A04_06_cave_opening.png` — 下沉开口与灰色坑壁可识别
7. `A04_07_underground_cave.png` — 石壁腔体、体素阶梯、拾取高亮清晰；右上蓝色区域与 run7 同帧号比对一致，为确定性几何开口视线而非渲染故障
8. `A04_08_chunk_boundaries.png` — 棋盘格染色清晰，跨边界地形连续无错位
9. `A05_edit_rebuild.png` — 新体素群与两侧 chunk 融合齐整，重建生效
10. `A09_picking_highlight.png` — 黄色描边框在命中面清晰可见

## 非阻断观察项（后续优化）

- RSS 10 段均值 515→526MB 缓慢漂移（约 1MB/分钟，run7 同趋势），远低于 A12 阈值且 Chunk/Mesh 计数恒定，按 A12 口径通过
- A04_06 洞口机位与 A09 高亮框视觉偏小/不醒目，满足"可识别"门槛但可改进
