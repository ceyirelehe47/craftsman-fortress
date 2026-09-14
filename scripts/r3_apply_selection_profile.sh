#!/usr/bin/env bash
# 用可提交的稳定 profile，从本地 inventory 自动解析 PalmStudio GLB 路径并生成 selection.tsv。
set -euo pipefail
cd "$(dirname "$0")/.."
export LC_ALL=C

MANIFEST="${1:?usage: r3_apply_selection_profile.sh <selection.tsv> [profile.tsv]}"
PROFILE="${2:-assets/r3/free_sample_selection_profile.tsv}"
LOCAL_DIR=$(dirname "$MANIFEST")
INVENTORY="$LOCAL_DIR/inventory.tsv"
[ -s "$MANIFEST" ] && [ -s "$PROFILE" ] && [ -s "$INVENTORY" ] || {
  echo "失败：selection/profile/inventory 缺失。" >&2; exit 1;
}
[ "$(head -1 "$PROFILE" | tr -d '\r')" = $'id\tbasename\tcategory\ttarget_height_m\tyaw_deg' ] || {
  echo "失败：selection profile header 不正确。" >&2; exit 1;
}

TMP="${MANIFEST}.tmp-$$"
trap 'rm -f "$TMP"' EXIT
# 保留由 prepare_sample 生成的来源元数据；替换数据行。
grep '^#' "$MANIFEST" > "$TMP"
printf 'id\tpath\tcategory\ttarget_height_m\tyaw_deg\n' >> "$TMP"

count=0
while IFS=$'\t' read -r id selector category target yaw extra; do
  # Windows 检出的 profile 为 CRLF；剥离行尾 \r 保证 selection/lock 字节跨平台一致。
  yaw=${yaw%$'\r'}
  [ "$id" = 'id' ] && continue
  [ -z "${extra:-}" ] || { echo "失败：profile 字段数错误：$id" >&2; exit 1; }
  [ -n "$id" ] && [ -n "$selector" ] || continue
  matches=()
  while IFS=$'\t' read -r sha bytes kind path; do
    [ "$sha" = 'sha256' ] && continue
    [ "$kind" = 'glb' ] || continue
    base=$(basename "$path")
    base=${base%.*}
    normalized=$(printf '%s' "$base" | tr '[:upper:] ' '[:lower:]_' | tr -cd 'a-z0-9_-')
    [ "$normalized" = "$selector" ] && matches+=("$path")
  done < "$INVENTORY"
  [ "${#matches[@]}" -eq 1 ] || {
    echo "失败：profile selector 必须唯一命中一个 GLB：$selector（命中 ${#matches[@]}）" >&2
    exit 1
  }
  printf '%s\t%s\t%s\t%s\t%s\n' "$id" "${matches[0]}" "$category" "$target" "$yaw" >> "$TMP"
  count=$((count + 1))
done < "$PROFILE"
[ "$count" -ge 3 ] || { echo "失败：profile 少于 3 项。" >&2; exit 1; }

mv "$TMP" "$MANIFEST"
echo "R3.1 selection profile 已应用：$PROFILE -> $MANIFEST（$count 项）"
