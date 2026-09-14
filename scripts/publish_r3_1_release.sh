#!/usr/bin/env bash
# 发布 R3.1 不可移动基线：证据必须先完成提交/PASS/泄漏校验。
set -euo pipefail
cd "$(dirname "$0")/.."
export LC_ALL=C

S="${1:?usage: publish_r3_1_release.sh <S> <impl.zip> <review.zip> <review.md> <selection.tsv> <inventory.tsv>}"
IMPL="${2:?missing implementation evidence zip}"
REVIEW="${3:?missing reviewer evidence zip}"
REVIEW_MD="${4:?missing reviewer report}"
MANIFEST="${5:?missing selection.tsv}"
INVENTORY="${6:?missing inventory.tsv}"
REPO="${GITHUB_REPOSITORY:-ceyirelehe47/craftsman-fortress}"
command -v gh >/dev/null 2>&1 || { echo "失败：需要 gh CLI。" >&2; exit 1; }
[ -z "$(git status --porcelain)" ] || { echo "失败：发布工作区必须干净。" >&2; exit 1; }
S=$(git rev-parse "$S^{commit}")
SHORT=${S:0:12}
TAG="r3.1-baseline-$SHORT"
for file in "$IMPL" "$REVIEW" "$REVIEW_MD" "$MANIFEST" "$INVENTORY"; do
  [ -s "$file" ] || { echo "失败：缺少发布输入 $file" >&2; exit 1; }
done

git show "$S" >/dev/null
# 历史正式标签不可移动：R3 基线必须仍解引用到原 R3 代码提交。
HIST_TAG='r3-baseline-355344c10846'
HIST_TARGET=$(git rev-list -n 1 "$HIST_TAG" 2>/dev/null || true)
[ "$HIST_TARGET" = '355344c10846f5ca63cac61a7fc9bbc3ba94f077' ] || {
  echo "失败：历史标签 $HIST_TAG 缺失或已被移动：$HIST_TARGET" >&2; exit 1
}
if git rev-parse "$TAG" >/dev/null 2>&1 || gh release view "$TAG" --repo "$REPO" >/dev/null 2>&1; then
  echo "失败：标签或 Release 已存在，禁止移动/覆盖：$TAG" >&2; exit 1
fi
bash scripts/check_vendor_assets.sh --manifest "$MANIFEST" --inventory "$INVENTORY" "$IMPL" "$REVIEW"

grep -Fq "$S" "$REVIEW_MD" || { echo "失败：Reviewer 报告未引用完整 S。" >&2; exit 1; }
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
check_index=0
ASSET_SETS=()
for zip in "$IMPL" "$REVIEW"; do
  check_index=$((check_index + 1))
  out="$TMP/check_$check_index"
  mkdir -p "$out"
  if command -v unzip >/dev/null 2>&1; then
    # PowerShell 生成的证据 ZIP 使 unzip 以警告码 1 结束但解包完整；>1 才失败。
    rc=0
    unzip -qq "$zip" -d "$out" || rc=$?
    [ "$rc" -le 1 ] || { echo "失败：unzip 无法展开 $zip（exit=$rc）。" >&2; exit 1; }
  else
    echo "失败：发布前证据检查需要 unzip。" >&2; exit 1
  fi
  root_commit=$(find "$out" -mindepth 2 -maxdepth 2 -type f -name commit.txt | head -1)
  root_report=$(find "$out" -mindepth 2 -maxdepth 2 -type f -name report.json | head -1)
  [ -s "$root_commit" ] && [ "$(tr -d '\r\n' < "$root_commit")" = "$S" ] || {
    echo "失败：$zip 的根级 commit.txt 不等于 S。" >&2; exit 1;
  }
  [ -s "$root_report" ] && grep -Eq '"overall"[[:space:]]*:[[:space:]]*"PASS"' "$root_report" || {
    echo "失败：$zip 的根级 report.json 不是 PASS。" >&2; exit 1;
  }
  asset_set=$(grep -o '"asset_set_sha256"[[:space:]]*:[[:space:]]*"[0-9a-f]\{64\}"' "$root_report" \
    | head -1 | grep -o '[0-9a-f]\{64\}')
  [ ${#asset_set} -eq 64 ] || {
    echo "失败：$zip 的 report.json 缺少有效的 asset_set_sha256。" >&2; exit 1;
  }
  ASSET_SETS+=("$asset_set")
done
[ "${ASSET_SETS[0]}" = "${ASSET_SETS[1]:-}" ] || {
  echo "失败：实现方与 Reviewer 的 asset_set_sha256 不一致：${ASSET_SETS[0]} / ${ASSET_SETS[1]}" >&2
  exit 1
}

IMPL_NAME=$(basename "$IMPL")
REVIEW_NAME=$(basename "$REVIEW")
cp "$IMPL" "$TMP/$IMPL_NAME"
cp "$REVIEW" "$TMP/$REVIEW_NAME"
(
  cd "$TMP"
  sha256sum "$IMPL_NAME" "$REVIEW_NAME" > SHA256SUMS.txt
)
IMPL_SHA=$(sha256sum "$IMPL" | awk '{print $1}')
REVIEW_SHA=$(sha256sum "$REVIEW" | awk '{print $1}')

git tag -a "$TAG" "$S" -m "R3.1 source-chain and redistribution-boundary baseline $S"
git push origin "$TAG"
gh release create "$TAG" --repo "$REPO" \
  "$TMP/$IMPL_NAME" "$TMP/$REVIEW_NAME" "$TMP/SHA256SUMS.txt" \
  --title "R3.1 素材来源链、内容哈希与授权边界封版（$SHORT）" \
  --notes "代码验收提交 S：\`$S\`\n\n实现方证据：\`$IMPL_NAME\`（SHA-256 \`$IMPL_SHA\`）\nReviewer 证据：\`$REVIEW_NAME\`（SHA-256 \`$REVIEW_SHA\`）\n\n第三方模型、贴图和原包未包含在 Release 中。"

bash scripts/verify_r3_1_release.sh "$TAG" "$MANIFEST" "$INVENTORY"
echo "R3.1 Release 发布完成：$TAG"
