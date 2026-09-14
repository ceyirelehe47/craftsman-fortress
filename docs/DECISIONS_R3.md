# R3 决策记录：Voxel Survival Pack 素材管线与视觉尺度验证

## R3-01 · 真实素材本地化

PalmStudio 模型与贴图不进入公开仓库、任务包、CI Artifact、证据 ZIP 或 Release。仓库只提交代码、清单格式、哈希/来源元数据和截图证据。这样既可执行真实导入验收，又不把素材重新打包分发。

## R3-02 · GLB 优先

R3 只接入 `.glb`。不为 FBX/OBJ 引入转换器或额外第三方 Bevy 插件。GLB 作为单文件容器，先验证 Bevy 0.19.1 自带 glTF 加载路径；其它格式后置。

## R3-03 · 清单驱动、稳定 ID

运行时代码不硬编码下载包绝对路径，也不保存 glTF 内部节点/场景索引。每项素材由本地 TSV 的稳定 ID、路径、类别、目标高度和朝向描述。内部 Rust 结构不是永久游戏设计契约；外部行为和证据格式才是本轮约束。

## R3-04 · 自动尺度与落地

原始模型单位、Pivot 和朝向不可信。R3 从节点层级与 Mesh AABB 计算原始包围盒，再按目标高度统一缩放，并把最低 Y 对齐地面。所有结果必须可数值复核，不能只凭截图判断。

## R3-05 · 素材对象不进入 CFSAVE02

R3 只是导入与视觉验证。第三方模型不成为地形体素，不进入 R2.1 修改覆盖层，也不改变存档 magic、格式版本或生成器修订。正式对象存档留到后续独立切片。

## R3-06 · 双运行和生命周期验收

同一清单在两次干净运行中必须产生相同清单哈希与 Transform 指纹。每项模型重复生成/销毁后，Mesh、Material、Image 和实体数量应回到基线，避免把第三方场景引入资源泄漏。

## R3-07 · 免费样本先行

Agent 先使用 `Free_Sample.rar` 建立管线。只有免费样本通过后，才允许在后续轮次评估完整版。若样本无法提供至少 3 个 GLB 和 2 个类别，Agent应记录实际内容并使用完整版（已合法取得时）补足；不得伪造 PalmStudio 资产或要求用户参与验收。

## R3-08 · 参考补丁的 Windows Git Bash 等价修正

参考补丁的 `check_vendor_assets.sh` ZIP 分支只探测 `unzip` 与 `bsdtar`。Git Bash 环境通常两者皆无（Windows 自带 bsdtar 以 `System32\tar.exe` 的 `tar` 名义提供，`command -v bsdtar` 无法发现），会使验收收尾的 ZIP 泄漏扫描在干净环境必然失败。按仓库既有先例（`scripts/publish_r1_1_release.sh` 的 `zip_list`）补充 `/c/Windows/System32/tar.exe -tf | tr -d '\r'` 回退。该修正不改变扫描的文件集合、判禁规则与失败语义，仅扩大可运行的平台范围。

## R3-09 · 样本取得路径记录

免费样本经由 itch.io 公开下载流程确认与触发（Download Now → No thanks → 免费样本 Free_Sample.rar，upload_id=12763857），实际字节由用户从中转服务器转交落地（`third_party_raw/Free_Sample.rar`，1,913,239 字节，sha256 `ababc51f543ec06d07e68d95cdcc90d8ae878d6d12908dd90a0746a836e82fed`）。来源 URL、页面标注日期（2025-02-09 15:03 UTC）、取得日期（2026-09-14）与授权条款收据均落盘于 `third_party_raw/license_receipt.txt`，收据与原包哈希进入清单元数据。授权边界不因转交方式改变：原件与处理后模型/贴图一律不进公开交付。

## R3-10 · 参考补丁运行时差异的等价修正

制包环境无 Rust/Bevy，补丁中的两处问题在本机首次编译与探针运行时暴露，均按"保持外部行为与验收门槛"原则做了等价修正：

1. `examples/r3_asset_lab.rs` 生命周期记录处存在 E0502 借用冲突（`lifecycle_counts.push` 的可变借用与参数中读取 `state.lifecycle_cycle` 冲突）。修正为先把 `lifecycle_cycle` 拷贝到局部变量再格式化，行为不变。
2. `r3_asset_lab` 沿用 `DefaultPlugins` 默认资产根（可执行文件目录 `target/release/examples/`），导致相对路径 GLB 全部 "Path not found"。修正为将 `AssetPlugin::file_path` 显式设为 `<CWD>/assets`，与清单 `--check-files assets` 的解析基准一致；不改变清单格式、路径语义与任何验收门槛。

## R3.1-01 · 受控私有缓存替代人工转交

公开仓库不能再分发素材，官方 itch.io 下载又不适合作为无交互 CI 依赖。R3.1 将受控私有缓存或受控临时 URL 定义为验收基础设施；Agent 与 Reviewer 各自在独立克隆中运行 `r3_acquire_sample.sh`，不得要求用户手工下载或传文件。

## R3.1-02 · 稳定授权收据与运行收据分离

`source_receipt.txt` 只记录稳定来源与授权快照，不含运行时间；`acquisition_receipt.txt` 记录本次取得方式与时间。selection 必须绑定实际 source receipt SHA，Reviewer 重新生成后应得到同一授权收据哈希。

## R3.1-03 · 双指纹

`manifest_hash` 用于稳定 ID、类别、尺度和朝向；`asset_set_sha256` 来自 `selection.lock.tsv`，同时绑定原包、授权收据、inventory、selection 和选中 GLB 的逐文件 SHA-256/字节数。任何模型或贴图字节变化都必须导致来源链验收失败。

## R3.1-04 · 内容哈希泄漏守卫

不能全局禁止 PNG，因为验收截图本身是 PNG。守卫改为组合使用本地路径规则、模型/压缩包扩展名、原包哈希和 inventory 中每个 GLB/PNG 的内容哈希，并递归检查嵌套 ZIP。第三方贴图改名后仍会失败。

## R3.1-05 · Bevy 层级与延迟采样

Bevy B0004 的根因是 GLB 场景子实体继承 Visibility，而父根缺少 Visibility。正式与临时根统一补齐 `Visibility`。临时根 despawn 后不在同一系统调用内采样，而是在后续帧确认资源和实体回到基线；五轮逐轮强制通过。


## R3.1-06 · 稳定选择 profile

R3 已经确认六个免费样本模型的类别和米制尺度。R3.1 将该选择记录为可提交的 `assets/r3/free_sample_selection_profile.tsv`；准备脚本依据规范化 basename 从本地 inventory 唯一解析真实路径，自动生成并锁定 selection。实现方与 Reviewer 不再手工编辑清单，且 profile 不包含任何模型或贴图字节。

## R3.1-07 · profile 读取剥离 CRLF（实现环境修正）

仓库在 Windows 上以 `autocrlf` 检出时，`free_sample_selection_profile.tsv` 的工作区副本为 CRLF 行尾；原读取循环只对表头做 `tr -d '\r'`，数据行的 `yaw_deg` 会带着 `\r` 写入 selection 与 lock，导致 `asset_set_sha256` 在不同平台之间不确定。修正为在读取循环内对行尾字段剥离 `\r`（`yaw=${yaw%$'\r'}`），使两个平台生成字节完全一致的 selection 与 lock；不改变字段数校验、语义或任何门槛。

## R3.1-08 · 发布与复验脚本补历史标签与双 ZIP asset-set 校验（实现环境修正）

R3.1 的发布前验证要求包含“历史 `r3-baseline-355344c10846` 不移动”，且最终门槛要求实现方与 Reviewer 的 `asset_set_sha256` 一致。参考补丁的 `publish_r3_1_release.sh`/`verify_r3_1_release.sh` 未实现这两项机器检查。修正为：发布与下载复验脚本均解引用历史标签并比对固定的 R3 代码提交 SHA；发布脚本另从两套证据 ZIP 的根级 `report.json` 提取 `asset_set_sha256` 并强制相等。两项均为新增校验，不放宽任何既有行为。

## R3.1-09 · 夹具脚本 Python 解释器探测（实现环境修正）

Windows 可能将 `python3` 解析到微软商店占位 stub（`WindowsApps/python3`，运行任意参数即以非零码失败），而 CI 的 ubuntu 自带真 `python3`。`test_r3_source_chain.sh` 改为对 `python3`、`python` 逐一做 `import sys` 探测后选用首个可用解释器构造测试 ZIP；夹具语义与全部负向断言不变。

## R3.1-10 · unzip 容忍警告退出码（实现环境修正）

本机 Git Bash 无 Info-ZIP `zip` CLI，`r2_acceptance.sh`/`r3_acceptance.sh` 的证据 ZIP 由 PowerShell `Compress-Archive` 生成：其本地文件头使用反斜杠，Info-ZIP `unzip -qq` 能完整解出全部条目但以退出码 1（警告）结束。`set -euo pipefail` 下这会误杀泄漏守卫的嵌套 ZIP 解包、发布脚本与下载复验脚本的证据解包。修正：`check_vendor_assets.sh`/`publish_r3_1_release.sh`/`verify_r3_1_release.sh` 的 unzip 调用仅在退出码 >1（真正的解包错误）时失败。已实测该类 ZIP 解包后条目数完整（108/108），守卫对嵌套内容的路径与内容哈希扫描不受影响。
