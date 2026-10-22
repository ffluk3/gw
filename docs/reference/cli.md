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
| `--jira-project <KEY>` | `GW_JIRA_PROJECT` | Ticket prefixes that a branch can reference. Comma-separated, repeatable. |
| `--jira-url <URL>` | `GW_JIRA_BASE_URL` | JIRA base URL. Must be `https://`. |

The argument parser provides `--help` and `--version`.

## Where commands can run

`status`, `add`, and `clean` need a layout. They find the layout by walking up
from the current directory until they reach a directory that contains
`.bare/HEAD`. `--repo` overrides discovery.

`init` and `repos` do not need a layout.

## `gw init <REMOTE>`

Make a bare-repo layout.

| Option | Meaning |
| --- | --- |
| `--dir <DIR>` | Layout root. Default: `<repos>/<owner>/<name>`. |
| `--branch <BRANCH>` | First checkout. Default: the default branch of the remote. |
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

Make or reuse a worktree.

| Option | Meaning |
| --- | --- |
| `--print-path` | Print the worktree path only. Used by the shell wrapper. Overrides `--json`. |
| `--cd` | Change the calling shell into the worktree. Needs the shell integration. |
| `--no-cd` | Stay in the current directory. Overrides `GW_CD=1`. The last of the `--cd` pair wins. |
| `--no-fetch` | Do not fetch from origin first. |
| `--json` | Machine-readable result, including whether `gw` created the worktree. |

The binary cannot perform `--cd`, because a process cannot change the directory
of its parent shell. Run standalone, `gw add --cd` prints the bare path,
exactly like `--print-path`, and nothing else. The `gw` shell function from
[Shell integration](#shell-integration) reads that path and changes the
directory. Set `GW_CD=1` to make a bare `gw add` change directory as well.
`--no-cd` cancels this behavior.

`--cd` and `--no-cd` override each other, so the last one on the command line
wins. Both override `--json` the same way that `--print-path` does, because a
shell function must receive the path and nothing else.

Strategy selection:

1. A worktree already checks out the branch: reuse it.
2. A local branch exists: move it to `origin/<branch>` (when a remote ref
   exists), then check it out.
3. Only a remote ref exists: make a local branch that tracks it.
4. Neither exists: make a local branch from `HEAD`.

The worktree directory is `<worktrees>/<branch>`, with slashes preserved. `gw`
validates branch names before it uses them.

## `gw clean`

Remove worktrees and delete branches.

| Option | Environment | Meaning |
| --- | --- | --- |
| `--dry-run` | — | Plan only. Default when stdout is not a terminal, or when `--json` is given. |
| `--yes`, `-y` | — | Apply without a prompt. |
| `--closed` | — | Also remove worktrees whose ticket is closed. |
| `--delete-branches` | — | Also delete stale orphan branches that are fully published. |
| `--stale-days <N>` | `GW_STALE_DAYS` | Stale limit in days. Default 7. |
| `--skip-github` | `GW_SKIP_GITHUB` | Skip the merged-PR lookup. |
| `--no-jira` | — | Skip the JIRA lookup. |
| `--json` | — | Machine-readable plan, with `applied` to say whether it ran. |

The command fetches with prune first. A fetch failure prints a warning and
continues against cached refs.

`gw` removes a worktree only when each condition in
[the safety rules](../explanation/clean-safety.md) is true. It never removes a
worktree under `.claude/worktrees/`.

After a deletion, `gw` prints `git branch <name> <tip>` for each deleted
branch. In JSON mode, those hints go to stderr, with the other progress lines,
so stdout stays a single document.

## `gw repos`

List the layouts under the repos root. Read-only.

| Option | Meaning |
| --- | --- |
| `--dirty` | Count dirty worktrees. Runs one `git status` for each worktree. |
| `--json` | Machine-readable output. |

Columns: `REPO`, `WORKTREES`, [`DIRTY`], `LAST COMMIT`, `JIRA`.

`LAST COMMIT` is the newest commit across the worktrees of the layout. `JIRA`
lists the project keys that the `.gw.toml` of that layout declares.

## Shell integration

`gw.sh` defines:

| Function | Behavior |
| --- | --- |
| `gw` | Runs `gw`. For `gw add --cd` (or `GW_CD=1`), it changes into the worktree afterwards. |
| `gwadd <branch>` | `gw add --cd <branch>`: make or reuse the worktree, then change into it. |
| `gwstatus` | `gw status "$@"` |
| `gwclean` | `gw clean "$@"` |

Source the file from your shell profile. The script returns early if `gw` is
not on `PATH`.

The `gw` function shadows the binary so that it can change directory in your
shell. It passes each argument through unchanged. It changes directory only for
`add`, and only when you give `--cd` or set `GW_CD=1`. `--no-cd` turns off this
behavior for one call. `gwclean` and `gwstatus` always call the binary
directly.

The installer copies the file to
`${XDG_DATA_HOME:-~/.local/share}/gw/gw.sh`. A source install also keeps the
original at `shell/gw.sh` in the checkout.
