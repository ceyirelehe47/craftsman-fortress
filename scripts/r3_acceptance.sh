#!/usr/bin/env bash
# R3.1 单一验收入口：R1/R2.1/R3 回归 + 来源链/字节身份/授权边界封版。
set -euo pipefail
cd "$(dirname "$0")/.."
export LC_ALL=C

MANIFEST="${R3_MANIFEST:-assets/vendor_local/voxel_survival_pack/v1.0/free_sample/selection.tsv}"
LOCAL_DIR=$(dirname "$MANIFEST")
LOCK="${R3_LOCK:-$LOCAL_DIR/selection.lock.tsv}"
INVENTORY="$LOCAL_DIR/inventory.tsv"
EVIDENCE_DIR="${1:-evidence_r3_1_$(date +%Y%m%d_%H%M%S)}"
WATCHDOG_SEC="${R3_ASSET_WATCHDOG_SEC:-420}"

[ "${R3_SOURCE_CHAIN_FIXTURE:-0}" = '0' ] || {
  echo "失败：正式 R3.1 验收禁止 R3_SOURCE_CHAIN_FIXTURE。" >&2
  exit 1
}

if [ -n "$(git status --porcelain)" ]; then
  echo "失败：工作区必须干净。" >&2
  git status --porcelain >&2
  exit 1
fi
for file in "$MANIFEST" "$LOCK" "$INVENTORY" "$LOCAL_DIR/source_receipt.txt" "$LOCAL_DIR/acquisition_receipt.txt"; do
  [ -s "$file" ] || { echo "失败：缺少 R3.1 本地来源链文件：$file" >&2; exit 1; }
done
if [ -e "$EVIDENCE_DIR" ] && [ -n "$(ls -A "$EVIDENCE_DIR" 2>/dev/null)" ]; then
  echo "失败：证据目录已存在且非空：$EVIDENCE_DIR" >&2
  exit 1
fi
mkdir -p "$EVIDENCE_DIR"
git check-ignore -q "$EVIDENCE_DIR" || {
  echo "失败：证据目录必须被 .gitignore 忽略。" >&2
  exit 1
}
SHA=$(git rev-parse HEAD)
printf '%s\n' "$SHA" > "$EVIDENCE_DIR/commit.txt"

SCRATCH=""
on_exit() {
  code=$?
  if [ -n "$SCRATCH" ] && [ -d "$SCRATCH" ]; then rm -rf "$SCRATCH" || true; fi
  if [ "$code" -ne 0 ] && [ -d "$EVIDENCE_DIR" ]; then
    printf '%s\n' "$code" > "$EVIDENCE_DIR/last_exit_code.txt" || true
    find "$EVIDENCE_DIR" -type f -print0 2>/dev/null \
      | xargs -0 sha256sum > "$EVIDENCE_DIR/SHA256SUMS.partial.txt" 2>/dev/null || true
  fi
  exit "$code"
}
trap on_exit EXIT
step() { printf '\n===== %s =====\n' "$1"; }
now_s() { date +%s; }

run_watchdog() {
  label="$1"; log="$2"; shift 2
  "$@" > "$log" 2>&1 &
  pid=$!
  deadline=$(( $(now_s) + WATCHDOG_SEC ))
  while kill -0 "$pid" 2>/dev/null; do
    if [ "$(now_s)" -ge "$deadline" ]; then
      {
        echo "$label exceeded ${WATCHDOG_SEC}s"
        echo "command: $*"
        echo "time: $(date -Iseconds)"
        tail -n 100 "$log" 2>/dev/null || true
      } > "$EVIDENCE_DIR/r3_1_watchdog.txt"
      kill "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
      return 124
    fi
    sleep 2
  done
  code=0
  wait "$pid" || code=$?
  return "$code"
}

step "R3.1 static, source-chain and redistribution checks"
bash -n scripts/*.sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --release
cargo build --release
bash scripts/audit_bevy.sh
bash scripts/check_naming.sh
cargo run --release --example r3_manifest_check -- "$MANIFEST" --check-files assets \
  | tee "$EVIDENCE_DIR/manifest_check.log"
mkdir -p "$EVIDENCE_DIR/source_chain"
R3_SOURCE_ARCHIVE="${R3_SOURCE_ARCHIVE:-third_party_raw/Free_Sample.rar}" \
  bash scripts/r3_verify_local_assets.sh "$MANIFEST" "$LOCK" assets "$EVIDENCE_DIR/source_chain" \
  | tee "$EVIDENCE_DIR/source_chain/verify_stdout.log"
ASSET_SET_SHA=$(sed -n 's/^asset_set_sha256=//p' "$EVIDENCE_DIR/source_chain/source_chain_report.txt")
[ ${#ASSET_SET_SHA} -eq 64 ] || { echo "失败：未取得 asset_set_sha256。" >&2; exit 1; }
bash scripts/check_vendor_assets.sh --manifest "$MANIFEST" --inventory "$INVENTORY"

for source_file in source_receipt.txt acquisition_receipt.txt inventory.tsv; do
  [ -s "$LOCAL_DIR/$source_file" ] || { echo "失败：本地元数据缺失：$source_file" >&2; exit 1; }
  cp "$LOCAL_DIR/$source_file" "$EVIDENCE_DIR/$source_file"
done
cp "$LOCK" "$EVIDENCE_DIR/selection.lock.tsv"
cp "$MANIFEST" "$EVIDENCE_DIR/manifest_snapshot.tsv"

step "R2.1 regression acceptance"
ACCEPTANCE_WATCHDOG_SEC="${ACCEPTANCE_WATCHDOG_SEC:-1500}" \
R2_HEADLESS_WATCHDOG_SEC="${R2_HEADLESS_WATCHDOG_SEC:-300}" \
  bash scripts/r2_acceptance.sh "$EVIDENCE_DIR/r2_1"
grep -Eq '"overall"[[:space:]]*:[[:space:]]*"PASS"' "$EVIDENCE_DIR/r2_1/report.json" || {
  echo "失败：R2.1 回归报告不是 PASS。" >&2; exit 1;
}

step "R3 visual/scale regression and lifecycle A/B"
cargo build --release --example r3_asset_lab
LAB="target/release/examples/r3_asset_lab"
[ -x "$LAB.exe" ] && LAB="$LAB.exe"
SCRATCH=$(mktemp -d)
for run in a b; do
  out="$EVIDENCE_DIR/lab_$run"
  mkdir -p "$out"; rmdir "$out"
  code=0
  run_watchdog "r3_asset_lab_$run" "$SCRATCH/lab_${run}.log" \
    "$LAB" --manifest "$MANIFEST" --evidence "$out" || code=$?
  mkdir -p "$out"
  cp "$SCRATCH/lab_${run}.log" "$out/app_stdout.log"
  [ "$code" -eq 0 ] || { cat "$out/app_stdout.log" >&2; exit "$code"; }
  grep -Eq '"overall"[[:space:]]*:[[:space:]]*"PASS"' "$out/asset_report.json" || {
    echo "失败：R3 lab_$run 未通过。" >&2; exit 1;
  }
  hits=$(grep -nE 'panic|ERROR|FATAL|B0004' "$out/app_stdout.log" || true)
  if [ -n "$hits" ]; then
    echo "失败：R3 lab_$run 日志包含错误或层级警告：" >&2
    printf '%s\n' "$hits" | head -20 >&2
    exit 1
  fi
done

hash_a=$(grep -o '"manifest_hash":"[^"]*"' "$EVIDENCE_DIR/lab_a/asset_report.json" | head -1 | cut -d'"' -f4)
hash_b=$(grep -o '"manifest_hash":"[^"]*"' "$EVIDENCE_DIR/lab_b/asset_report.json" | head -1 | cut -d'"' -f4)
fp_a=$(grep -o '"transform_fingerprint":"[^"]*"' "$EVIDENCE_DIR/lab_a/asset_report.json" | head -1 | cut -d'"' -f4)
fp_b=$(grep -o '"transform_fingerprint":"[^"]*"' "$EVIDENCE_DIR/lab_b/asset_report.json" | head -1 | cut -d'"' -f4)
[ -n "$hash_a" ] && [ "$hash_a" = "$hash_b" ] || {
  echo "失败：两次运行 manifest hash 不一致：$hash_a / $hash_b" >&2; exit 1;
}
[ -n "$fp_a" ] && [ "$fp_a" = "$fp_b" ] || {
  echo "失败：两次运行 Transform 指纹不一致：$fp_a / $fp_b" >&2; exit 1;
}

bash scripts/check_vendor_assets.sh --manifest "$MANIFEST" --inventory "$INVENTORY" "$EVIDENCE_DIR"

cat > "$EVIDENCE_DIR/report.json" <<JSON
{
  "commit":"$SHA",
  "overall":"PASS",
  "r2_1":"PASS",
  "r3":"PASS",
  "r3_1":"PASS",
  "manifest_hash":"$hash_a",
  "transform_fingerprint":"$fp_a",
  "asset_set_sha256":"$ASSET_SET_SHA",
  "checks":[
    {"id":"D01","status":"PASS","name":"autonomous private-cache acquisition receipt"},
    {"id":"D02","status":"PASS","name":"approved archive hash and byte count"},
    {"id":"D03","status":"PASS","name":"stable license receipt bound to manifest"},
    {"id":"D04","status":"PASS","name":"inventory hash and every local file verified"},
    {"id":"D05","status":"PASS","name":"selection lock and asset-set identity"},
    {"id":"D06","status":"PASS","name":"reviewer-ready independent acquisition workflow"},
    {"id":"D07","status":"PASS","name":"model texture archive and nested-zip leakage guard"},
    {"id":"D08","status":"PASS","name":"Bevy hierarchy visibility complete; no B0004"},
    {"id":"D09","status":"PASS","name":"every lifecycle cycle returns to baseline after deferred despawn"},
    {"id":"D10","status":"PASS","name":"dual-run manifest transform and asset-set determinism"},
    {"id":"D11","status":"PASS","name":"R1 R2.1 R3 regression; CFSAVE02 unchanged"},
    {"id":"D12","status":"PASS","name":"evidence ready for isolated reviewer and release scan"}
  ]
}
JSON
cat > "$EVIDENCE_DIR/report.md" <<MD
# R3.1 source-chain and redistribution-boundary acceptance

- commit: \`$SHA\`
- R2.1 full regression: **PASS**
- R3 visual/scale regression: **PASS**
- source/license/inventory/selection-lock chain: **PASS**
- manifest hash: \`$hash_a\`
- transform fingerprint: \`$fp_a\`
- asset set SHA-256: \`$ASSET_SET_SHA\`
- D01-D12: **PASS**
- B0004 hierarchy warnings: **0**
- third-party model/texture/archive bytes included in evidence: **NO**
MD

find "$EVIDENCE_DIR" -type f ! -name SHA256SUMS.txt -print0 \
  | xargs -0 sha256sum > "$EVIDENCE_DIR/SHA256SUMS.txt"
rm -f "$EVIDENCE_DIR/SHA256SUMS.partial.txt" "$EVIDENCE_DIR/last_exit_code.txt"
ZIP_FILE="${EVIDENCE_DIR}.zip"
rm -f "$ZIP_FILE"
if command -v zip >/dev/null 2>&1; then
  zip -qr "$ZIP_FILE" "$EVIDENCE_DIR"
elif command -v powershell >/dev/null 2>&1; then
  powershell -NoProfile -Command \
    "Compress-Archive -Path '$EVIDENCE_DIR' -DestinationPath '$ZIP_FILE'" >/dev/null
else
  echo "失败：无法生成证据 ZIP。" >&2; exit 1
fi
[ -s "$ZIP_FILE" ] || { echo "失败：证据 ZIP 未生成。" >&2; exit 1; }
bash scripts/check_vendor_assets.sh --manifest "$MANIFEST" --inventory "$INVENTORY" \
  "$EVIDENCE_DIR" "$ZIP_FILE"
sha256sum "$ZIP_FILE" >> "$EVIDENCE_DIR/SHA256SUMS.txt"
echo "R3.1 acceptance PASS: $EVIDENCE_DIR"
echo "evidence package: $ZIP_FILE"
