#!/usr/bin/env bash
# @Author: benbenbang
# Install local git guards for local-only branches:
#   1. pre-push             - refuse to push a protected branch to any remote
#   2. reference-transaction - refuse to delete a protected branch locally
#
# The protected set lives in scripts/protected-branches.txt (version controlled).
# The hooks read that file at runtime, so add/remove branches by editing it -
# no reinstall required. Re-run this script only to (re)create the hooks after
# a fresh clone or when adding a worktree. Idempotent.
#
# Usage:
#   scripts/install_branch_guards.sh            # install/refresh the hooks
#   scripts/install_branch_guards.sh feat/x     # also append feat/x to the list

set -euo pipefail

GREEN='\033[1;32m'
CYAN='\033[0;36m'
NC='\033[0m'

GIT_ROOT=$(git rev-parse --show-toplevel)
HOOKS_DIR="$(git rev-parse --git-common-dir)/hooks"   # shared across worktrees
LIST_REL="scripts/protected-branches.txt"
LIST="${GIT_ROOT}/${LIST_REL}"
mkdir -p "${HOOKS_DIR}"

# Seed the list file if it does not exist.
if [ ! -f "${LIST}" ]; then
	cat > "${LIST}" <<-EOF
	# Branches protected by scripts/install_branch_guards.sh
	# One branch name per line (e.g. feat/foo). '#' and blank lines are ignored.
	EOF
fi

# Optionally append branches passed as arguments (skip duplicates).
for b in "$@"; do
	if ! grep -qxF "${b}" "${LIST}"; then
		printf '%s\n' "${b}" >> "${LIST}"
		printf "added ${CYAN}%s${NC} to %s\n" "${b}" "${LIST_REL}"
	fi
done

# --- pre-push: block pushing any protected branch; keep the Git LFS hook ---
cat > "${HOOKS_DIR}/pre-push" <<EOF
#!/bin/sh
# branch-guard: refuse to push a protected branch (see ${LIST_REL}) to any remote.
root=\$(git rev-parse --show-toplevel 2>/dev/null) || exit 0
list="\$root/${LIST_REL}"
input=\$(cat)   # consume the ref list git feeds us on stdin

is_protected() {
	[ -f "\$list" ] || return 1
	while IFS= read -r line; do
		line=\$(printf '%s' "\$line" | tr -d '[:space:]')
		case "\$line" in ''|\\#*) continue ;; esac
		[ "refs/heads/\$line" = "\$1" ] && return 0
	done < "\$list"
	return 1
}

blocked=\$(printf '%s\n' "\$input" | awk '{print \$1}' | while read -r ref; do
	is_protected "\$ref" && printf '%s\n' "\$ref"
done)
if [ -n "\$blocked" ]; then
	printf >&2 "\n\033[31mBLOCKED:\033[0m refusing to push protected branch(es) to a remote:\n%s\n" "\$blocked"
	printf >&2 "Edit ${LIST_REL} to change the protected set, or delete .git/hooks/pre-push to override.\n\n"
	exit 1
fi

# --- Git LFS hook (stdin replayed unchanged) ---
command -v git-lfs >/dev/null 2>&1 || { printf >&2 "\n%s\n\n" "This repository is configured for Git LFS but 'git-lfs' was not found on your path. If you no longer wish to use Git LFS, remove this hook by deleting the 'pre-push' file in the hooks directory (set by 'core.hookspath'; usually '.git/hooks')."; exit 2; }
printf '%s\n' "\$input" | git lfs pre-push "\$@"
EOF
chmod +x "${HOOKS_DIR}/pre-push"

# --- reference-transaction: block local deletion of any protected branch ---
cat > "${HOOKS_DIR}/reference-transaction" <<EOF
#!/bin/sh
# branch-guard: refuse to delete a protected branch (see ${LIST_REL}).
# Fires on every ref update; only acts on deletion of a protected ref.
[ "\$1" = "prepared" ] || exit 0
root=\$(git rev-parse --show-toplevel 2>/dev/null) || exit 0
list="\$root/${LIST_REL}"
[ -f "\$list" ] || exit 0

is_protected() {
	while IFS= read -r line; do
		line=\$(printf '%s' "\$line" | tr -d '[:space:]')
		case "\$line" in ''|\\#*) continue ;; esac
		[ "refs/heads/\$line" = "\$1" ] && return 0
	done < "\$list"
	return 1
}

while read -r old new ref; do
	case "\$new" in
		"" | *[!0]*) continue ;;   # empty or non-zero => not a deletion
	esac
	if is_protected "\$ref"; then
		printf >&2 "\n\033[31mBLOCKED:\033[0m refusing to delete protected branch %s\n" "\$ref"
		printf >&2 "Edit ${LIST_REL} to change the protected set, or delete .git/hooks/reference-transaction to override.\n\n"
		exit 1
	fi
done
exit 0
EOF
chmod +x "${HOOKS_DIR}/reference-transaction"

printf "${GREEN}Installed branch guards${NC} reading from ${CYAN}%s${NC}\n" "${LIST_REL}"
printf "  push guard   : %s\n" "${HOOKS_DIR}/pre-push"
printf "  delete guard : %s\n" "${HOOKS_DIR}/reference-transaction"
printf "  protected    : %s\n" "$(grep -vE '^\s*(#|$)' "${LIST}" | tr '\n' ' ')"
