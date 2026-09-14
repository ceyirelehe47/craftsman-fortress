#!/usr/bin/env bash
# R3.1 自主取得 PalmStudio 免费样本：只允许受控私有缓存或受控下载 URL。
# 不把第三方素材写入 Git；缺少缓存时明确失败，不把下载步骤转交给用户。
set -euo pipefail
cd "$(dirname "$0")/.."
export LC_ALL=C

EXPECTED_SHA256='ababc51f543ec06d07e68d95cdcc90d8ae878d6d12908dd90a0746a836e82fed'
EXPECTED_BYTES='1913239'
DEST="${1:-third_party_raw/Free_Sample.rar}"
CACHE_FILE="${R3_ASSET_CACHE_FILE:-}"
DOWNLOAD_URL="${R3_ASSET_DOWNLOAD_URL:-}"

if [ -z "$CACHE_FILE" ]; then
  if [ -n "${XDG_CACHE_HOME:-}" ]; then
    candidate="$XDG_CACHE_HOME/craftsman-fortress/vendor/palmstudio/voxel-survival-pack/v1.0/Free_Sample.rar"
  elif [ -n "${LOCALAPPDATA:-}" ]; then
    candidate="$LOCALAPPDATA/craftsman-fortress/vendor/palmstudio/voxel-survival-pack/v1.0/Free_Sample.rar"
  else
    candidate="${HOME:-.}/.cache/craftsman-fortress/vendor/palmstudio/voxel-survival-pack/v1.0/Free_Sample.rar"
  fi
  [ -f "$candidate" ] && CACHE_FILE="$candidate"
fi

mkdir -p "$(dirname "$DEST")"
TMP="${DEST}.tmp-$$"
rm -f "$TMP"
cleanup() { rm -f "$TMP"; }
trap cleanup EXIT

SOURCE_KIND=''
SOURCE_LOCATOR=''
if [ -n "$CACHE_FILE" ] && [ -f "$CACHE_FILE" ]; then
  cp "$CACHE_FILE" "$TMP"
  SOURCE_KIND='private_cache'
  SOURCE_LOCATOR='managed-cache:palmstudio-vsp-v1.0-free-sample'
elif [ -n "$DOWNLOAD_URL" ]; then
  command -v curl >/dev/null 2>&1 || {
    echo "失败：R3_ASSET_DOWNLOAD_URL 已设置，但环境缺少 curl。" >&2
    exit 69
  }
  curl --fail --location --retry 3 --retry-delay 2 --output "$TMP" "$DOWNLOAD_URL"
  SOURCE_KIND='private_url'
  SOURCE_LOCATOR='managed-url:palmstudio-vsp-v1.0-free-sample'
else
  cat >&2 <<MESSAGE
失败：缺少 R3 私有素材缓存。
请由运行环境预置 R3_ASSET_CACHE_FILE，或提供受控 R3_ASSET_DOWNLOAD_URL。
验收流程不得要求用户手工下载/转交文件。
MESSAGE
  exit 78
fi

ACTUAL_SHA=$(sha256sum "$TMP" | awk '{print $1}')
ACTUAL_BYTES=$(wc -c < "$TMP" | tr -d ' ')
[ "$ACTUAL_SHA" = "$EXPECTED_SHA256" ] || {
  echo "失败：样本 SHA-256 不匹配：$ACTUAL_SHA" >&2
  exit 1
}
[ "$ACTUAL_BYTES" = "$EXPECTED_BYTES" ] || {
  echo "失败：样本字节数不匹配：$ACTUAL_BYTES" >&2
  exit 1
}

mv "$TMP" "$DEST"
LOCATOR_ID=$(printf '%s' "$SOURCE_LOCATOR" | sha256sum | awk '{print $1}')
cat > "$(dirname "$DEST")/acquisition_receipt.txt" <<RECEIPT
source_kind=$SOURCE_KIND
source_locator_id=$LOCATOR_ID
archive_name=Free_Sample.rar
archive_sha256=$ACTUAL_SHA
archive_bytes=$ACTUAL_BYTES
acquired_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
user_interaction=none
RECEIPT

echo "R3.1 素材自主取得完成：$DEST"
echo "archive_sha256=$ACTUAL_SHA"
echo "archive_bytes=$ACTUAL_BYTES"
