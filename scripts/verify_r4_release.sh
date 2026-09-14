#!/usr/bin/env bash
# 下载复验 R4 标签、证据提交、E01-E12、校验和与第三方素材泄漏。
set -euo pipefail
cd "$(dirname "$0")/.."
export LC_ALL=C

TAG="${1:?usage: verify_r4_release.sh <tag> <selection.tsv> <inventory.tsv>}"
MANIFEST="${2:?missing selection.tsv}"
INVENTORY="${3:?missing inventory.tsv}"
REPO="${GITHUB_REPOSITORY:-ceyirelehe47/craftsman-fortress}"
command -v gh >/dev/null 2>&1 || { echo "失败：需要 gh CLI。" >&2; exit 1; }
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

git fetch --tags --force >/dev/null
TARGET=$(git rev-list -n 1 "$TAG")
case "$TAG" in r4-baseline-*) ;; *) echo "失败：标签名不是 r4-baseline-*" >&2; exit 1;; esac
SHORT=${TAG#r4-baseline-}
[ "${TARGET:0:12}" = "$SHORT" ] || {
  echo "失败：标签短 SHA 与目标不一致：$TAG -> $TARGET" >&2; exit 1;
}
HIST=$(git rev-list -n 1 r3.1-baseline-bbcfdb8b8bcd 2>/dev/null || true)
[ "$HIST" = 'bbcfdb8b8bcd4eda845a7f002accecd0289e1ac3' ] || {
  echo "失败：R3.1 历史标签缺失或移动。" >&2; exit 1;
}

gh release download "$TAG" --repo "$REPO" --dir "$TMP"
[ -s "$TMP/SHA256SUMS.txt" ] || { echo "失败：缺少 SHA256SUMS.txt" >&2; exit 1; }
(cd "$TMP" && sha256sum -c SHA256SUMS.txt)
mapfile -t zips < <(find "$TMP" -maxdepth 1 -type f -name 'evidence_*.zip' | sort)
[ "${#zips[@]}" -eq 2 ] || { echo "失败：Release 必须恰好含两套 evidence ZIP。" >&2; exit 1; }
for zip in "${zips[@]}"; do
  out="$TMP/unpacked_$(basename "$zip" .zip)"
  mkdir -p "$out"
  rc=0
  unzip -q "$zip" -d "$out" || rc=$?
  [ "$rc" -le 1 ] || { echo "失败：无法展开 $zip" >&2; exit 1; }
  root_commit=$(find "$out" -mindepth 2 -maxdepth 2 -type f -name commit.txt | head -1)
  root_report=$(find "$out" -mindepth 2 -maxdepth 2 -type f -name report.json | head -1)
  [ "$(tr -d '\r\n' < "$root_commit")" = "$TARGET" ] || {
    echo "失败：证据提交与标签不一致。" >&2; exit 1;
  }
  grep -Eq '"overall"[[:space:]]*:[[:space:]]*"PASS"' "$root_report" || {
    echo "失败：证据报告不是 PASS。" >&2; exit 1;
  }
  for id in E01 E02 E03 E04 E05 E06 E07 E08 E09 E10 E11 E12; do
    grep -Fq "\"id\":\"$id\"" "$root_report" || {
      echo "失败：证据缺少 $id。" >&2; exit 1;
    }
  done
done
bash scripts/check_vendor_assets.sh --manifest "$MANIFEST" --inventory "$INVENTORY" "$TMP"
echo "R4 Release 验证通过：$TAG -> $TARGET"
