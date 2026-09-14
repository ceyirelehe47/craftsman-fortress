# R3 第三方 3D 素材管线

## 目标

R3 只验证一条可重复、可审计、可替换的 Bevy 0.19.1 GLB 管线，不把第三方模型变成游戏世界的权威数据，也不改变 `CFSAVE02`。

## 目录边界

```text
third_party_raw/                              原始下载包，仅本地
assets/vendor_local/voxel_survival_pack/     解压/处理后的运行素材，仅本地
assets/r3/README.md                           可提交的使用说明
scripts/r3_prepare_sample.sh                  本地准备工具
src/asset_manifest.rs                         可提交的清单解析与尺度数学
examples/r3_asset_lab.rs                      可提交的视觉/资源验收程序
```

`third_party_raw/` 与 `assets/vendor_local/` 必须被 `.gitignore` 忽略。模型、贴图、原始压缩包不得进入 Git、任务包、CI Artifact、证据 ZIP 或 GitHub Release。

## 清单

本地 `selection.tsv` 是运行时清单。它记录来源、版本、原包 SHA-256、授权收据 SHA-256，以及稳定资产 ID、GLB 路径、类别、目标高度和 Y 轴旋转。

游戏与验收代码只引用稳定 ID 和 `assets/` 下的本地相对路径，不依赖下载包的绝对路径或 glTF 内部索引。

## 尺度规范化

- 世界单位：1 m。
- 每个 GLB 先根据全部 glTF 节点与 Mesh AABB 计算静态原始包围盒。
- 统一缩放：`target_height_m / raw_height`。
- 统一落地：缩放后的最低 Y 对齐地面 `Y=0`。
- 朝向修正由清单中的 `yaw_deg` 明确记录。
- 结果必须输出原始/最终包围盒、缩放值、落地误差和确定性 Transform 指纹。

本轮不改变模型文件，不烘焙新 GLB，不建立动画重定向，也不把素材对象写入存档。

## 验收

`scripts/r3_acceptance.sh`：

1. 执行静态检查、许可证边界检查和真实本地文件检查；
2. 完整重跑 R2.1（包含 R1 图形回归）；
3. 对同一清单独立运行两次 `r3_asset_lab`；
4. 比较清单哈希和 Transform 指纹；
5. 检查固定截图、尺度、落地、加载错误和重复生成/销毁后的资源稳定性；
6. 生成不含第三方模型文件的证据包。

## R3.1 来源链与字节身份

R3.1 将“清单路径存在”升级为完整来源链：

```text
受控私有缓存 / 受控下载 URL
→ Free_Sample.rar 原包 SHA-256 + 字节数
→ acquisition_receipt.txt（本次取得，user_interaction=none）
→ source_receipt.txt（稳定授权快照，无运行时间戳）
→ inventory.tsv（全部 GLB/PNG 的 SHA-256 与字节数）
→ selection.tsv（稳定 ID 与尺度配置）
→ selection.lock.tsv（选中 GLB 的实际字节身份）
```

`scripts/r3_acquire_sample.sh` 不提供人工下载回退；缺少私有缓存时明确失败。实现方与 Reviewer 必须分别在独立克隆中运行取得与准备流程。`scripts/r3_verify_local_assets.sh` 会重新计算原包、授权收据、inventory、selection 与所有本地 GLB/PNG 的哈希。

`assets/r3/free_sample_selection_profile.tsv` 固定本轮已经人工确认过的六个代表模型、类别、目标高度和朝向。`r3_prepare_sample.sh` 会按规范化 basename 从 inventory 唯一解析路径、自动生成 selection 并立即锁定；实现方与 Reviewer 不再手工编辑清单。

`manifest_hash` 继续描述运行配置；`asset_set_sha256` 描述具体素材字节集合。两者必须同时稳定。

## R3.1 再分发守卫

`check_vendor_assets.sh` 除路径与扩展名外，还使用 inventory/原包内容哈希扫描证据目录、发布 ZIP 与最多三层嵌套 ZIP，因此第三方 PNG 即使改名为普通截图也会被拒绝。

## R3.1 生命周期

所有承载 GLB 场景的父实体必须带 `Visibility`，避免 Bevy B0004。每轮临时对象销毁后至少等待命令应用和层级传播完成，再记录资源计数；五轮中的每一轮都必须回到基线，而不是只检查最终一轮。
