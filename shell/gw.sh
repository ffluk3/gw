# shellcheck shell=bash
# gw shell integration.
#
#   source /path/to/gw/shell/gw.sh
#
# A child process cannot change its parent shell's directory, so `gw add --cd`
# is only a request: the binary prints the worktree path (the same contract as
# `--print-path`) and the `gw` function below performs the `cd`.
#
# Every other invocation runs as a normal child process. Setting GW_CD=1 makes
# a bare `gw add` change directory too; `--no-cd` always wins.

command -v gw >/dev/null 2>&1 || return 0

gw() {
  local arg subcommand="" skip_next=0
  for arg in "$@"; do
    if [[ $skip_next -eq 1 ]]; then
      skip_next=0
      continue
    fi
    case "$arg" in
      # Global options that take a value; the value is not the subcommand.
      --repo | --skip-provider) skip_next=1 ;;
      -*) ;;
      *)
        subcommand=$arg
        break
        ;;
    esac
  done

  local cd_requested=0 has_no_cd=0
  for arg in "$@"; do
    case "$arg" in
      --cd) cd_requested=1 ;;
      --no-cd) has_no_cd=1 ;;
    esac
  done

  if [[ $subcommand == add && $has_no_cd -eq 0 ]]; then
    if [[ $cd_requested -eq 0 && ${GW_CD:-0} == 1 ]]; then
      cd_requested=1
    fi
  else
    cd_requested=0
  fi

  if [[ $cd_requested -eq 1 ]]; then
    # Ask for the bare path so nothing else reaches the command substitution.
    local path
    path="$(command gw "$@" --print-path)" || return $?
    [[ -n "$path" ]] || return 1
    cd "$path" || return 1
    return 0
  fi

  command gw "$@"
}

# `gwadd` keeps the old muscle memory: create or reuse the worktree, then cd.
gwadd() {
  if [[ $# -eq 0 ]]; then
    echo "usage: gwadd <branch>" >&2
    return 1
  fi
  # GW_CD makes the `gw` function navigate without forwarding --cd, so any
  # other arguments (--no-fetch, --repo) pass through unchanged.
  GW_CD=1 gw add "$@"
}

# Shorthands matching the previous sourced functions.
gwclean() { command gw clean "$@"; }
gwstatus() { command gw status "$@"; }
