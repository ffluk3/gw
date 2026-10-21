# shellcheck shell=bash
# gw shell integration.
#
#   source /path/to/gw/shell/gw.sh
#
# `gw add` cannot change its parent shell's directory, so this wrapper keeps
# the old `gwadd` muscle memory: create or reuse the worktree, then cd into it.
# Everything else runs as a normal child process.

command -v gw >/dev/null 2>&1 || return 0

gwadd() {
  if [[ $# -eq 0 ]]; then
    echo "usage: gwadd <branch>" >&2
    return 1
  fi

  local path
  path="$(command gw add --print-path "$@")" || return $?
  [[ -n "$path" ]] || return 1
  cd "$path" || return 1
}

# Shorthands matching the previous sourced functions.
gwclean() { command gw clean "$@"; }
gwstatus() { command gw status "$@"; }
