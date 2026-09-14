#!/usr/bin/env bash
# 将 PalmStudio Voxel Survival Pack 免费样本准备到本地忽略目录。
# 只复制 GLB/PNG；原始压缩包与处理后素材均不得进入 Git 或 Release。
set -euo pipefail
cd "$(dirname "$0")/.."

usage() {
  cat <<'USAGE'
用法：
  bash scripts/r3_prepare_sample.sh <Free_Sample.rar 或已解压目录> [目标目录]

默认目标：assets/vendor_local/voxel_survival_pack/v1.0/free_sample
环境变量：
  R3_REPLACE=1                    允许清空并重建已有目标目录
  R3_SOURCE_ARCHIVE_SHA256=<hex>  当输入为已解压目录时必须提供原压缩包 SHA-256
USAGE
}

[ "$#" -ge 1 ] && [ "$#" -le 2 ] || { usage >&2; exit 2; }
SOURCE="$1"
DEST="${2:-assets/vendor_local/voxel_survival_pack/v1.0/free_sample}"
SOURCE_URL='https://palmstudio.itch.io/voxel-survival-pack'
PACK_VERSION='v1.0'
SAMPLE_FILE='Free_Sample.rar'

[ -e "$SOURCE" ] || { echo "失败：输入不存在：$SOURCE" >&2; exit 1; }

case "$DEST" in
  assets/vendor_local/voxel_survival_pack/*) ;;
  *) echo "失败：目标必须位于 assets/vendor_local/voxel_survival_pack/ 下" >&2; exit 1 ;;
esac

if [ -d "$DEST" ] && [ -n "$(find "$DEST" -mindepth 1 -maxdepth 1 -print -quit 2>/dev/null)" ]; then
  if [ "${R3_REPLACE:-0}" != "1" ]; then
    echo "失败：目标目录已非空；设置 R3_REPLACE=1 才可重建：$DEST" >&2
    exit 1
  fi
  rm -rf "$DEST"
fi
mkdir -p "$DEST"

SCRATCH=$(mktemp -d)
trap 'rm -rf "$SCRATCH"' EXIT
EXTRACTED="$SCRATCH/extracted"
mkdir -p "$EXTRACTED"

if [ -f "$SOURCE" ]; then
  ARCHIVE_SHA=$(sha256sum "$SOURCE" | awk '{print $1}')
  ARCHIVE_NAME=$(basename "$SOURCE")
  case "${SOURCE,,}" in
    *.rar)
      if command -v 7z >/dev/null 2>&1; then
        7z x -y -o"$EXTRACTED" "$SOURCE" >/dev/null
      elif command -v 7zz >/dev/null 2>&1; then
        7zz x -y -o"$EXTRACTED" "$SOURCE" >/dev/null
      elif command -v unrar >/dev/null 2>&1; then
        unrar x -o+ "$SOURCE" "$EXTRACTED/" >/dev/null
      else
        echo "失败：解压 RAR 需要 7z、7zz 或 unrar。" >&2
        exit 1
      fi
      ;;
    *.zip)
      command -v unzip >/dev/null 2>&1 || { echo "失败：缺少 unzip" >&2; exit 1; }
      unzip -q "$SOURCE" -d "$EXTRACTED"
      ;;
    *) echo "失败：仅支持 RAR/ZIP，或直接传入已解压目录。" >&2; exit 1 ;;
  esac
else
  ARCHIVE_SHA="${R3_SOURCE_ARCHIVE_SHA256:-}"
  [ ${#ARCHIVE_SHA} -eq 64 ] || {
    echo "失败：输入为目录时必须提供 R3_SOURCE_ARCHIVE_SHA256。" >&2
    exit 1
  }
  ARCHIVE_NAME="$SAMPLE_FILE"
  cp -R "$SOURCE"/. "$EXTRACTED"/
fi

# 仅复制运行所需的 GLB 与其可能使用的外部 PNG。保留相对目录结构。
while IFS= read -r -d '' file; do
  rel=${file#"$EXTRACTED"/}
  mkdir -p "$DEST/$(dirname "$rel")"
  cp "$file" "$DEST/$rel"
done < <(find "$EXTRACTED" -type f \( -iname '*.glb' -o -iname '*.png' \) -print0)

GLB_COUNT=$(find "$DEST" -type f -iname '*.glb' | wc -l | tr -d ' ')
[ "$GLB_COUNT" -ge 1 ] || {
  echo "失败：样本中没有找到 GLB；R3 只接受 GLB 管线。" >&2
  exit 1
}

ACQUIRED_AT=$(date -u +%Y-%m-%dT%H:%M:%SZ)
cat > "$DEST/source_receipt.txt" <<RECEIPT
source_url=$SOURCE_URL
pack_name=Voxel Survival Pack
publisher=PalmStudio
pack_version=$PACK_VERSION
sample_file=$SAMPLE_FILE
source_archive_name=$ARCHIVE_NAME
source_archive_sha256=$ARCHIVE_SHA
acquired_at=$ACQUIRED_AT
license_personal_use=allowed
license_commercial_use=allowed
license_modification=allowed
license_attribution=appreciated_not_required
license_redistribution=prohibited_without_explicit_permission
license_reselling=prohibited_without_explicit_permission
license_repackaging=prohibited_without_explicit_permission
source_page_snapshot_note=Record the page text/date in the R3 evidence; do not copy asset files into evidence.
RECEIPT
RECEIPT_SHA=$(sha256sum "$DEST/source_receipt.txt" | awk '{print $1}')

{
  printf 'sha256\tbytes\tkind\tpath\n'
  while IFS= read -r -d '' file; do
    rel=${file#assets/}
    sha=$(sha256sum "$file" | awk '{print $1}')
    bytes=$(wc -c < "$file" | tr -d ' ')
    ext=${file##*.}
    printf '%s\t%s\t%s\t%s\n' "$sha" "$bytes" "${ext,,}" "$rel"
  done < <(find "$DEST" -type f \( -iname '*.glb' -o -iname '*.png' \) -print0 | sort -z)
} > "$DEST/inventory.tsv"

MANIFEST="$DEST/selection.tsv"
{
  echo "# source_url=$SOURCE_URL"
  echo "# pack_version=$PACK_VERSION"
  echo "# sample_file=$SAMPLE_FILE"
  echo "# archive_sha256=$ARCHIVE_SHA"
  echo "# license_receipt_sha256=$RECEIPT_SHA"
  echo "# acquired_at=$ACQUIRED_AT"
  printf 'id\tpath\tcategory\ttarget_height_m\tyaw_deg\n'
  count=0
  while IFS= read -r -d '' file; do
    count=$((count + 1))
    [ "$count" -le 8 ] || break
    rel=${file#assets/}
    base=$(basename "$file" .glb)
    id=$(printf '%s' "$base" | tr '[:upper:] ' '[:lower:]_' | tr -cd 'a-z0-9_-')
    [ -n "$id" ] || id="asset_$count"
    printf '%s\t%s\tunknown\t1.0\t0\n' "$id" "$rel"
  done < <(find "$DEST" -type f -iname '*.glb' -print0 | sort -z)
} > "$MANIFEST"

cat <<SUMMARY
R3 样本准备完成：
  目标目录：$DEST
  GLB 数量：$GLB_COUNT
  原包 SHA-256：$ARCHIVE_SHA
  授权收据 SHA-256：$RECEIPT_SHA
  清单：$MANIFEST

下一步：Agent 必须审阅 selection.tsv，将 category、target_height_m、yaw_deg 从占位值改为真实值；
至少选 3 个 GLB、覆盖至少 2 个类别。素材目录与清单保持本地，不提交 Git。
SUMMARY
