#!/usr/bin/env bash
# CI 夹具：不含真实 PalmStudio 字节，验证 R3.1 来源链和泄漏守卫的失败语义。
set -euo pipefail
cd "$(dirname "$0")/.."
export LC_ALL=C

# Windows 可能把 python3 解析到微软商店占位 stub（运行即失败）；
# 逐个探测可真正执行的解释器，CI 的 python3 优先。
PY=''
for candidate in python3 python; do
  if command -v "$candidate" >/dev/null 2>&1 \
    && "$candidate" -c 'import sys' >/dev/null 2>&1; then
    PY="$candidate"
    break
  fi
done
[ -n "$PY" ] || { echo "失败：来源链夹具需要可用的 python3 或 python。" >&2; exit 1; }

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
ASSETS="$TMP/assets"
LOCAL="$ASSETS/vendor_local/voxel_survival_pack/v1.0/free_sample"
RAW="$TMP/third_party_raw"
mkdir -p "$LOCAL" "$RAW"
printf 'fixture-glb-a\n' > "$LOCAL/a.glb"
printf 'fixture-glb-b\n' > "$LOCAL/b.glb"
printf 'fixture-glb-a\n' > "$LOCAL/c.glb"
printf 'fixture-texture\n' > "$LOCAL/texture.png"
printf 'fixture-archive\n' > "$RAW/Free_Sample.rar"
ARCHIVE_SHA=$(sha256sum "$RAW/Free_Sample.rar" | awk '{print $1}')
ARCHIVE_BYTES=$(wc -c < "$RAW/Free_Sample.rar" | tr -d ' ')

cat > "$RAW/acquisition_receipt.txt" <<RECEIPT
source_kind=private_cache
source_locator_id=$(printf fixture | sha256sum | awk '{print $1}')
archive_name=Free_Sample.rar
archive_sha256=$ARCHIVE_SHA
archive_bytes=$ARCHIVE_BYTES
acquired_at=2026-09-14T00:00:00Z
user_interaction=none
RECEIPT
cp "$RAW/acquisition_receipt.txt" "$LOCAL/acquisition_receipt.txt"
cat > "$LOCAL/source_receipt.txt" <<RECEIPT
source_url=https://palmstudio.itch.io/voxel-survival-pack
pack_name=Voxel Survival Pack
publisher=PalmStudio
pack_version=v1.0
sample_file=Free_Sample.rar
source_archive_name=Free_Sample.rar
source_archive_sha256=$ARCHIVE_SHA
source_archive_bytes=$ARCHIVE_BYTES
license_snapshot_id=palmstudio-vsp-v1.0-observed-2026-09-14
approved_archive_observed_at=2026-09-14T03:02:52Z
license_personal_use=allowed
license_commercial_use=allowed
license_modification=allowed
license_attribution=appreciated_not_required
license_redistribution=prohibited_without_explicit_permission
license_reselling=prohibited_without_explicit_permission
license_repackaging=prohibited_without_explicit_permission
RECEIPT
RECEIPT_SHA=$(sha256sum "$LOCAL/source_receipt.txt" | awk '{print $1}')

{
  printf 'sha256\tbytes\tkind\tpath\n'
  while IFS= read -r file; do
    rel=${file#"$ASSETS"/}
    sha=$(sha256sum "$file" | awk '{print $1}')
    bytes=$(wc -c < "$file" | tr -d ' ')
    kind=${file##*.}
    printf '%s\t%s\t%s\t%s\n' "$sha" "$bytes" "$kind" "$rel"
  done < <(printf '%s\n' "$LOCAL/a.glb" "$LOCAL/b.glb" "$LOCAL/c.glb" "$LOCAL/texture.png" | sort)
} > "$LOCAL/inventory.tsv"

cat > "$LOCAL/selection.tsv" <<MANIFEST
# source_url=https://palmstudio.itch.io/voxel-survival-pack
# pack_version=v1.0
# sample_file=Free_Sample.rar
# archive_sha256=$ARCHIVE_SHA
# license_receipt_sha256=$RECEIPT_SHA
# acquired_at=2026-09-14T03:02:52Z
id	path	category	target_height_m	yaw_deg
placeholder	vendor_local/voxel_survival_pack/v1.0/free_sample/a.glb	unknown	1.0	0
MANIFEST
cat > "$TMP/profile.tsv" <<PROFILE
id	basename	category	target_height_m	yaw_deg
fixture_a	a	prop	1.0	0
fixture_b	b	tool	0.8	90
fixture_c	c	environment	2.0	180
PROFILE
bash scripts/r3_apply_selection_profile.sh "$LOCAL/selection.tsv" "$TMP/profile.tsv"

bash scripts/r3_lock_selection.sh "$LOCAL/selection.tsv" "$LOCAL/selection.lock.tsv"
R3_SOURCE_CHAIN_FIXTURE=1 R3_SOURCE_ARCHIVE="$RAW/Free_Sample.rar" \
  bash scripts/r3_verify_local_assets.sh \
  "$LOCAL/selection.tsv" "$LOCAL/selection.lock.tsv" "$ASSETS" "$TMP/report"

# 内容篡改必须失败。
cp "$LOCAL/a.glb" "$TMP/a.backup"
printf 'tampered\n' >> "$LOCAL/a.glb"
if R3_SOURCE_CHAIN_FIXTURE=1 R3_SOURCE_ARCHIVE="$RAW/Free_Sample.rar" \
  bash scripts/r3_verify_local_assets.sh \
  "$LOCAL/selection.tsv" "$LOCAL/selection.lock.tsv" "$ASSETS" >/dev/null 2>&1; then
  echo "失败：篡改 GLB 后来源链仍通过。" >&2; exit 1
fi
mv "$TMP/a.backup" "$LOCAL/a.glb"

# 授权收据篡改必须失败。
cp "$LOCAL/source_receipt.txt" "$TMP/receipt.backup"
printf 'tampered=true\n' >> "$LOCAL/source_receipt.txt"
if R3_SOURCE_CHAIN_FIXTURE=1 R3_SOURCE_ARCHIVE="$RAW/Free_Sample.rar" \
  bash scripts/r3_verify_local_assets.sh \
  "$LOCAL/selection.tsv" "$LOCAL/selection.lock.tsv" "$ASSETS" >/dev/null 2>&1; then
  echo "失败：篡改授权收据后来源链仍通过。" >&2; exit 1
fi
mv "$TMP/receipt.backup" "$LOCAL/source_receipt.txt"

# 顶层 ZIP：独立截图可通过，改名后的第三方贴图必须失败。
mkdir -p "$TMP/clean" "$TMP/leak"
printf 'independent screenshot bytes\n' > "$TMP/clean/screenshot.png"
cp "$LOCAL/texture.png" "$TMP/leak/renamed_screenshot.png"
"$PY" - "$TMP/clean.zip" "$TMP/clean" "$TMP/leak.zip" "$TMP/leak" <<'PY_TOP'
import pathlib, sys, zipfile
for zip_name, directory in ((sys.argv[1], sys.argv[2]), (sys.argv[3], sys.argv[4])):
    root = pathlib.Path(directory)
    with zipfile.ZipFile(zip_name, 'w') as z:
        for p in root.rglob('*'):
            if p.is_file():
                z.write(p, p.relative_to(root))
PY_TOP
bash scripts/check_vendor_assets.sh --manifest "$LOCAL/selection.tsv" \
  --inventory "$LOCAL/inventory.tsv" "$TMP/clean.zip"
if bash scripts/check_vendor_assets.sh --manifest "$LOCAL/selection.tsv" \
  --inventory "$LOCAL/inventory.tsv" "$TMP/leak.zip" >/dev/null 2>&1; then
  echo "失败：改名后的第三方 PNG 泄漏未被内容哈希拦截。" >&2; exit 1
fi

# 嵌套 ZIP 中的改名贴图也必须失败。
mkdir -p "$TMP/inner_leak" "$TMP/outer"
cp "$LOCAL/texture.png" "$TMP/inner_leak/presentation.png"
"$PY" - "$TMP/outer/inner.zip" "$TMP/inner_leak" "$TMP/nested_leak.zip" "$TMP/outer" <<'PY_NESTED'
import pathlib, sys, zipfile
for zip_name, directory in ((sys.argv[1], sys.argv[2]), (sys.argv[3], sys.argv[4])):
    root = pathlib.Path(directory)
    with zipfile.ZipFile(zip_name, 'w') as z:
        for p in root.rglob('*'):
            if p.is_file():
                z.write(p, p.relative_to(root))
PY_NESTED
if bash scripts/check_vendor_assets.sh --manifest "$LOCAL/selection.tsv" \
  --inventory "$LOCAL/inventory.tsv" "$TMP/nested_leak.zip" >/dev/null 2>&1; then
  echo "失败：嵌套 ZIP 内的第三方 PNG 泄漏未被拦截。" >&2; exit 1
fi

echo "R3.1 source-chain fixture PASS"
