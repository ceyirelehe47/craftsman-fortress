#!/usr/bin/env bash
# 将人工审阅后的 selection.tsv 锁定到 inventory 中的具体字节。
set -euo pipefail
cd "$(dirname "$0")/.."
export LC_ALL=C

MANIFEST="${1:?usage: r3_lock_selection.sh <selection.tsv> [selection.lock.tsv]}"
LOCK="${2:-$(dirname "$MANIFEST")/selection.lock.tsv}"
LOCAL_DIR=$(dirname "$MANIFEST")
INVENTORY="$LOCAL_DIR/inventory.tsv"
RECEIPT="$LOCAL_DIR/source_receipt.txt"

for file in "$MANIFEST" "$INVENTORY" "$RECEIPT"; do
  [ -s "$file" ] || { echo "失败：缺少文件 $file" >&2; exit 1; }
done

meta() {
  key="$1"; file="$2"
  sed -n "s/^# ${key}=//p; s/^#${key}=//p" "$file" | head -1 | tr -d '\r'
}
SOURCE_URL=$(meta source_url "$MANIFEST")
PACK_VERSION=$(meta pack_version "$MANIFEST")
SAMPLE_FILE=$(meta sample_file "$MANIFEST")
ARCHIVE_SHA=$(meta archive_sha256 "$MANIFEST")
RECEIPT_SHA=$(meta license_receipt_sha256 "$MANIFEST")
ACQUIRED_AT=$(meta acquired_at "$MANIFEST")
ACTUAL_RECEIPT_SHA=$(sha256sum "$RECEIPT" | awk '{print $1}')
INVENTORY_SHA=$(sha256sum "$INVENTORY" | awk '{print $1}')
SELECTION_SHA=$(sha256sum "$MANIFEST" | awk '{print $1}')

[ "$RECEIPT_SHA" = "$ACTUAL_RECEIPT_SHA" ] || {
  echo "失败：selection 的授权收据 SHA 与实际 source_receipt.txt 不一致。" >&2
  exit 1
}
[ -n "$SOURCE_URL" ] && [ -n "$PACK_VERSION" ] && [ -n "$SAMPLE_FILE" ] \
  && [ ${#ARCHIVE_SHA} -eq 64 ] && [ -n "$ACQUIRED_AT" ] || {
  echo "失败：selection 元数据不完整。" >&2
  exit 1
}

TMP="${LOCK}.tmp-$$"
rm -f "$TMP"
trap 'rm -f "$TMP"' EXIT
{
  echo "# format=r3-selection-lock-v1"
  echo "# source_url=$SOURCE_URL"
  echo "# pack_version=$PACK_VERSION"
  echo "# sample_file=$SAMPLE_FILE"
  echo "# archive_sha256=$ARCHIVE_SHA"
  echo "# license_receipt_sha256=$RECEIPT_SHA"
  echo "# inventory_sha256=$INVENTORY_SHA"
  echo "# selection_sha256=$SELECTION_SHA"
  printf 'id\tpath\tfile_sha256\tfile_bytes\tcategory\ttarget_height_m\tyaw_deg\n'

  tail -n +2 < <(grep -v '^#' "$MANIFEST") | while IFS=$'\t' read -r id path category target yaw extra; do
    [ -z "${extra:-}" ] || { echo "失败：selection 字段数错误：$id" >&2; exit 1; }
    [ -n "$id" ] || continue
    row=$(awk -F '\t' -v p="$path" 'NR>1 && $4==p {print $1 "\t" $2 "\t" $3; found++} END {if(found!=1) exit 3}' "$INVENTORY") || {
      echo "失败：inventory 中无法唯一找到 $path" >&2
      exit 1
    }
    IFS=$'\t' read -r file_sha file_bytes kind <<< "$row"
    [ "$kind" = 'glb' ] || { echo "失败：选中项不是 GLB：$path" >&2; exit 1; }
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
      "$id" "$path" "$file_sha" "$file_bytes" "$category" "$target" "$yaw"
  done
} > "$TMP"

mv "$TMP" "$LOCK"
ASSET_SET_SHA=$(sha256sum "$LOCK" | awk '{print $1}')
echo "R3.1 selection lock 已生成：$LOCK"
echo "asset_set_sha256=$ASSET_SET_SHA"
