#!/usr/bin/env bash
# 从受控缓存/URL 自主取得素材、准备稳定清单与字节锁，然后运行完整 R3.1 验收。
set -euo pipefail
cd "$(dirname "$0")/.."
export LC_ALL=C

EVIDENCE_DIR="${1:-evidence_r3_1_$(date +%Y%m%d_%H%M%S)}"
ARCHIVE="third_party_raw/Free_Sample.rar"
DEST="assets/vendor_local/voxel_survival_pack/v1.0/free_sample"

bash scripts/r3_acquire_sample.sh "$ARCHIVE"
R3_REPLACE=1 bash scripts/r3_prepare_sample.sh "$ARCHIVE" "$DEST"
R3_SOURCE_ARCHIVE="$ARCHIVE" \
R3_MANIFEST="$DEST/selection.tsv" \
R3_LOCK="$DEST/selection.lock.tsv" \
  bash scripts/r3_acceptance.sh "$EVIDENCE_DIR"
