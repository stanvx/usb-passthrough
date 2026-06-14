#!/usr/bin/env bash
# Generate changelog from conventional commits between two git refs.
# Usage: gen-changelog.sh <tag> <previous_tag>
# If previous_tag is empty, shows all commits up to tag.
set -euo pipefail

TAG="${1:-HEAD}"
PREV="${2:-}"

if [ -n "$PREV" ]; then
    RANGE="${PREV}..${TAG}"
    echo "## What's Changed since ${PREV}"
else
    RANGE="${TAG}"
    echo "## What's Changed"
fi
echo ""

# Categories in display order
declare -A SECTIONS
SECTIONS=(
    ["feat"]="🚀 Features"
    ["fix"]="🐛 Bug Fixes"
    ["docs"]="📖 Documentation"
    ["perf"]="⚡ Performance"
    ["refactor"]="♻️ Refactoring"
    ["test"]="🧪 Tests"
    ["ci"]="🔧 CI/CD"
    ["chore"]="🧹 Chores"
    ["style"]="🎨 Style"
    ["build"]="🏗️ Build"
    ["revert"]="⏪ Reverts"
)

# Order of sections
ORDER=(feat fix perf refactor test docs ci chore build style revert)

# Temporary files for each section
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

for key in "${ORDER[@]}"; do
    : > "$TMPDIR/$key"
done

# Parse conventional commits
git log --oneline "$RANGE" 2>/dev/null | while IFS= read -r line; do
    # Extract commit hash and message
    HASH=$(echo "$line" | cut -d' ' -f1)
    MSG=$(echo "$line" | cut -d' ' -f2-)

    # Check for conventional commit pattern: type(scope): description
    if echo "$MSG" | grep -qE '^[a-z]+(\([^)]+\))?: '; then
        TYPE=$(echo "$MSG" | sed -n 's/^\([a-z]*\).*/\1/p')
        SCOPE=$(echo "$MSG" | sed -n 's/^[a-z]*(\([^)]*\)).*/\1/p' || echo "")
        DESC=$(echo "$MSG" | sed -n 's/^[a-z]*(\([^)]*\)): //p' || echo "$MSG" | sed -n 's/^[a-z]*: //p')
        [ -z "$SCOPE" ] && SCOPE=""
    else
        TYPE="other"
        DESC="$MSG"
        SCOPE=""
    fi

    SHORT_HASH=$(echo "$HASH" | cut -c1-7)

    if [ -f "$TMPDIR/$TYPE" ]; then
        if [ -n "$SCOPE" ]; then
            echo "- **${SCOPE}**: ${DESC} (${SHORT_HASH})" >> "$TMPDIR/$TYPE"
        else
            echo "- ${DESC} (${SHORT_HASH})" >> "$TMPDIR/$TYPE"
        fi
    else
        echo "- ${DESC} (${SHORT_HASH})" >> "$TMPDIR/other"
    fi
done

# Output in order
any_section=false
for key in "${ORDER[@]}"; do
    if [ -s "$TMPDIR/$key" ]; then
        echo "### ${SECTIONS[$key]}"
        echo ""
        cat "$TMPDIR/$key"
        echo ""
        any_section=true
    fi
done

# Uncategorized commits
if [ -s "$TMPDIR/other" ]; then
    echo "### Others"
    echo ""
    cat "$TMPDIR/other"
    echo ""
    any_section=true
fi

if [ "$any_section" = false ]; then
    echo "_No conventional commits found in this release._"
    echo ""
fi
