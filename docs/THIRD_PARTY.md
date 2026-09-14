# 第三方依赖与许可证

直接依赖仅 3 个 crate；**未使用任何直接依赖 Bevy 的第三方插件**（排除 bevy_* 系生态插件，
规避 0.19 兼容性风险）。

| crate | 版本约束 | 许可证 | 用途 | Bevy 关系 |
|---|---|---|---|---|
| `bevy` | `=0.19.1` | MIT OR Apache-2.0 | 游戏引擎（冻结 0.19 系列） | — |
| `image` | `0.25`（default-features=false, +png+gif） | MIT OR Apache-2.0 | 验收证据：PNG 截图 / GIF 巡航录像编码 | 版本线与 bevy_image 0.19.1 的 image 依赖一致，由 cargo 统一 |
| `sysinfo` | `0.33` | MIT | 验收 A12：进程 RSS 采样 | 无关 |

## 版本审计

`scripts/acceptance.sh` 每次验收都从 `cargo tree` 提取全部 `bevy`/`bevy_*` 版本并断言
均为 `0.19.x`，同时检查 `Cargo.toml` 声明为 `=0.19.x`。`Cargo.lock` 入库锁定实际解析结果。

## 美术素材

R2.1 及之前的正式运行内容仍全部为程序化视觉。R3 开始验证 PalmStudio 的
**Voxel Survival Pack v1.0** 免费样本，但模型原件、修改后模型、贴图和原始压缩包均为
本地忽略内容，不进入 Git、任务包、CI Artifact、验收证据 ZIP 或 GitHub Release。

来源：`https://palmstudio.itch.io/voxel-survival-pack`。来源页面允许个人与商业项目使用及修改，
署名为可选；禁止在未获明确许可时重新分发、转售或重新打包原始/修改素材。
仓库只提交导入代码、清单格式、测试夹具、来源/哈希元数据和不含模型文件的视觉证据。
`scripts/check_vendor_assets.sh` 在 CI 与完整验收中强制执行该边界。


## R3.1 可审计来源链

第三方样本通过受控私有缓存或受控下载 URL 提供给验收环境；缓存本身不进入仓库或 Release。批准的免费样本原包身份为：

- 文件：`Free_Sample.rar`
- SHA-256：`ababc51f543ec06d07e68d95cdcc90d8ae878d6d12908dd90a0746a836e82fed`
- 字节数：`1,913,239`

`source_receipt.txt`、`inventory.tsv`、`selection.tsv` 和 `selection.lock.tsv` 形成可公开的元数据链，但不包含模型或贴图字节。`scripts/check_vendor_assets.sh` 使用路径、扩展名与内容哈希共同阻止模型、贴图和原包进入公开交付。
