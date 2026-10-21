#!/usr/bin/env bash
# Point git at the checked-in hooks in .githooks/.
#
# Run this once after cloning. It is idempotent.
#
#   ./scripts/install-hooks.sh
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

git config core.hooksPath .githooks
chmod +x .githooks/* scripts/*.sh

echo "Hooks installed: core.hooksPath=$(git config --get core.hooksPath)"
echo "Each commit is now checked against the Conventional Commits format."
