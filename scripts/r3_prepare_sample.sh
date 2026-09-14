#!/usr/bin/env bash
# 将 PalmStudio Voxel Survival Pack 免费样本准备到本地忽略目录。
# R3.1：稳定授权收据与每次取得记录分离；只复制 GLB/PNG。
set -euo pipefail
cd "$(dirname "$0")/.."
export LC_ALL=C

usage() {
  cat <<'USAGE'
用法：
  bash scripts/r3_prepare_sample.sh <Free_Sample.rar 或已解压目录> [目标目录]

默认目标：assets/vendor_local/voxel_survival_pack/v1.0/free_sample
环境变量：
  R3_REPLACE=1                    允许清空并重建已有目标目录
  R3_SOURCE_ARCHIVE_SHA256=<hex>  输入为已解压目录时必须提供
  R3_SOURCE_ARCHIVE_BYTES=<n>     输入为已解压目录时必须提供
USAGE
}

[ "$#" -ge 1 ] && [ "$#" -le 2 ] || { usage >&2; exit 2; }
SOURCE="$1"
DEST="${2:-assets/vendor_local/voxel_survival_pack/v1.0/free_sample}"
SOURCE_URL='https://palmstudio.itch.io/voxel-survival-pack'
PACK_VERSION='v1.0'
SAMPLE_FILE='Free_Sample.rar'
EXPECTED_SHA256='ababc51f543ec06d07e68d95cdcc90d8ae878d6d12908dd90a0746a836e82fed'
EXPECTED_BYTES='1913239'
APPROVED_ARCHIVE_OBSERVED_AT='2026-09-14T03:02:52Z'

[ -e "$SOURCE" ] || { echo "失败：输入不存在：$SOURCE" >&2; exit 1; }
case "$DEST" in
  assets/vendor_local/voxel_survival_pack/*) ;;
  *) echo "失败：目标必须位于 assets/vendor_local/voxel_survival_pack/ 下" >&2; exit 1 ;;
esac

if [ -d "$DEST" ] && [ -n "$(find "$DEST" -mindepth 1 -maxdepth 1 -print -quit 2>/dev/null)" ]; then
  [ "${R3_REPLACE:-0}" = "1" ] || {
    echo "失败：目标目录已非空；设置 R3_REPLACE=1 才可重建：$DEST" >&2
    exit 1
  }
  rm -rf "$DEST"
fi
mkdir -p "$DEST"

SCRATCH=$(mktemp -d)
trap 'rm -rf "$SCRATCH"' EXIT
EXTRACTED="$SCRATCH/extracted"
mkdir -p "$EXTRACTED"

if [ -f "$SOURCE" ]; then
  ARCHIVE_SHA=$(sha256sum "$SOURCE" | awk '{print $1}')
  ARCHIVE_BYTES=$(wc -c < "$SOURCE" | tr -d ' ')
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
  ARCHIVE_BYTES="${R3_SOURCE_ARCHIVE_BYTES:-}"
  [ ${#ARCHIVE_SHA} -eq 64 ] && [ -n "$ARCHIVE_BYTES" ] || {
    echo "失败：输入为目录时必须提供 R3_SOURCE_ARCHIVE_SHA256 与 R3_SOURCE_ARCHIVE_BYTES。" >&2
    exit 1
  }
  cp -R "$SOURCE"/. "$EXTRACTED"/
fi
[ "$ARCHIVE_SHA" = "$EXPECTED_SHA256" ] && [ "$ARCHIVE_BYTES" = "$EXPECTED_BYTES" ] || {
  echo "失败：样本原包不是已批准的 Free_Sample.rar 字节。" >&2
  echo "sha=$ARCHIVE_SHA bytes=$ARCHIVE_BYTES" >&2
  exit 1
}

while IFS= read -r -d '' file; do
  rel=${file#"$EXTRACTED"/}
  mkdir -p "$DEST/$(dirname "$rel")"
  cp "$file" "$DEST/$rel"
done < <(find "$EXTRACTED" -type f \( -iname '*.glb' -o -iname '*.png' \) -print0)

GLB_COUNT=$(find "$DEST" -type f -iname '*.glb' | wc -l | tr -d ' ')
[ "$GLB_COUNT" -ge 1 ] || { echo "失败：样本中没有找到 GLB。" >&2; exit 1; }

# 稳定授权/来源快照：不得包含本次运行时间，因此实现方与 Reviewer 应得到相同 SHA。
cat > "$DEST/source_receipt.txt" <<RECEIPT
source_url=$SOURCE_URL
pack_name=Voxel Survival Pack
publisher=PalmStudio
pack_version=$PACK_VERSION
sample_file=$SAMPLE_FILE
source_archive_name=$SAMPLE_FILE
source_archive_sha256=$ARCHIVE_SHA
source_archive_bytes=$ARCHIVE_BYTES
license_snapshot_id=palmstudio-vsp-v1.0-observed-2026-09-14
approved_archive_observed_at=$APPROVED_ARCHIVE_OBSERVED_AT
license_personal_use=allowed
license_commercial_use=allowed
license_modification=allowed
license_attribution=appreciated_not_required
license_redistribution=prohibited_without_explicit_permission
license_reselling=prohibited_without_explicit_permission
license_repackaging=prohibited_without_explicit_permission
RECEIPT
RECEIPT_SHA=$(sha256sum "$DEST/source_receipt.txt" | awk '{print $1}')

# 本次取得信息单独保存；它不参与稳定授权收据哈希。
SOURCE_ACQUISITION="$(dirname "$SOURCE")/acquisition_receipt.txt"
if [ -s "$SOURCE_ACQUISITION" ]; then
  cp "$SOURCE_ACQUISITION" "$DEST/acquisition_receipt.txt"
else
  cat > "$DEST/acquisition_receipt.txt" <<RECEIPT
source_kind=direct_input
source_locator_id=$(printf '%s' "$SOURCE" | sha256sum | awk '{print $1}')
archive_name=$SAMPLE_FILE
archive_sha256=$ARCHIVE_SHA
archive_bytes=$ARCHIVE_BYTES
acquired_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
user_interaction=none
RECEIPT
fi
ACQUIRED_AT=$(sed -n 's/^acquired_at=//p' "$DEST/acquisition_receipt.txt" | head -1)
[ -n "$ACQUIRED_AT" ] || { echo "失败：acquisition_receipt 缺少 acquired_at。" >&2; exit 1; }

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
INVENTORY_SHA=$(sha256sum "$DEST/inventory.tsv" | awk '{print $1}')

MANIFEST="$DEST/selection.tsv"
{
  echo "# source_url=$SOURCE_URL"
  echo "# pack_version=$PACK_VERSION"
  echo "# sample_file=$SAMPLE_FILE"
  echo "# archive_sha256=$ARCHIVE_SHA"
  echo "# license_receipt_sha256=$RECEIPT_SHA"
  echo "# acquired_at=$APPROVED_ARCHIVE_OBSERVED_AT"
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

PROFILE="${R3_SELECTION_PROFILE:-assets/r3/free_sample_selection_profile.tsv}"
[ -s "$PROFILE" ] || { echo "失败：缺少稳定选择 profile：$PROFILE" >&2; exit 1; }
bash scripts/r3_apply_selection_profile.sh "$MANIFEST" "$PROFILE"
bash scripts/r3_lock_selection.sh "$MANIFEST" "$DEST/selection.lock.tsv"

cat <<SUMMARY
R3.1 样本准备完成：
  目标目录：$DEST
  GLB 数量：$GLB_COUNT
  原包 SHA-256：$ARCHIVE_SHA
  稳定授权收据 SHA-256：$RECEIPT_SHA
  inventory SHA-256：$INVENTORY_SHA
  选择 profile：$PROFILE
  清单：$MANIFEST
  字节锁：$DEST/selection.lock.tsv
SUMMARY
