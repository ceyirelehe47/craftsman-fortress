#!/usr/bin/env bash
# 下载并验证 R3.1 Release：标签目标、证据提交、PASS 报告、校验和与第三方字节泄漏。
set -euo pipefail
cd "$(dirname "$0")/.."
export LC_ALL=C

TAG="${1:?usage: verify_r3_1_release.sh <tag> <selection.tsv> <inventory.tsv>}"
MANIFEST="${2:?missing selection.tsv}"
INVENTORY="${3:?missing inventory.tsv}"
REPO="${GITHUB_REPOSITORY:-ceyirelehe47/craftsman-fortress}"
command -v gh >/dev/null 2>&1 || { echo "失败：需要 gh CLI。" >&2; exit 1; }
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

git fetch --tags --force >/dev/null
TARGET=$(git rev-list -n 1 "$TAG")
case "$TAG" in r3.1-baseline-*) ;; *) echo "失败：标签名不是 r3.1-baseline-*" >&2; exit 1;; esac
SHORT=${TAG#r3.1-baseline-}
[ "${TARGET:0:12}" = "$SHORT" ] || {
  echo "失败：标签短 SHA 与目标不一致：$TAG -> $TARGET" >&2; exit 1;
}
# 历史正式标签不可移动：R3 基线必须仍解引用到原 R3 代码提交。
HIST_TARGET=$(git rev-list -n 1 'r3-baseline-355344c10846' 2>/dev/null || true)
[ "$HIST_TARGET" = '355344c10846f5ca63cac61a7fc9bbc3ba94f077' ] || {
  echo "失败：历史标签 r3-baseline-355344c10846 缺失或已被移动：$HIST_TARGET" >&2; exit 1
}

gh release download "$TAG" --repo "$REPO" --dir "$TMP"
[ -s "$TMP/SHA256SUMS.txt" ] || { echo "失败：Release 缺少 SHA256SUMS.txt" >&2; exit 1; }
(cd "$TMP" && sha256sum -c SHA256SUMS.txt)
mapfile -t zips < <(find "$TMP" -maxdepth 1 -type f -name 'evidence_*.zip' | sort)
[ "${#zips[@]}" -eq 2 ] || { echo "失败：Release 必须恰好包含两套 evidence ZIP。" >&2; exit 1; }

for zip in "${zips[@]}"; do
  name=$(basename "$zip" .zip)
  out="$TMP/unpacked_$name"
  mkdir -p "$out"
  if command -v unzip >/dev/null 2>&1; then
    # PowerShell 生成的证据 ZIP 使 unzip 以警告码 1 结束但解包完整；>1 才失败。
    rc=0
    unzip -qq "$zip" -d "$out" || rc=$?
    [ "$rc" -le 1 ] || { echo "失败：unzip 无法展开 $zip（exit=$rc）。" >&2; exit 1; }
  elif [ -x /c/Windows/System32/tar.exe ]; then
    /c/Windows/System32/tar.exe -xf "$zip" -C "$out"
  else
    echo "失败：无法展开证据 ZIP。" >&2; exit 1
  fi
  root_commit=$(find "$out" -mindepth 2 -maxdepth 2 -type f -name commit.txt | head -1)
  [ -s "$root_commit" ] || { echo "失败：$zip 缺少根级 commit.txt" >&2; exit 1; }
  [ "$(tr -d '\r\n' < "$root_commit")" = "$TARGET" ] || {
    echo "失败：$zip 的根级 commit.txt 不等于标签目标。" >&2; exit 1;
  }
  root_report=$(find "$out" -mindepth 2 -maxdepth 2 -type f -name report.json | head -1)
  [ -s "$root_report" ] && grep -Eq '"overall"[[:space:]]*:[[:space:]]*"PASS"' "$root_report" || {
    echo "失败：$zip 缺少根级 PASS report.json。" >&2; exit 1;
  }
done

bash scripts/check_vendor_assets.sh --manifest "$MANIFEST" --inventory "$INVENTORY" "$TMP"
echo "R3.1 Release 验证通过：$TAG -> $TARGET"
