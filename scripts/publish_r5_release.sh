#!/usr/bin/env bash
# 发布 R5 不可移动基线；发布后下载复验失败时撤销本轮新建的标签与 Release。
set -euo pipefail
cd "$(dirname "$0")/.."
export LC_ALL=C

S="${1:?usage: publish_r5_release.sh <S> <impl.zip> <review.zip> <review.md> <selection.tsv> <inventory.tsv>}"
IMPL="${2:?missing implementation evidence zip}"
REVIEW="${3:?missing reviewer evidence zip}"
REVIEW_MD="${4:?missing reviewer report}"
MANIFEST="${5:?missing selection.tsv}"
INVENTORY="${6:?missing inventory.tsv}"
REPO="${GITHUB_REPOSITORY:-ceyirelehe47/craftsman-fortress}"
command -v gh >/dev/null 2>&1 || { echo "失败：需要 gh CLI。" >&2; exit 1; }
command -v unzip >/dev/null 2>&1 || { echo "失败：需要 unzip。" >&2; exit 1; }
[ -z "$(git status --porcelain)" ] || { echo "失败：发布工作区必须干净。" >&2; exit 1; }
S=$(git rev-parse "$S^{commit}")
SHORT=${S:0:12}
TAG="r5-baseline-$SHORT"
for file in "$IMPL" "$REVIEW" "$REVIEW_MD" "$MANIFEST" "$INVENTORY"; do
  [ -s "$file" ] || { echo "失败：缺少发布输入 $file" >&2; exit 1; }
done
HIST=$(git rev-list -n 1 r4-baseline-1c05cfafce2e 2>/dev/null || true)
[ "$HIST" = '1c05cfafce2e9dabf46eb5853cd58ece1bf0bbdc' ] || {
  echo "失败：R4 历史基线缺失或移动：$HIST" >&2
  exit 1
}
git merge-base --is-ancestor "$HIST" "$S" || {
  echo "失败：代码验收提交 S 不是 R4 正式基线的后代：$S" >&2
  exit 1
}

if git rev-parse "$TAG" >/dev/null 2>&1 || gh release view "$TAG" --repo "$REPO" >/dev/null 2>&1; then
  echo "失败：标签或 Release 已存在：$TAG" >&2
  exit 1
fi
bash scripts/check_vendor_assets.sh --manifest "$MANIFEST" --inventory "$INVENTORY" "$IMPL" "$REVIEW"
grep -Fq "$S" "$REVIEW_MD" || { echo "失败：Reviewer 报告未引用完整 S。" >&2; exit 1; }

TMP=$(mktemp -d)
CREATED_TAG=0
CREATED_RELEASE=0
cleanup() {
  code=$?
  if [ "$code" -ne 0 ]; then
    [ "$CREATED_RELEASE" -eq 0 ] || gh release delete "$TAG" --repo "$REPO" --yes >/dev/null 2>&1 || true
    if [ "$CREATED_TAG" -ne 0 ]; then
      git push origin ":refs/tags/$TAG" >/dev/null 2>&1 || true
      git tag -d "$TAG" >/dev/null 2>&1 || true
    fi
  fi
  rm -rf "$TMP"
  exit "$code"
}
trap cleanup EXIT

for zip_file in "$IMPL" "$REVIEW"; do
  out="$TMP/$(basename "$zip_file" .zip)"
  mkdir -p "$out"
  unzip -q "$zip_file" -d "$out" || [ "$?" -eq 1 ]
  root_commit=$(find "$out" -mindepth 2 -maxdepth 2 -type f -name commit.txt | head -1)
  root_report=$(find "$out" -mindepth 2 -maxdepth 2 -type f -name report.json | head -1)
  [ -s "$root_commit" ] && [ "$(tr -d '\r\n' < "$root_commit")" = "$S" ] || {
    echo "失败：$zip_file 根级 commit.txt 不等于 S。" >&2
    exit 1
  }
  [ -s "$root_report" ] && grep -Eq '"overall"[[:space:]]*:[[:space:]]*"PASS"' "$root_report" || {
    echo "失败：$zip_file 根级 report.json 不是 PASS。" >&2
    exit 1
  }
  for id in F01 F02 F03 F04 F05 F06 F07 F08 F09 F10 F11 F12; do
    grep -Fq "\"id\":\"$id\"" "$root_report" || {
      echo "失败：$zip_file 缺少 $id。" >&2
      exit 1
    }
  done
done

IMPL_NAME=$(basename "$IMPL")
REVIEW_NAME=$(basename "$REVIEW")
cp "$IMPL" "$TMP/$IMPL_NAME"
cp "$REVIEW" "$TMP/$REVIEW_NAME"
(cd "$TMP" && sha256sum "$IMPL_NAME" "$REVIEW_NAME" > SHA256SUMS.txt)
IMPL_SHA=$(sha256sum "$IMPL" | awk '{print $1}')
REVIEW_SHA=$(sha256sum "$REVIEW" | awk '{print $1}')

git tag -a "$TAG" "$S" -m "R5 modular building layer baseline $S"
git push origin "$TAG"
CREATED_TAG=1
gh release create "$TAG" --repo "$REPO" \
  "$TMP/$IMPL_NAME" "$TMP/$REVIEW_NAME" "$TMP/SHA256SUMS.txt" \
  --title "R5 模块化建筑构件与建筑存档（$SHORT）" \
  --notes "代码验收提交 S：\`$S\`\n\n实现方证据：\`$IMPL_NAME\`（SHA-256 \`$IMPL_SHA\`）\nReviewer 证据：\`$REVIEW_NAME\`（SHA-256 \`$REVIEW_SHA\`）\n\n第三方模型、贴图和原包未包含在 Release 中。"
CREATED_RELEASE=1
bash scripts/verify_r5_release.sh "$TAG" "$MANIFEST" "$INVENTORY"
trap - EXIT
rm -rf "$TMP"
echo "R5 Release 发布完成：$TAG"
