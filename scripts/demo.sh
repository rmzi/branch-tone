#!/usr/bin/env bash
# demo.sh — Play branch-tone hook sound for repos in a directory
#
# Usage:
#   ./scripts/demo.sh                  # scan current directory for git repos
#   ./scripts/demo.sh /path/to/repos   # scan a specific directory
#   ./scripts/demo.sh --all            # include worktrees too
#
# Each git repo gets its unique tone. Same repo, different branch = same
# voice (key/scale/timbre), different melody (pattern/rhythm).

set -euo pipefail

INCLUDE_WORKTREES=false
DIRS=()

for arg in "$@"; do
    case "$arg" in
        --all) INCLUDE_WORKTREES=true ;;
        *)     DIRS+=("$arg") ;;
    esac
done

# Default: scan current directory
if [[ ${#DIRS[@]} -eq 0 ]]; then
    DIRS=(".")
fi

# Collect repos: any directory containing a .git dir/file
repos=()
for dir in "${DIRS[@]}"; do
    [[ -d "$dir" ]] || continue
    for candidate in "$dir"/*/; do
        [[ -d "$candidate/.git" ]] && repos+=("$candidate")
    done
done

# Optionally include worktrees
if $INCLUDE_WORKTREES; then
    worktree_repos=()
    for repo in "${repos[@]}"; do
        wt_dir="$repo/.worktrees"
        [[ -d "$wt_dir" ]] || continue
        for wt in "$wt_dir"/*/; do
            [[ -d "$wt/.git" || -f "$wt/.git" ]] && worktree_repos+=("$wt")
        done
    done
    repos+=("${worktree_repos[@]}")
fi

if [[ ${#repos[@]} -eq 0 ]]; then
    echo "No git repos found in: ${DIRS[*]}"
    exit 1
fi

echo "Found ${#repos[@]} repo(s). Playing each..."
echo ""

for repo in "${repos[@]}"; do
    # Get repo name and branch for display
    name=$(git -C "$repo" remote get-url origin 2>/dev/null | sed 's|.*/||;s|\.git$||' || basename "$repo")
    branch=$(git -C "$repo" branch --show-current 2>/dev/null || echo "?")

    # Dry run first for info
    printf "▸ %-30s [%s]\n" "$name" "$branch"
    echo '{"cwd":"'"$repo"'"}' | branch-tone hook
    echo ""
done

echo "Done."
