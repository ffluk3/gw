# shellcheck shell=bash
# gw shell integration.
#
#   source /path/to/gw/shell/gw.sh
#
# A child process cannot change its parent shell's directory, so `gw add --cd`
# is only a request: the binary prints the worktree path (the same contract as
# `--print-path`) and the `gw` function below performs the `cd`.
#
# `gw add` changes directory by default once this file is sourced. `--no-cd`
# stays put for one call. Set GW_CD=0 to make staying put the default instead.
#
# Every other invocation runs as a normal child process.

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

  # The pair overrides each other, so remember the last one given. This has to
  # match clap's `overrides_with` in crates/gw/src/cli.rs; otherwise the shell
  # and the binary disagree about the same command line.
  local override=""
  for arg in "$@"; do
    case "$arg" in
      --cd) override=enter ;;
      --no-cd) override=stay ;;
    esac
  done

  # `add` changes directory unless the last override said to stay, or GW_CD=0
  # turns the default off. GW_CD=1 is still accepted and matches the default.
  local cd_requested=0
  if [[ $subcommand == add ]]; then
    case "$override" in
      enter) cd_requested=1 ;;
      stay) cd_requested=0 ;;
      *) [[ ${GW_CD:-1} != 0 ]] && cd_requested=1 ;;
    esac
  fi

  if [[ $cd_requested -eq 1 ]]; then
    # Ask for the bare path so nothing else reaches the command substitution.
    #
    # This variable must not be named `path`: zsh ties `path` to `PATH`, so
    # `local path` would empty PATH and the `command gw` below would fail with
    # "command not found".
    local worktree
    worktree="$(command gw "$@" --print-path)" || return $?
    [[ -n "$worktree" ]] || return 1
    cd "$worktree" || return 1
    return 0
  fi

  command gw "$@"
}
