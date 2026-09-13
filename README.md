# 工匠要塞 · 初版验证构建

验证构建产物：**底层框架 + 确定性世界生成 + 自由透视视角 + 可修改世界与存档底座**。
技术基线：Rust + Bevy **0.19.1** + ECS；1 m 三维体素；Chunk 数据与 ECS 独立对象分离。

## 环境要求

- Windows 10/11（开发与验收均在 Windows 11 + Vulkan / NVIDIA RTX 3060 Laptop 上验证）
- Rust stable 工具链 **1.98.1**（`rust-toolchain.toml` 已锁定；Bevy 0.19.1 的 MSRV 为 1.95.0）
- 支持 Vulkan / DX12 / Metal 的 GPU（wgpu 29 默认后端）
- Git Bash（运行验收脚本 `scripts/acceptance.sh` 时需要）

## 构建与启动

```bash
cargo build --release          # Release 构建
cargo run --release            # 进入验证世界（交互模式）
```

启动参数（`--key value` 形式，`--help` 查看全部）：

| 参数 | 默认 | 说明 |
|---|---|---|
| `--seed N` | `12345` | 世界种子（固定验收预设，特征齐备） |
| `--size X Y Z` | `256 128 256` | 世界尺寸（体素；16 的整数倍；256..512） |
| `--window W H` | `1280 720` | 窗口分辨率（物理像素） |
| `--tint` | 关 | 启动即开启 Chunk 边界调试着色（运行中 F3 切换） |
| `--acceptance` | 关 | 验收模式：脚本相机巡航 + 证据 + 自动判定 |
| `--evidence DIR` | `evidence` | 证据输出目录 |
| `--save PATH` | `saves/quicksave.cfsv` | F5 写入的双槽逻辑路径 |
| `--load PATH` | 无 | 启动时加载双槽存档；未给 `--save` 时原路径回写 |

## 操控方式（程序内左下角亦有提示）

| 操作 | 按键 |
|---|---|
| 平移 | `WASD`（相机朝向相对）；`R`/`F` 升降 |
| 水平旋转 | `Q`/`E` 或鼠标左键拖动 |
| 平移视角 | 鼠标右键拖动 |
| 缩放 | 滚轮 |
| 加速 | `Shift` |
| Chunk 边界调试着色 | `F3` |
| 删除命中体素 | `Delete` |
| 在命中面外侧放置 Stone | `B` |
| 保存 | `F5` |
| 调试截图（全分辨率 PNG 至工作目录） | `F12` |

限制：俯仰 12°..87°，轨道距离 6..150 m，焦点限制在世界边界内；普通交互焦点移动会拆成小于半个体素的子步，并对每段执行 DDA 连续扫描，完整方向受阻时按轴滑动，避免低帧率或 Shift 加速时穿过薄墙。相机眼位继续使用碰撞收缩（实际距离可临时收缩至 0）和一阶平滑。

## 自动验收（单一入口）

```bash
bash scripts/acceptance.sh          # 暖缓存通常约 12-15 分钟
bash scripts/acceptance.sh my_dir   # 指定全新的证据目录

# 较慢机器可覆盖“应用阶段”的 watchdog；不包含前置编译时间
ACCEPTANCE_WATCHDOG_SEC=1500 bash scripts/acceptance.sh my_dir
```

完整干净克隆还包含首次编译，实际总耗时可能显著高于暖缓存运行。应用阶段 watchdog 默认 1200 秒。

一次调用依次完成：
`cargo fmt --check` → `clippy -D warnings` → 依赖版本审计（bevy 全家必须 0.19.x）→ 旧名称扫描 → `cargo test --release` → Release 构建 → `--acceptance` 模式运行（8 个固定机位截图、120 s 指标巡航、单体素编辑重建、拾取断言、低空普通控制路径（横穿山体/悬崖/边界，无脚本贴地保护）、双帧率阶段 TPS 验证、10 分钟耐久、GIF 录像）。

证据目录产物：
`report.json`（机器可读判定）、`report.md`（人类可读报告）、`metrics.csv`（逐秒指标）、
`resource_timeline.csv`（资源生命周期时间序列）、`screenshots/*.png`、`cruise_timelapse.gif`、`features.txt`、`build_checks.json`、`machine.txt`（OS/工具链）、`machine.json`（GPU/驱动/图形后端/窗口模式/命令行/run_id）、`commit.txt`（验收提交完整 SHA）、`app_stdout.log`、`panic.txt`（仅 panic 时出现）。

入口自身有前置守卫：工作区必须干净（未提交改动即失败）、证据目录必须全新（已存在非空目录即失败）、应用运行有总超时（卡死自动失败并留下 `watchdog.txt`）、运行日志自动扫描 panic/ERROR/FATAL 与持续重复错误。

**退出码 0 = A01–A13 全部 PASS**；A14（独立复核）由 Reviewer 从干净状态重跑本入口出具。

## 存档与加载

```bash
cargo run --release -- --save saves/colony.cfsv
cargo run --release -- --load saves/colony.cfsv
```

逻辑路径对应 `.slot0` / `.slot1` 两个轮换槽。存档仅保存相对 Seed 世界的修改覆盖层；加载时重新生成基础世界、应用覆盖层并核对语义哈希。损坏的最新槽会自动回退到上一有效槽。格式与恢复协议见 `docs/SAVE_FORMAT.md`。

基岩不可编辑，世界最顶层 `H-1` 保持为空气安全层。

## R2 自动验收

```bash
bash scripts/r2_acceptance.sh
```

该入口先完整重跑 R1 图形验收，再执行体素删除/放置、跨 Chunk 标脏、覆盖层规范化、双槽保存、精确加载、损坏回退和槽位修复。实现方与隔离 Reviewer 都必须从全新克隆运行。

## 诊断 HUD（左上角）

Seed、世界尺寸、20 TPS 固定步计数、FPS/帧时间、RSS、相机/焦点/距离/俯仰、
已生成/已 Mesh/脏 Chunk 数、**可见未准备数**（正式巡航恒为 0）、当前拾取命中。
应用内文本为 ASCII 英文标签（Bevy 默认字体无 CJK 字形，见决策记录 D-16）。

## 仓库布局

```
src/            全部源码（模块职责见 docs/ARCHITECTURE.md）
scripts/        acceptance.sh / r2_acceptance.sh 验收入口
examples/       feature_scan.rs / r2_roundtrip.rs 验收工具
docs/           ARCHITECTURE.md / SAVE_FORMAT.md / DECISIONS.md / THIRD_PARTY.md
evidence_*/     验收证据包（运行时生成，不入库）
```

## 测试

```bash
cargo test --release    # 全量：确定性、坐标转换、边界、Mesh、拾取、相机安全、固定步与特征回归
```
