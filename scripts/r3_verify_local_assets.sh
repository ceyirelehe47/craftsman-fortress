#!/usr/bin/env bash
# R3.1 来源链验证：archive → acquisition receipt → stable license receipt → inventory → selection lock → actual files。
set -euo pipefail
cd "$(dirname "$0")/.."
export LC_ALL=C

MANIFEST="${1:?usage: r3_verify_local_assets.sh <selection.tsv> <selection.lock.tsv> [assets-root] [report-dir]}"
LOCK="${2:?missing selection.lock.tsv}"
ASSET_ROOT="${3:-assets}"
REPORT_DIR="${4:-}"
LOCAL_DIR=$(dirname "$MANIFEST")
INVENTORY="$LOCAL_DIR/inventory.tsv"
SOURCE_RECEIPT="$LOCAL_DIR/source_receipt.txt"
ACQUISITION_RECEIPT="$LOCAL_DIR/acquisition_receipt.txt"
ARCHIVE="${R3_SOURCE_ARCHIVE:-third_party_raw/Free_Sample.rar}"
EXPECTED_ARCHIVE_SHA256='ababc51f543ec06d07e68d95cdcc90d8ae878d6d12908dd90a0746a836e82fed'
EXPECTED_ARCHIVE_BYTES='1913239'
EXPECTED_APPROVED_AT='2026-09-14T03:02:52Z'
EXPECTED_SOURCE_URL='https://palmstudio.itch.io/voxel-survival-pack'

for file in "$MANIFEST" "$LOCK" "$INVENTORY" "$SOURCE_RECEIPT" "$ACQUISITION_RECEIPT" "$ARCHIVE"; do
  [ -s "$file" ] || { echo "失败：来源链缺少 $file" >&2; exit 1; }
done

selection_rows=$(mktemp)
lock_rows=$(mktemp)
inventory_paths=$(mktemp)
cleanup() { rm -f "$selection_rows" "$lock_rows" "$inventory_paths"; }
trap cleanup EXIT

meta() {
  key="$1"; file="$2"
  sed -n "s/^# ${key}=//p; s/^#${key}=//p" "$file" | head -1 | tr -d '\r'
}
kv() {
  key="$1"; file="$2"
  sed -n "s/^${key}=//p" "$file" | head -1 | tr -d '\r'
}
valid_sha() {
  value="$1"
  [ ${#value} -eq 64 ] && ! printf '%s' "$value" | grep -Eq '[^0-9a-f]'
}

MANIFEST_ARCHIVE=$(meta archive_sha256 "$MANIFEST")
MANIFEST_RECEIPT=$(meta license_receipt_sha256 "$MANIFEST")
LOCK_ARCHIVE=$(meta archive_sha256 "$LOCK")
LOCK_RECEIPT=$(meta license_receipt_sha256 "$LOCK")
LOCK_INVENTORY=$(meta inventory_sha256 "$LOCK")
LOCK_SELECTION=$(meta selection_sha256 "$LOCK")
ACTUAL_ARCHIVE=$(sha256sum "$ARCHIVE" | awk '{print $1}')
ACTUAL_ARCHIVE_BYTES=$(wc -c < "$ARCHIVE" | tr -d ' ')
ACTUAL_RECEIPT=$(sha256sum "$SOURCE_RECEIPT" | awk '{print $1}')
ACTUAL_INVENTORY=$(sha256sum "$INVENTORY" | awk '{print $1}')
ACTUAL_SELECTION=$(sha256sum "$MANIFEST" | awk '{print $1}')
ACQ_ARCHIVE=$(kv archive_sha256 "$ACQUISITION_RECEIPT")
ACQ_BYTES=$(kv archive_bytes "$ACQUISITION_RECEIPT")
ACQ_INTERACTION=$(kv user_interaction "$ACQUISITION_RECEIPT")
ACQ_KIND=$(kv source_kind "$ACQUISITION_RECEIPT")
MANIFEST_SOURCE=$(meta source_url "$MANIFEST")
MANIFEST_PACK=$(meta pack_version "$MANIFEST")
MANIFEST_SAMPLE=$(meta sample_file "$MANIFEST")
MANIFEST_APPROVED_AT=$(meta acquired_at "$MANIFEST")
LOCK_FORMAT=$(meta format "$LOCK")
LOCK_SOURCE=$(meta source_url "$LOCK")
LOCK_PACK=$(meta pack_version "$LOCK")
LOCK_SAMPLE=$(meta sample_file "$LOCK")
RECEIPT_SOURCE=$(kv source_url "$SOURCE_RECEIPT")
RECEIPT_ARCHIVE=$(kv source_archive_sha256 "$SOURCE_RECEIPT")
RECEIPT_BYTES=$(kv source_archive_bytes "$SOURCE_RECEIPT")
RECEIPT_APPROVED_AT=$(kv approved_archive_observed_at "$SOURCE_RECEIPT")
RECEIPT_REDISTRIBUTION=$(kv license_redistribution "$SOURCE_RECEIPT")

for value in "$MANIFEST_ARCHIVE" "$MANIFEST_RECEIPT" "$LOCK_ARCHIVE" "$LOCK_RECEIPT" \
  "$LOCK_INVENTORY" "$LOCK_SELECTION" "$ACTUAL_ARCHIVE" "$ACTUAL_RECEIPT" \
  "$ACTUAL_INVENTORY" "$ACTUAL_SELECTION" "$ACQ_ARCHIVE"; do
  valid_sha "$value" || { echo "失败：来源链包含非法 SHA-256：$value" >&2; exit 1; }
done

for pair in \
  "$MANIFEST_ARCHIVE:$ACTUAL_ARCHIVE:manifest/archive" \
  "$LOCK_ARCHIVE:$ACTUAL_ARCHIVE:lock/archive" \
  "$ACQ_ARCHIVE:$ACTUAL_ARCHIVE:acquisition/archive" \
  "$MANIFEST_RECEIPT:$ACTUAL_RECEIPT:manifest/license-receipt" \
  "$LOCK_RECEIPT:$ACTUAL_RECEIPT:lock/license-receipt" \
  "$LOCK_INVENTORY:$ACTUAL_INVENTORY:lock/inventory" \
  "$LOCK_SELECTION:$ACTUAL_SELECTION:lock/selection"; do
  IFS=: read -r expected actual label <<< "$pair"
  [ "$expected" = "$actual" ] || {
    echo "失败：$label SHA-256 不一致：expected=$expected actual=$actual" >&2
    exit 1
  }
done

if [ "${R3_SOURCE_CHAIN_FIXTURE:-0}" != '1' ]; then
  [ "$ACTUAL_ARCHIVE" = "$EXPECTED_ARCHIVE_SHA256" ] \
    && [ "$ACTUAL_ARCHIVE_BYTES" = "$EXPECTED_ARCHIVE_BYTES" ] || {
    echo "失败：实际原包不是已批准的 PalmStudio 免费样本字节。" >&2; exit 1;
  }
fi
[ "$ACQ_BYTES" = "$ACTUAL_ARCHIVE_BYTES" ] || {
  echo "失败：acquisition receipt 的 archive_bytes 不匹配。" >&2; exit 1;
}
[ "$ACQ_INTERACTION" = 'none' ] || {
  echo "失败：素材取得记录不是 user_interaction=none。" >&2; exit 1;
}
case "$ACQ_KIND" in
  private_cache|private_url) ;;
  *) echo "失败：正式验收只接受 private_cache/private_url，自主取得记录为：$ACQ_KIND" >&2; exit 1 ;;
esac
[ "$MANIFEST_SOURCE" = "$EXPECTED_SOURCE_URL" ] \
  && [ "$LOCK_SOURCE" = "$EXPECTED_SOURCE_URL" ] \
  && [ "$RECEIPT_SOURCE" = "$EXPECTED_SOURCE_URL" ] || {
  echo "失败：来源 URL 链不一致。" >&2; exit 1;
}
[ "$MANIFEST_PACK" = 'v1.0' ] && [ "$LOCK_PACK" = 'v1.0' ] \
  && [ "$MANIFEST_SAMPLE" = 'Free_Sample.rar' ] \
  && [ "$LOCK_SAMPLE" = 'Free_Sample.rar' ] || {
  echo "失败：pack/sample 元数据链不一致。" >&2; exit 1;
}
[ "$LOCK_FORMAT" = 'r3-selection-lock-v1' ] || {
  echo "失败：selection lock 格式未知：$LOCK_FORMAT" >&2; exit 1;
}
[ "$MANIFEST_APPROVED_AT" = "$EXPECTED_APPROVED_AT" ] \
  && [ "$RECEIPT_APPROVED_AT" = "$EXPECTED_APPROVED_AT" ] || {
  echo "失败：批准原包观察时间不一致。" >&2; exit 1;
}
[ "$RECEIPT_ARCHIVE" = "$ACTUAL_ARCHIVE" ] \
  && [ "$RECEIPT_BYTES" = "$ACTUAL_ARCHIVE_BYTES" ] || {
  echo "失败：稳定来源收据与原包身份不一致。" >&2; exit 1;
}
[ "$RECEIPT_REDISTRIBUTION" = 'prohibited_without_explicit_permission' ] || {
  echo "失败：稳定授权收据缺少禁止再分发边界。" >&2; exit 1;
}

[ "$(head -1 "$INVENTORY" | tr -d '\r')" = $'sha256\tbytes\tkind\tpath' ] || {
  echo "失败：inventory header 不正确。" >&2; exit 1;
}
: > "$inventory_paths"
inventory_count=0
previous_inventory_path=''
while IFS=$'\t' read -r expected_sha expected_bytes kind path extra; do
  [ "$expected_sha" = 'sha256' ] && continue
  [ -z "${extra:-}" ] || { echo "失败：inventory 字段数错误：$path" >&2; exit 1; }
  valid_sha "$expected_sha" || { echo "失败：inventory SHA 非法：$path" >&2; exit 1; }
  [[ "$expected_bytes" =~ ^[0-9]+$ ]] && [ "$expected_bytes" -gt 0 ] || {
    echo "失败：inventory bytes 非法：$path" >&2; exit 1;
  }
  case "$kind" in glb|png) ;; *) echo "失败：inventory kind 非法：$kind" >&2; exit 1;; esac
  case "$path" in vendor_local/voxel_survival_pack/*) ;; *) echo "失败：inventory 路径越界：$path" >&2; exit 1;; esac
  case "$path" in *'..'*) echo "失败：inventory 路径含 ..：$path" >&2; exit 1;; esac
  if [ -n "$previous_inventory_path" ] && [[ "$path" < "$previous_inventory_path" || "$path" = "$previous_inventory_path" ]]; then
    echo "失败：inventory 路径必须严格递增且唯一：$previous_inventory_path / $path" >&2
    exit 1
  fi
  previous_inventory_path="$path"
  printf '%s\n' "$path" >> "$inventory_paths"
  file="$ASSET_ROOT/$path"
  [ -f "$file" ] || { echo "失败：inventory 文件缺失：$file" >&2; exit 1; }
  actual_sha=$(sha256sum "$file" | awk '{print $1}')
  actual_bytes=$(wc -c < "$file" | tr -d ' ')
  [ "$actual_sha" = "$expected_sha" ] && [ "$actual_bytes" = "$expected_bytes" ] || {
    echo "失败：inventory 文件内容不匹配：$path" >&2; exit 1;
  }
  inventory_count=$((inventory_count + 1))
done < "$INVENTORY"
[ "$inventory_count" -gt 0 ] || { echo "失败：inventory 为空。" >&2; exit 1; }

[ "$(grep -v '^#' "$MANIFEST" | head -1 | tr -d '\r')" = $'id\tpath\tcategory\ttarget_height_m\tyaw_deg' ] || {
  echo "失败：selection header 不正确。" >&2; exit 1;
}
[ "$(grep -v '^#' "$LOCK" | head -1 | tr -d '\r')" = $'id\tpath\tfile_sha256\tfile_bytes\tcategory\ttarget_height_m\tyaw_deg' ] || {
  echo "失败：selection lock header 不正确。" >&2; exit 1;
}
tail -n +2 < <(grep -v '^#' "$MANIFEST") | tr -d '\r' > "$selection_rows"
tail -n +2 < <(grep -v '^#' "$LOCK") | tr -d '\r' > "$lock_rows"
selected_count=0
while IFS=$'\t' read -r id path file_sha file_bytes category target yaw extra; do
  [ -z "${extra:-}" ] || { echo "失败：lock 字段数错误：$id" >&2; exit 1; }
  [ -n "$id" ] || continue
  valid_sha "$file_sha" || { echo "失败：lock SHA 非法：$id" >&2; exit 1; }
  [[ "$file_bytes" =~ ^[0-9]+$ ]] && [ "$file_bytes" -gt 0 ] || {
    echo "失败：lock bytes 非法：$id" >&2; exit 1;
  }
  selection_line=$(awk -F '\t' -v wanted="$id" '$1==wanted {print; found++} END{if(found!=1) exit 4}' "$selection_rows") || {
    echo "失败：selection 中无法唯一找到 id=$id" >&2; exit 1;
  }
  IFS=$'\t' read -r sid spath scategory starget syaw <<< "$selection_line"
  [ "$path" = "$spath" ] && [ "$category" = "$scategory" ] \
    && [ "$target" = "$starget" ] && [ "$yaw" = "$syaw" ] || {
      echo "失败：lock 与 selection 配置不一致：$id" >&2; exit 1;
    }
  inventory_line=$(awk -F '\t' -v p="$path" 'NR>1 && $4==p {print; found++} END{if(found!=1) exit 5}' "$INVENTORY") || {
    echo "失败：lock 路径未唯一绑定 inventory：$path" >&2; exit 1;
  }
  IFS=$'\t' read -r isha ibytes ikind ipath <<< "$inventory_line"
  [ "$file_sha" = "$isha" ] && [ "$file_bytes" = "$ibytes" ] && [ "$ikind" = 'glb' ] || {
    echo "失败：lock 字节身份与 inventory 不一致：$id" >&2; exit 1;
  }
  selected_count=$((selected_count + 1))
done < "$lock_rows"
[ "$selected_count" -ge 3 ] || { echo "失败：selection lock 少于 3 项。" >&2; exit 1; }
selection_count=$(wc -l < "$selection_rows" | tr -d ' ')
[ "$selected_count" = "$selection_count" ] || {
  echo "失败：selection 与 lock 项数不一致。" >&2; exit 1;
}

ASSET_SET_SHA=$(sha256sum "$LOCK" | awk '{print $1}')
if [ -n "$REPORT_DIR" ]; then
  mkdir -p "$REPORT_DIR"
  cat > "$REPORT_DIR/source_chain_report.json" <<JSON
{
  "overall":"PASS",
  "archive_sha256":"$ACTUAL_ARCHIVE",
  "archive_bytes":$ACTUAL_ARCHIVE_BYTES,
  "license_receipt_sha256":"$ACTUAL_RECEIPT",
  "inventory_sha256":"$ACTUAL_INVENTORY",
  "selection_sha256":"$ACTUAL_SELECTION",
  "asset_set_sha256":"$ASSET_SET_SHA",
  "inventory_files":$inventory_count,
  "selected_assets":$selected_count,
  "source_kind":"$ACQ_KIND",
  "user_interaction":"none"
}
JSON
  cat > "$REPORT_DIR/source_chain_report.txt" <<TXT
source_chain=PASS
archive_sha256=$ACTUAL_ARCHIVE
archive_bytes=$ACTUAL_ARCHIVE_BYTES
license_receipt_sha256=$ACTUAL_RECEIPT
inventory_sha256=$ACTUAL_INVENTORY
selection_sha256=$ACTUAL_SELECTION
asset_set_sha256=$ASSET_SET_SHA
inventory_files=$inventory_count
selected_assets=$selected_count
source_kind=$ACQ_KIND
user_interaction=none
TXT
fi

echo "R3.1 来源链验证通过。"
echo "asset_set_sha256=$ASSET_SET_SHA"
echo "inventory_files=$inventory_count"
echo "selected_assets=$selected_count"
