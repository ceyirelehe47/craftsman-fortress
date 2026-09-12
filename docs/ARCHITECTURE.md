# 架构说明

初版切片的职责边界与 Bevy 迁移接点。目标读者：后续迭代与 Bevy 次版本迁移的执行者。

## 数据分层（自底向上）

```
noise.rs        纯函数：整数哈希 value noise + fbm/ridged（无全局状态，逐位确定）
   ↓
generation.rs   纯函数：高度场(大陆/丘陵/山脊台地/河谷) + 3D 密度雕刻(通道/洞室) → ChunkData
   ↓
chunk.rs        ChunkData：palette(去重方块表) + 1 字节/体素紧凑索引（存储方向约束）
   ↓
world.rs        World：全部 Chunk 的单一权威持有者（非 ECS Entity，驻留于 Resource）
                voxel() 统一查询 / set_voxel() 单体素修改 + 边界双侧标脏 / world_hash()
   ↓
meshing.rs      纯函数：面剔除 Chunk 网格（跨界邻居经 World 权威查询，跨界无内面/无裂缝）
   ↓
render.rs       Bevy 资源装配：Mesh/StandardMaterial 实体生命周期、加载分片、脏重建、调试着色
```

关键原则：
- **每个体素不是 ECS Entity**；渲染/拾取/生成共享 `WorldRes` 中的同一 `World` 对象，
  任何人不得自建体素副本。
- 生成与网格化都是 `(参数, 坐标) → 结果` 的纯函数，天然与调度顺序无关（A02/A03 的根据）。

## 运行时模块

| 模块 | 职责 |
|---|---|
| `app.rs` / `app_state.rs` | 组装插件、状态机 `Loading → Ready → Finished`、UI 文本 |
| `config.rs` | 命令行解析（Seed/尺寸/窗口/验收开关）；`DEFAULT_SEED=12345` 固定验收预设 |
| `camera.rs` | 焦点轨道相机（`CameraRig` 为权威，`Transform` 是投影）；碰撞收缩 + 一阶平滑（无振荡） |
| `picking.rs` | Amanatides & Woo DDA 拾取；脚本射线与鼠标射线共用；gizmos 高亮 |
| `diagnostics.rs` | 20 TPS 固定步计数、HUD、CSV 指标、RSS 采样、visible-unready 诊断 |
| `features.rs` | 验收世界八类地形特征自动探测（纯数据，供测试与验收共享） |
| `acceptance.rs` | `--acceptance` 模式控制器：时间线巡航、截图、编辑/拾取测试、A01-A13 自动判定 |

## 关键机制

- **时间基线**：`Time::<Fixed>::from_hz(20)` 驱动 `FixedUpdate` 计数器，与渲染帧率天然分离（A10）。
- **加载策略**：CPU 世界数据全常驻（2048 chunks 约 34 MiB 原始体素），加载期每帧 10 ms 预算分片
  生成 + 网格化，完成后切 `Ready`；因此正式巡航中 visible-unready 恒为 0（A07）。
- **边界重建**：`set_voxel` 在体素贴 Chunk 边时同时标脏本块与邻居块；渲染层每帧消费脏队列。
- **相机约束**：俯仰/距离/焦点范围全部钳制；从焦点向相机方向 DDA，命中实体则收缩轨道距离，
  `dist` 一阶指数平滑收敛（A08 无振荡的根据）。

## Bevy 渲染底层 / 第三方接点（迁移敏感区）

| 接点 | 位置 | 说明 |
|---|---|---|
| Mesh 顶点属性装配 | `render.rs::mesh_from_data` | `insert_attribute(POSITION/NORMAL/COLOR)` + `Indices::U32`；注意 `ATTRIBUTE_COLOR` 要求 **Float32x4** |
| `Assets<Mesh>` 句柄复用 | `render.rs::apply_chunk_mesh` | `Assets::insert` 返回 `Result`（0.17+） |
| 截图 API | `acceptance.rs::process_shot_queue` | `bevy::render::view::window::screenshot`；observer 闭包 `On<ScreenshotCaptured>` |
| 观察者/消息系统 | `acceptance.rs` | `On`（0.17 起，原 `Trigger`）、`MessageWriter`（原 `EventWriter`） |
| 系统排序 | `camera.rs::CameraSolve`（SystemSet） | 0.19 起 `before/after` 只接受 SystemSet |
| DirectionalLight 字段 | `render.rs::setup_scene` | `shadow_maps_enabled`（原 `shadows_enabled`） |
| UI 文本 | `app.rs::spawn_ui_text` | `TextFont.font_size: FontSize::Px(f32)`（0.19） |
| Gizmos | `picking.rs` | `gizmos.cube`（0.18 起，原 `cuboid`） |
| 第三方（非 Bevy 插件） | `Cargo.toml` | `image 0.25`（PNG/GIF 证据编码）、`sysinfo 0.33`（RSS） |

## 验收流水线

`scripts/acceptance.sh`（静态检查 + 审计 + 测试 + 构建）→
应用内 `AcceptancePlugin`（巡航 + 采样 + 判定）→ `report.json` / 退出码 →
独立 Reviewer 干净重跑（A14）。
