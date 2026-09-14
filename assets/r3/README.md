# R3 本地素材目录说明

真实 Voxel Survival Pack 文件不在本目录提交。

准备流程：

```bash
mkdir -p third_party_raw
# Agent 自行从 PalmStudio itch.io 页面取得 Free_Sample.rar 后：
bash scripts/r3_prepare_sample.sh third_party_raw/Free_Sample.rar
```

默认生成：

```text
assets/vendor_local/voxel_survival_pack/v1.0/free_sample/
├─ ...本地 GLB/PNG...
├─ source_receipt.txt
├─ inventory.tsv
└─ selection.tsv
```

Agent 必须审阅 `selection.tsv`，为至少 3 个 GLB 设置真实类别、目标高度和朝向。该目录被 `.gitignore` 忽略，不得强制添加。
