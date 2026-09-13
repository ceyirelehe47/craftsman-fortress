#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

usage() {
  cat <<'USAGE'
用法：
  bash scripts/verify_r1_1_release.sh \
    <tag> <baseline-full-sha> <impl-asset-name> <review-asset-name> <review-md>
USAGE
}

[ "$#" -eq 5 ] || { usage >&2; exit 2; }
REPO="${GITHUB_REPOSITORY:-ceyirelehe47/craftsman-fortress}"
OLD_TAG="${R1_SUPERSEDED_RELEASE_TAG:-r1-baseline-6123b37}"
TAG="$1"
BASELINE_SHA=$(git rev-parse "${2}^{commit}")
IMPL_NAME="$3"
REVIEW_NAME="$4"
REVIEW_MD="$5"

command -v gh >/dev/null || { echo "缺少 gh CLI" >&2; exit 1; }
command -v sha256sum >/dev/null || { echo "缺少 sha256sum" >&2; exit 1; }

OBJ_TYPE=$(gh api "repos/$REPO/git/ref/tags/$TAG" --jq '.object.type')
OBJ_SHA=$(gh api "repos/$REPO/git/ref/tags/$TAG" --jq '.object.sha')
if [ "$OBJ_TYPE" = "tag" ]; then
  TAG_TARGET=$(gh api "repos/$REPO/git/tags/$OBJ_SHA" --jq '.object.sha')
else
  TAG_TARGET="$OBJ_SHA"
fi
[ "$TAG_TARGET" = "$BASELINE_SHA" ] || {
  echo "标签 $TAG 指向 $TAG_TARGET，不是 $BASELINE_SHA" >&2; exit 1;
}

RELEASE_TAG=$(gh release view "$TAG" --repo "$REPO" --json tagName --jq '.tagName')
[ "$RELEASE_TAG" = "$TAG" ] || { echo "Release 标签不一致" >&2; exit 1; }

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
gh release download "$TAG" --repo "$REPO" --dir "$TMP" \
  --pattern "$IMPL_NAME" --pattern "$REVIEW_NAME" --pattern 'SHA256SUMS.txt'

for f in "$IMPL_NAME" "$REVIEW_NAME" SHA256SUMS.txt; do
  [ -s "$TMP/$f" ] || { echo "Release 资产缺失或为空：$f" >&2; exit 1; }
done
(
  cd "$TMP"
  sha256sum -c SHA256SUMS.txt
)

IMPL_HASH=$(sha256sum "$TMP/$IMPL_NAME" | awk '{print $1}')
REVIEW_HASH=$(sha256sum "$TMP/$REVIEW_NAME" | awk '{print $1}')
grep -Fq "$BASELINE_SHA" "$REVIEW_MD" || { echo "报告未引用 baseline SHA" >&2; exit 1; }
grep -Fq "$IMPL_HASH" "$REVIEW_MD" || { echo "报告未引用实现方哈希" >&2; exit 1; }
grep -Fq "$REVIEW_HASH" "$REVIEW_MD" || { echo "报告未引用 Reviewer 哈希" >&2; exit 1; }

DESCRIPTION=$(gh repo view "$REPO" --json description --jq '.description')
printf '%s' "$DESCRIPTION" | grep -Fq '《工匠要塞》' || {
  echo "仓库简介未使用正式名称：$DESCRIPTION" >&2; exit 1;
}
LEGACY_SUBTITLE='机械''纪元'
LEGACY_TEMP='暂''名'
if printf '%s' "$DESCRIPTION" | grep -Eq "${LEGACY_SUBTITLE}|${LEGACY_TEMP}"; then
  echo "仓库简介仍含历史占位名称：$DESCRIPTION" >&2; exit 1
fi

if gh release view "$OLD_TAG" --repo "$REPO" >/dev/null 2>&1; then
  OLD_TITLE=$(gh release view "$OLD_TAG" --repo "$REPO" --json name --jq '.name')
  OLD_BODY=$(gh release view "$OLD_TAG" --repo "$REPO" --json body --jq '.body')
  printf '%s' "$OLD_TITLE" | grep -Fq '历史中间轮' || {
    echo "历史 Release 标题未标明中间轮：$OLD_TITLE" >&2; exit 1;
  }
  printf '%s' "$OLD_BODY" | grep -Fq "已被 $TAG 取代" || {
    echo "历史 Release 正文未引用新基线 $TAG" >&2; exit 1;
  }
fi

echo "标签目标、Release 资产、SHA-256、复核报告、仓库简介与历史 Release 状态全部一致。"
