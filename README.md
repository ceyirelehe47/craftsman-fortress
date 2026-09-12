# 工匠要塞：机械纪元（暂名）· 初版验证构建

任务书 V0.2 切片的可运行产物：**底层框架 + 确定性世界生成 + 自由透视视角**。
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

## 操控方式（程序内左下角亦有提示）

| 操作 | 按键 |
|---|---|
| 平移 | `WASD`（相机朝向相对）；`R`/`F` 升降 |
| 水平旋转 | `Q`/`E` 或鼠标左键拖动 |
| 平移视角 | 鼠标右键拖动 |
| 缩放 | 滚轮 |
| 加速 | `Shift` |
| Chunk 边界调试着色 | `F3` |

限制：俯仰 12°..87°，轨道距离 6..150 m，焦点限制在世界边界内；相机不会停留在实体体素内（碰撞收缩 + 一阶平滑）。

## 自动验收（单一入口）

```bash
bash scripts/acceptance.sh          # 完整验收，约 12-15 分钟
bash scripts/acceptance.sh my_dir   # 指定证据目录
```

一次调用依次完成（任务书 5.1）：
`cargo fmt --check` → `clippy -D warnings` → 依赖版本审计（bevy 全家必须 0.19.x）→ `cargo test --release` → Release 构建 → `--acceptance` 模式运行（8 个固定机位截图、120 s 指标巡航、单体素编辑重建、拾取断言、10 分钟耐久、GIF 录像）。

证据目录产物：
`report.json`（机器可读判定）、`report.md`（人类可读报告）、`metrics.csv`（逐秒指标）、
`screenshots/*.png`、`cruise_timelapse.gif`、`features.txt`、`build_checks.json`、`machine.txt`、`app_stdout.log`、`panic.txt`（仅 panic 时出现）。

**退出码 0 = A01–A13 全部 PASS**；A14（独立复核）由 Reviewer 从干净状态重跑本入口出具。

## 诊断 HUD（左上角）

Seed、世界尺寸、20 TPS 固定步计数、FPS/帧时间、RSS、相机/焦点/距离/俯仰、
已生成/已 Mesh/脏 Chunk 数、**可见未准备数**（正式巡航恒为 0）、当前拾取命中。

## 仓库布局

```
src/            全部源码（模块职责见 docs/ARCHITECTURE.md）
scripts/        acceptance.sh 验收单一入口
examples/       feature_scan.rs 验收世界特征扫描工具
docs/           ARCHITECTURE.md / DECISIONS.md / THIRD_PARTY.md
evidence_*/     验收证据包（运行时生成，不入库）
```

## 测试

```bash
cargo test --release    # 42 项：确定性、坐标转换、边界、Mesh、拾取、固定步、特征回归
```
