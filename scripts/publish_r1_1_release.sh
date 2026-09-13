#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

usage() {
  cat <<'USAGE'
用法：
  bash scripts/publish_r1_1_release.sh \
    <baseline-full-sha> <impl-zip> <review-zip> <review-md>
USAGE
}

[ "$#" -eq 4 ] || { usage >&2; exit 2; }

REPO="${GITHUB_REPOSITORY:-ceyirelehe47/craftsman-fortress}"
OLD_TAG="${R1_SUPERSEDED_RELEASE_TAG:-r1-baseline-6123b37}"
BASELINE_INPUT="$1"
IMPL_ZIP="$2"
REVIEW_ZIP="$3"
REVIEW_MD="$4"

command -v git >/dev/null || { echo "缺少 git" >&2; exit 1; }
command -v gh >/dev/null || { echo "缺少 gh CLI" >&2; exit 1; }
command -v sha256sum >/dev/null || { echo "缺少 sha256sum" >&2; exit 1; }

BASELINE_SHA=$(git rev-parse "${BASELINE_INPUT}^{commit}")
SHORT=$(git rev-parse --short=12 "$BASELINE_SHA")
TAG="r1.1-baseline-${SHORT}"
EXPECTED_IMPL="evidence_impl_${SHORT}.zip"
EXPECTED_REVIEW="evidence_review_${SHORT}.zip"

[ -f "$IMPL_ZIP" ] || { echo "缺少 $IMPL_ZIP" >&2; exit 1; }
[ -f "$REVIEW_ZIP" ] || { echo "缺少 $REVIEW_ZIP" >&2; exit 1; }
[ -f "$REVIEW_MD" ] || { echo "缺少 $REVIEW_MD" >&2; exit 1; }
[ "$(basename "$IMPL_ZIP")" = "$EXPECTED_IMPL" ] || {
  echo "实现方证据文件名必须为 $EXPECTED_IMPL" >&2; exit 1;
}
[ "$(basename "$REVIEW_ZIP")" = "$EXPECTED_REVIEW" ] || {
  echo "Reviewer 证据文件名必须为 $EXPECTED_REVIEW" >&2; exit 1;
}

zip_list() {
  local archive="$1"
  if command -v unzip >/dev/null 2>&1; then
    unzip -Z1 "$archive"
  elif [ -x /c/Windows/System32/tar.exe ]; then
    # bsdtar 列表输出为 CRLF 行尾，统一去掉，保证条目名可精确匹配与提取。
    /c/Windows/System32/tar.exe -tf "$archive" | tr -d '\r'
  else
    echo "缺少 unzip 或 Windows bsdtar，无法检查证据 ZIP" >&2
    return 1
  fi
}

zip_read() {
  local archive="$1" regex="$2" entry
  entry=$(zip_list "$archive" | grep -E "$regex" | head -1 || true)
  [ -n "$entry" ] || { echo "ZIP 中找不到 $regex: $archive" >&2; return 1; }
  if command -v unzip >/dev/null 2>&1; then
    unzip -p "$archive" "$entry"
  else
    /c/Windows/System32/tar.exe -xOf "$archive" "$entry"
  fi
}

remote_tag_target() {
  local tag="$1" object_type object_sha
  object_type=$(gh api "repos/$REPO/git/ref/tags/$tag" --jq '.object.type')
  object_sha=$(gh api "repos/$REPO/git/ref/tags/$tag" --jq '.object.sha')
  if [ "$object_type" = "tag" ]; then
    gh api "repos/$REPO/git/tags/$object_sha" --jq '.object.sha'
  else
    printf '%s\n' "$object_sha"
  fi
}

for archive in "$IMPL_ZIP" "$REVIEW_ZIP"; do
  COMMIT=$(zip_read "$archive" '(^|/)commit\.txt$' | tr -d '\r\n')
  [ "$COMMIT" = "$BASELINE_SHA" ] || {
    echo "$archive 的 commit.txt=$COMMIT，与 $BASELINE_SHA 不一致" >&2; exit 1;
  }
  REPORT=$(zip_read "$archive" '(^|/)report\.json$')
  printf '%s' "$REPORT" | grep -Eq '"overall"[[:space:]]*:[[:space:]]*"PASS"' || {
    echo "$archive 的 report.json 不是 PASS" >&2; exit 1;
  }
done

IMPL_HASH=$(sha256sum "$IMPL_ZIP" | awk '{print $1}')
REVIEW_HASH=$(sha256sum "$REVIEW_ZIP" | awk '{print $1}')

grep -Fq "$BASELINE_SHA" "$REVIEW_MD" || {
  echo "$REVIEW_MD 未引用 baseline SHA" >&2; exit 1;
}
grep -Fq "$IMPL_HASH" "$REVIEW_MD" || {
  echo "$REVIEW_MD 未引用实现方 ZIP 哈希" >&2; exit 1;
}
grep -Fq "$REVIEW_HASH" "$REVIEW_MD" || {
  echo "$REVIEW_MD 未引用 Reviewer ZIP 哈希" >&2; exit 1;
}
if grep -Eq '__[A-Z0-9_]+__' "$REVIEW_MD"; then
  echo "$REVIEW_MD 仍含占位符" >&2; exit 1
fi

if gh release view "$TAG" --repo "$REPO" >/dev/null 2>&1; then
  echo "Release $TAG 已存在；封版脚本拒绝覆盖" >&2
  exit 1
fi

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
SUMS="$TMP/SHA256SUMS.txt"
NOTES="$TMP/RELEASE_NOTES.md"
printf '%s  %s\n' "$IMPL_HASH" "$EXPECTED_IMPL" > "$SUMS"
printf '%s  %s\n' "$REVIEW_HASH" "$EXPECTED_REVIEW" >> "$SUMS"
cat > "$NOTES" <<EOF_NOTES
R1.1 工程基线封版。

- 代码验收提交：\`$BASELINE_SHA\`
- 实现方证据：\`$EXPECTED_IMPL\`（SHA-256 \`$IMPL_HASH\`）
- Reviewer 证据：\`$EXPECTED_REVIEW\`（SHA-256 \`$REVIEW_HASH\`）
- 两套证据的 \`commit.txt\` 均指向同一代码提交，A01–A14 全部通过。

后置的复核报告文档提交不会改变本标签的证据目标。
EOF_NOTES

# 允许“标签已正确推送但 Release 创建失败”的安全重试；绝不移动错误标签。
if gh api "repos/$REPO/git/ref/tags/$TAG" >/dev/null 2>&1; then
  EXISTING_TARGET=$(remote_tag_target "$TAG")
  [ "$EXISTING_TARGET" = "$BASELINE_SHA" ] || {
    echo "远端标签 $TAG 已存在但指向 $EXISTING_TARGET；拒绝移动到 $BASELINE_SHA" >&2
    exit 1
  }
  echo "远端标签已存在且目标正确，继续创建 Release：$TAG"
else
  if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
    LOCAL_TARGET=$(git rev-parse "${TAG}^{commit}")
    [ "$LOCAL_TARGET" = "$BASELINE_SHA" ] || {
      echo "本地标签 $TAG 指向错误提交 $LOCAL_TARGET" >&2; exit 1;
    }
  else
    git tag -a "$TAG" "$BASELINE_SHA" -m "R1.1 baseline $BASELINE_SHA"
  fi
  git push origin "refs/tags/$TAG"
fi

gh repo edit "$REPO" --description \
  "《工匠要塞》初版验证构建：Rust + Bevy 0.19 三维体素世界、确定性地形生成、自由透视相机与全自动验收体系"

gh release create "$TAG" \
  "$IMPL_ZIP" "$REVIEW_ZIP" "$SUMS" \
  --repo "$REPO" \
  --verify-tag \
  --title "R1.1 工程基线封版（${SHORT}）" \
  --notes-file "$NOTES"

# 历史 Release 保留原标签和资产，但明确标记已被新基线取代。
if gh release view "$OLD_TAG" --repo "$REPO" >/dev/null 2>&1; then
  OLD_BODY=$(gh release view "$OLD_TAG" --repo "$REPO" --json body --jq '.body')
  if ! printf '%s' "$OLD_BODY" | grep -Fq "已被 $TAG 取代"; then
    OLD_NOTES="$TMP/OLD_RELEASE_NOTES.md"
    {
      echo "**历史中间轮：已被 \`$TAG\` 取代。此处标签与资产仅用于保留过程证据，不代表当前工程基线。**"
      echo
      printf '%s\n' "$OLD_BODY"
    } > "$OLD_NOTES"
    gh release edit "$OLD_TAG" --repo "$REPO" \
      --title "R1 历史中间轮（已被 ${TAG} 取代）" \
      --notes-file "$OLD_NOTES"
  fi
fi

bash scripts/verify_r1_1_release.sh \
  "$TAG" "$BASELINE_SHA" "$EXPECTED_IMPL" "$EXPECTED_REVIEW" "$REVIEW_MD"

echo "R1.1 Release 发布与复核通过：$TAG"
