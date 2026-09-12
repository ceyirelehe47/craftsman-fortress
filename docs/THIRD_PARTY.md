# 第三方依赖与许可证

直接依赖仅 3 个 crate；**未使用任何直接依赖 Bevy 的第三方插件**（排除 bevy_* 系生态插件，
规避 0.19 兼容性风险）。

| crate | 版本约束 | 许可证 | 用途 | Bevy 关系 |
|---|---|---|---|---|
| `bevy` | `=0.19.1` | MIT OR Apache-2.0 | 游戏引擎（任务书冻结 0.19 系列） | — |
| `image` | `0.25`（default-features=false, +png+gif） | MIT OR Apache-2.0 | 验收证据：PNG 截图 / GIF 巡航录像编码 | 版本线与 bevy_image 0.19.1 的 image 依赖一致，由 cargo 统一 |
| `sysinfo` | `0.33` | MIT | 验收 A12：进程 RSS 采样 | 无关 |

## 版本审计

`scripts/acceptance.sh` 每次验收都从 `cargo tree` 提取全部 `bevy`/`bevy_*` 版本并断言
均为 `0.19.x`，同时检查 `Cargo.toml` 声明为 `=0.19.x`。`Cargo.lock` 入库锁定实际解析结果。

## 美术素材

无任何外部美术资产。全部视觉为程序化生成：方块调色板（`voxel.rs`）、
面朝向着色系数、天空/雾色、gizmos 高亮。
