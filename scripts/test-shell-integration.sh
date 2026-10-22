#!/usr/bin/env bash
# Exercise shell/gw.sh in every shell it claims to support.
#
# The wrapper has to work in both bash and zsh. zsh ties the `path` array to
# `PATH`, so an ordinary `local path` inside the function empties PATH and the
# binary it is about to run is suddenly unfindable. That failure is invisible
# to a bash-only test, which is why this script runs each case in both shells.
#
# Usage: test-shell-integration.sh <path-to-gw-binary>

set -euo pipefail

binary=${1:-}
[[ -n $binary && -x $binary ]] || {
  echo "usage: $0 <path-to-gw-binary>" >&2
  exit 2
}

# Absolute before anything changes directory: the cases below `cd` into the
# fixture, and a relative path would then point at nothing. A missing binary
# makes every "enters the worktree" case look like a failure to enter, so
# resolve it once here and assert it is really on PATH below.
binary=$(cd "$(dirname "$binary")" && pwd)/$(basename "$binary")

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
wrapper="$script_dir/../shell/gw.sh"
[[ -f $wrapper ]] || {
  echo "no shell integration at $wrapper" >&2
  exit 2
}

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# A minimal bare-repo layout, which is all `gw add` needs.
root="$tmp/layout"
mkdir -p "$root"
git init --bare -q "$root/.bare"
git --git-dir="$root/.bare" symbolic-ref HEAD refs/heads/main
work="$tmp/seed"
mkdir -p "$work"
printf 'seed\n' >"$work/README.md"
git --git-dir="$root/.bare" --work-tree="$work" add README.md
GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t \
  GIT_COMMITTER_EMAIL=t@t \
  git --git-dir="$root/.bare" --work-tree="$work" commit -qm seed

failures=0

# check <shell> <description> <expected-pwd-suffix|-> <env-assignment> <args...>
check() {
  local shell=$1 description=$2 expected=$3 env=$4
  shift 4

  # Run `gw` without discarding stderr. Silencing it hides the wrapper failing
  # outright, which looks exactly like the "stayed put" cases passing.
  local shell_err="$tmp/stderr.$shell"
  local actual
  actual=$(
    cd "$root" || exit 1
    PATH="$(dirname "$binary"):$PATH" "$shell" -f -c "
      command -v gw >/dev/null || { echo 'gw not on PATH' >&2; exit 127; }
      set -- $(printf '%q ' "$@")
      source $(printf '%q' "$wrapper")
      $env
      gw \"\$@\" >/dev/null
      printf '\n%s\n' \"\$?\"
      pwd -P
    " 2>"$shell_err"
  )

  # Output is "<blank>\n<gw exit status>\n<pwd>". Reading the status explicitly
  # means a case that failed to run cannot pass by comparing directories.
  local gw_status
  gw_status=$(sed -n '2p' <<<"$actual")
  actual=$(tail -n 1 <<<"$actual")

  local status=ok
  [[ $expected == - ]] && expected=$root
  # Temp roots may resolve through a symlink (/var on macOS), so compare
  # physical paths on both sides.
  local want
  want=$(cd "$expected" 2>/dev/null && pwd -P) || want=$expected
  [[ $gw_status == 0 && $actual == "$want" ]] || status=FAIL

  printf '%-5s %-6s %s\n' "$status" "$shell" "$description"
  if [[ $status == FAIL ]]; then
    printf '      want %s\ngot  %s\nexit %s\n' "$want" "$actual" "$gw_status"
    # Show the binary's own message rather than letting a hard failure look
    # like a navigation miss.
    [[ -s $shell_err ]] && sed 's/^/      stderr: /' "$shell_err"
    failures=$((failures + 1))
  fi
}

for shell in bash zsh; do
  command -v "$shell" >/dev/null 2>&1 || {
    printf 'SKIP  %-6s not installed\n' "$shell"
    continue
  }

  check "$shell" 'add enters the worktree by default' \
    "$root/worktrees/a" '' add a --no-fetch
  check "$shell" '--no-cd stays put' \
    - '' add b --no-fetch --no-cd
  check "$shell" 'GW_CD=0 stays put' \
    - 'export GW_CD=0' add c --no-fetch
  check "$shell" 'GW_CD=1 still enters' \
    "$root/worktrees/d" 'export GW_CD=1' add d --no-fetch
  # The last of the pair wins, matching clap's overrides_with. Without this,
  # the wrapper and the binary would disagree about the same command line.
  check "$shell" '--cd --no-cd stays put' \
    - '' add e --no-fetch --cd --no-cd
  check "$shell" '--no-cd --cd enters' \
    "$root/worktrees/f" '' add f --no-fetch --no-cd --cd
  check "$shell" 'other subcommands never move' \
    - '' status
done

if [[ $failures -gt 0 ]]; then
  printf '\n%d case(s) failed\n' "$failures" >&2
  exit 1
fi
echo "shell integration ok"
