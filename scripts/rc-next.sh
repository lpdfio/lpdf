#!/bin/sh
# Cuts the next core release candidate (Lpdf-devcycle.md §4.2).
#
# Works out vX.Y.0-rc.N from the tags on GitHub, shows what would be tagged,
# and pushes the tag only after you confirm. The push starts core's
# release.yml, job "rc": tests, build, a GitHub pre-release, then `core-rc`
# to the five SDK and extension repos, whose rc.yml builds and tests against
# it. Nothing reaches a package registry.
#
#   make rc-next                  next minor after the latest release
#   make rc-next VERSION=v1.0.0   a specific version, e.g. a major bump
#
# Tags GitHub's main, never your local branch, and reads tags from GitHub
# (git ls-remote), so stale or local-only tags can't skew the numbering.
set -eu

REMOTE=origin
BRANCH=main
REPO_URL=https://github.com/lpdfio/lpdf
REPOS="lpdf-js lpdf-dotnet lpdf-php lpdf-python lpdf-vscode"

fail() { printf 'rc-next: %s\n' "$*" >&2; exit 1; }

# ── What GitHub has ────────────────────────────────────────────────────────────
# Fetch main (for the commit and its log) and tags (for the "since" range).
git fetch --quiet --tags "$REMOTE" "$BRANCH" || fail "could not fetch from $REMOTE"
SHA=$(git ls-remote "$REMOTE" "refs/heads/$BRANCH" | cut -f1)
[ -n "$SHA" ] || fail "$REMOTE has no $BRANCH branch"
TAGS=$(git ls-remote --tags --refs "$REMOTE" 'v*' | sed 's|.*refs/tags/||')

# A tag push runs release.yml as it is AT the tagged commit. Without the RC job
# there, the old workflow would treat vX.Y.0-rc.N as a full public release.
git show "$SHA:.github/workflows/release.yml" 2>/dev/null | grep -q 'Publish release candidate' \
  || fail "GitHub's $BRANCH has no RC job in .github/workflows/release.yml yet. Push that first, and rc.yml to the five SDK/extension repos."

# A dispatch only starts a workflow that exists on the receiving repo's main.
# A repo without rc.yml would silently run nothing, and the RC would look
# complete with one run missing. The repos are public, so the raw file answers.
MISSING=""
for r in $REPOS; do
  code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 15 \
    "https://raw.githubusercontent.com/lpdfio/$r/$BRANCH/.github/workflows/rc.yml" || echo "000")
  case "$code" in
    200) ;;
    404) MISSING="$MISSING $r" ;;
    *)   echo "rc-next: could not check $r for rc.yml (HTTP $code). Check it by hand." >&2 ;;
  esac
done
[ -z "$MISSING" ] || fail "no .github/workflows/rc.yml on $BRANCH in:$MISSING. Push it there first."

LAST=$(printf '%s\n' "$TAGS" | grep -E '^v[0-9]+\.[0-9]+\.0$' | sort -V | tail -n 1 || true)

# ── Which version ──────────────────────────────────────────────────────────────
TARGET="${1:-}"
if [ -n "$TARGET" ]; then
  printf '%s\n' "$TARGET" | grep -qE '^v[0-9]+\.[0-9]+\.0$' \
    || fail "VERSION must look like v1.3.0 (core releases are always MAJOR.MINOR.0), got '$TARGET'"
elif [ -n "$LAST" ]; then
  # Next minor: v0.16.0 → v0.17.0. A major bump is always explicit (VERSION=).
  MAJOR=$(printf '%s' "$LAST" | sed -E 's/^v([0-9]+)\..*/\1/')
  MINOR=$(printf '%s' "$LAST" | sed -E 's/^v[0-9]+\.([0-9]+)\..*/\1/')
  TARGET="v$MAJOR.$((MINOR + 1)).0"
else
  fail "no release tag found on $REMOTE; pass one, e.g. make rc-next VERSION=v0.1.0"
fi

printf '%s\n' "$TAGS" | grep -qxF "$TARGET" \
  && fail "$TARGET is already released; pass a later VERSION"

# ── Which RC number ────────────────────────────────────────────────────────────
TARGET_RE=$(printf '%s' "$TARGET" | sed 's/\./\\./g')   # dots literal in the regex
PREV_RC=$(printf '%s\n' "$TAGS" | grep -E "^$TARGET_RE-rc\.[0-9]+\$" | sort -V | tail -n 1 || true)
if [ -n "$PREV_RC" ]; then
  N=$(( ${PREV_RC##*-rc.} + 1 ))
else
  N=1
fi
NEW="$TARGET-rc.$N"

# ── Show it ────────────────────────────────────────────────────────────────────
SHORT=$(git rev-parse --short "$SHA")
echo ""
echo "-------------------------------"
echo ">>> Next core release candidate"
echo ""
echo "  Last release:  ${LAST:-none}"
echo "  Next version:  $TARGET"
if [ -n "$PREV_RC" ]; then
  # From the tags fetched above; ^{commit} peels annotated and lightweight alike.
  PREV_SHA=$(git rev-parse --verify --quiet "refs/tags/$PREV_RC^{commit}" || true)
  echo "  Previous RC:   $PREV_RC on $(git rev-parse --short "$PREV_SHA" 2>/dev/null || echo '?')"
else
  echo "  Previous RC:   none"
fi
echo "  New tag:       $NEW"
echo "  On commit:     $(git log -1 --format='%h %s (%ar)' "$SHA")  <- GitHub's $BRANCH"

if [ -n "$LAST" ]; then
  COUNT=$(git rev-list --count "$LAST..$SHA")
  echo "  Since $LAST:  $COUNT commit(s)"
  git log --oneline --no-merges -n 15 "$LAST..$SHA" | sed 's/^/                 /'
  [ "$COUNT" -gt 15 ] && echo "                 …"
fi

# Heads-ups: none of these stop you.
if [ -n "$PREV_RC" ] && [ "${PREV_SHA:-}" = "$SHA" ]; then
  echo ""
  echo "  Note: $PREV_RC is already on this commit. A new RC would test the same"
  echo "        code. To retry after a flaky run, re-run that workflow instead."
fi
LOCAL=$(git rev-parse --verify --quiet "refs/heads/$BRANCH" || true)
if [ -n "$LOCAL" ] && [ "$LOCAL" != "$SHA" ]; then
  AHEAD=$(git rev-list --count "$SHA..$LOCAL" 2>/dev/null || echo "?")
  BEHIND=$(git rev-list --count "$LOCAL..$SHA" 2>/dev/null || echo "?")
  echo ""
  echo "  Note: your local $BRANCH is $AHEAD ahead / $BEHIND behind GitHub. The RC"
  echo "        tags GitHub's $BRANCH; push first if your commits belong in it."
fi

echo ""
echo "Pushing the tag starts release.yml on GitHub: tests, build, a GitHub"
echo "pre-release, then core-rc to $(echo "$REPOS" | sed 's/ /, /g')."
echo "Nothing is published to a package registry."
echo ""
printf 'Push %s? [y/N] ' "$NEW"
read -r ANSWER || ANSWER=""
case "$ANSWER" in
  y|Y|yes|YES) ;;
  *) echo "Nothing pushed."; exit 0 ;;
esac

# ── Tag and push ───────────────────────────────────────────────────────────────
# Annotated, so the tag records who cut the RC and when. The core gate accepts
# lightweight and annotated tags alike.
git tag -a "$NEW" "$SHA" -m "Release candidate $NEW"
if ! git push --quiet "$REMOTE" "refs/tags/$NEW"; then
  git tag -d "$NEW" >/dev/null
  fail "push failed; the local tag was removed, nothing changed on GitHub"
fi

echo ""
echo "Pushed $NEW."
echo ""
echo "  Check progress:  make rc-check   (all six runs, what failed, and the release link once green)"
echo "  Follow the run:  $REPO_URL/actions/workflows/release.yml"
echo "  RC runs:"
for r in $REPOS; do
  echo "    https://github.com/lpdfio/$r/actions/workflows/rc.yml"
done
echo ""
echo "When all five are green and the .vsix checks out, publish $TARGET on the"
echo "same commit. This opens the release form with tag and commit filled in:"
echo "  $REPO_URL/releases/new?tag=$TARGET&target=$SHA&title=$TARGET"
