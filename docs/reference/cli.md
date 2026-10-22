# Command-line reference

```text
gw [GLOBAL OPTIONS] <COMMAND> [OPTIONS]
```

## Global options

| Option | Environment | Meaning |
| --- | --- | --- |
| `--repo <PATH\|SLUG>` | `GW_REPO` | Layout root, `.bare` path, or `owner/name` resolved under the repos root. |
| `--json` | — | Force machine-readable output. |
| `--no-color` | `NO_COLOR` | Disable color and emoji. |
| `--jira-project <KEY>` | `GW_JIRA_PROJECT` | Ticket prefixes a branch may reference. Comma-separated, repeatable. |
| `--jira-url <URL>` | `GW_JIRA_BASE_URL` | JIRA base URL. Must be `https://`. |

`--help` and `--version` are provided by the argument parser.

## Where commands can run

`status`, `add`, and `clean` need a layout. They find it by walking up from the
current directory until they reach a directory that contains `.bare/HEAD`.
`--repo` overrides discovery.

`init` and `repos` do not need a layout.

## `gw init <REMOTE>`

Create a bare-repo layout.

| Option | Meaning |
| --- | --- |
| `--dir <DIR>` | Layout root. Default: `<repos>/<owner>/<name>`. |
| `--branch <BRANCH>` | First checkout. Default: the remote's default branch. |
| `--dry-run` | Print the plan. Write nothing. |
| `--json` | Machine-readable result. |

Steps: clone `--bare` into `<root>/.bare`, set the
`+refs/heads/*:refs/remotes/origin/*` refspec, fetch with prune, then add a
worktree at `<root>/worktrees/<branch>`.

The command fails if `<root>/.bare/HEAD` already exists.

## `gw status`

| Option | Meaning |
| --- | --- |
| `--no-jira` | Skip the JIRA summary lookup. |

Columns: `BRANCH`, `LAST COMMIT`, `DIRTY`, `STATE`, `SUMMARY`.

`STATE` is one of `harness`, `prunable`, `closed`, or empty. `DIRTY` shows `●`
when the worktree has uncommitted changes.

The JIRA lookup is best-effort. A failure prints a warning and leaves
`SUMMARY` empty.

## `gw add <BRANCH>`

Create or reuse a worktree.

| Option | Meaning |
| --- | --- |
| `--print-path` | Print the worktree path only. Used by the shell wrapper. Overrides `--json`. |
| `--cd` | Change the calling shell into the worktree. Needs the shell integration. |
| `--no-cd` | Stay in the current directory. Overrides `GW_CD=1` and the last of the `--cd` pair wins. |
| `--no-fetch` | Do not fetch from origin first. |
| `--json` | Machine-readable result, including whether the worktree was created. |

`--cd` cannot be performed by the binary: a process cannot change its parent
shell's directory. Run standalone, `gw add --cd` prints the bare path, exactly
like `--print-path`, and nothing else. The `gw` shell function from
[Shell integration](#shell-integration) reads that path and performs the `cd`.
Set `GW_CD=1` to make a bare `gw add` navigate as well; `--no-cd` opts out.

`--cd` and `--no-cd` override each other, so the last one on the command line
wins. Both override `--json` the same way `--print-path` does, because a shell
function must receive the path and nothing else.

Strategy selection:

1. A worktree already checks out the branch: reuse it.
2. A local branch exists: move it to `origin/<branch>` (when a remote ref
   exists), then check it out.
3. Only a remote ref exists: create a local branch tracking it.
4. Neither exists: create a local branch from `HEAD`.

The worktree directory is `<worktrees>/<branch>`, with slashes preserved.
Branch names are validated before use.

## `gw clean`

Remove worktrees and delete branches.

| Option | Environment | Meaning |
| --- | --- | --- |
| `--dry-run` | — | Plan only. Default when stdout is not a terminal, or when `--json` is given. |
| `--yes`, `-y` | — | Apply without prompting. |
| `--closed` | — | Also remove worktrees whose ticket is closed. |
| `--delete-branches` | — | Also delete stale, fully published orphan branches. |
| `--stale-days <N>` | `GW_STALE_DAYS` | Stale limit in days. Default 7. |
| `--skip-github` | `GW_SKIP_GITHUB` | Skip the merged-PR lookup. |
| `--no-jira` | — | Skip the JIRA lookup. |
| `--json` | — | Machine-readable plan, with `applied` saying whether it ran. |

The command fetches with prune first. A fetch failure prints a warning and
continues against cached refs.

`gw` removes a worktree only when every condition in
[the safety rules](../explanation/clean-safety.md) holds. It never removes a
worktree under `.claude/worktrees/`.

After a deletion, `gw` prints `git branch <name> <tip>` for each deleted
branch. In JSON mode those hints go to stderr, next to the other progress
lines, so stdout stays a single document.

## `gw repos`

List layouts under the repos root. Read-only.

| Option | Meaning |
| --- | --- |
| `--dirty` | Count dirty worktrees. Runs one `git status` per worktree. |
| `--json` | Machine-readable output. |

Columns: `REPO`, `WORKTREES`, [`DIRTY`], `LAST COMMIT`, `JIRA`.

`LAST COMMIT` is the newest commit across the layout's worktrees. `JIRA` lists
the project keys declared in that layout's `.gw.toml`.

## Shell integration

`gw.sh` defines:

| Function | Behavior |
| --- | --- |
| `gw` | Runs `gw`. For `gw add --cd` (or `GW_CD=1`), it `cd`s into the worktree afterwards. |
| `gwadd <branch>` | `gw add --cd <branch>`: create or reuse the worktree, then `cd` into it. |
| `gwstatus` | `gw status "$@"` |
| `gwclean` | `gw clean "$@"` |

Source it from your shell profile. The script returns early if `gw` is not on
`PATH`.

The `gw` function shadows the binary so it can `cd` in your shell. It passes
every argument through unchanged. It only navigates for `add`, and only when
`--cd` is given or `GW_CD=1` is set; `--no-cd` turns navigation off for that
one call. `gwclean` and `gwstatus` always call the binary directly.

The installer copies the file to
`${XDG_DATA_HOME:-~/.local/share}/gw/gw.sh`. A source install leaves the
original at `shell/gw.sh` in the checkout as well.
