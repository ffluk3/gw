---
name: gw
description: Manage git worktrees in a bare-repo layout with the `gw` CLI. Use to give each task or agent its own checkout, to inspect worktree status as JSON, and to safely remove merged or stale worktrees. Activate when a repository has `.bare/` plus `worktrees/`, when switching branches would disturb another agent's work, when asked to clean up worktrees or branches, or when the user mentions gw.
license: MIT
compatibility: Requires git and the `gw` CLI on PATH. Optional gh enables merged-PR detection in `gw clean`; optional curl plus JIRA credentials enable ticket summaries.
metadata:
  version: "0.1.0"
---

# gw: one worktree per branch

`gw` manages a bare-repo layout:

```text
<root>/
  .bare/               the bare repository
  worktrees/<branch>/  one checkout per branch
```

A branch is checked out in at most one worktree. To change branch, change
directory. Do not use `git checkout` or `git switch` to move work between
branches: in a layout, that strands or collides with another checkout.

## Confirm you are in a layout

```bash
gw status --json --skip-provider jira,github
```

Exit 0 means the current directory is inside a layout (any subdirectory
works). Exit 1 means it is not: a normal `.git` clone is rejected by design.
Do not "fix" that by pointing `gw` at the clone. Use `gw init` only for a
repository that has no checkout yet, and ask before creating one.

From outside any layout, address a known one by name or path:

```bash
gw --repo owner/name status --json
GW_REPO=owner/name gw add PROJ-1234 --print-path
```

Names resolve under `GW_REPOS_ROOT` (default `~/repos`).

## Give a task its own checkout

```bash
worktree="$(gw add --print-path PROJ-1234)"
```

- The command prints the absolute worktree path and nothing else on stdout.
- It is idempotent: it reuses a worktree that already checks out the branch.
- It fetches from origin first. Pass `--no-fetch` when offline or in a sandbox.
- The directory mirrors the branch name: `feature/x` becomes
  `worktrees/feature/x`.

Use the returned path in `git -C "$worktree" ...` or as an absolute path in
later commands. Do not assume a `cd` persists between tool calls.

In an interactive shell with the integration sourced, a bare `gw add` changes
into the worktree. A non-interactive tool call gets no navigation from it, so
always use `--print-path` and capture stdout. Name that shell variable
`worktree`, not `path`: zsh ties `path` to `PATH`, and assigning it replaces
the command search path.

`gw add` selects the branch source in this order: an existing worktree, a
remote-tracking branch, a local branch moved to `origin/<branch>`, or a new
branch from `HEAD`. Use it instead of raw `git worktree add`: it applies the
path convention and branch-tracking rules, and its result is what `gw clean`
expects to manage.

## Inspect state

```bash
gw status --json          # every worktree in the layout
gw repos --json --dirty   # every layout under the repos root
```

Useful `gw status` fields per worktree:

| Field | Meaning |
| --- | --- |
| `branch`, `path` | Checkout identity. `path` is relative to the layout root. |
| `age_secs` | Age of the newest commit, not of filesystem activity. |
| `dirty_state` | `clean`, `dirty`, or `unknown`. `unknown` is treated as dirty. |
| `kind` | `linked`, `harness`, or `prunable`. Harness worktrees are agent-owned. |
| `ticket`, `summary`, `ticket_state` | JIRA data when configured; best-effort. |

Read JSON, never the table: column layout and emoji are not a contract.

## Finish work, then clean up

The agent's job is to leave a worktree eligible for cleanup: commit what
should survive and push it. A worktree with uncommitted changes or commits
reachable from no remote is kept. That is the safety policy, not a failure.

```bash
# 1. Preview. Always. This writes nothing.
gw clean --dry-run --json --no-color

# 2. Read the entries and counts. Removal requires every protecting check to
#    pass (branch attached, not protected, HEAD readable, all commits
#    published, tree clean) plus one destroying fact: a merged PR, or age
#    beyond the stale limit. With --closed, a closed ticket also qualifies;
#    without it, a closed ticket is only a recommendation.

# 3. Apply only with explicit human approval.
gw clean --yes
```

Safety rules for agents:

- **Never run `gw clean --yes` unattended or as a "tidy up" step.** Worktree
  removal cannot be undone by `gw`. A deleted branch is recoverable only from
  the `git branch <name> <tip>` line that the command prints.
- `--yes` is the only way to apply a plan from a non-interactive shell.
  Without it, stdout is not a TTY, so `gw clean` is a dry run. Do not read
  that no-op as "cleaned".
- `--delete-branches` also deletes stale orphan branches. Do not add it
  without approval.
- `--closed` removes worktrees whose ticket is in JIRA's `done` category.
  Without it those entries are recommendations only.
- Exit 3 means some destructive actions failed and some succeeded. Report the
  partial result; do not retry blindly.
- `--skip-provider <id>` hides that provider's facts: skipping `github`
  hides merged PRs, skipping `jira` hides closed tickets and summaries. Use it
  only when `gh`, `curl`, or credentials are unavailable, and say that the
  preview was reduced.

## Gotchas

- `gw add` has no JSON mode. `--json` on `gw add` still prints prose; use
  `--print-path` and capture stdout only.
- A bare `gw add` changes directory only in an interactive shell that sourced
  the integration. A non-interactive tool call gets no navigation, so use
  `--print-path` instead. Standalone `--cd` prints the path on stdout and warns
  on stderr.
- Some releases print a human progress line to stdout before the
  `gw clean --json` document. If `jq` cannot parse the stream, strip
  everything before the first `{`:
  `gw clean --dry-run --json | sed -n '/^{/,$p' | jq .counts`.
- In `gw clean --json`, an entry's `reason` can be an object
  (`{"reason":"merged_pr", ...}`) rather than a bare string. Read
  `(.reason.reason // .reason)` so both shapes work.
- Worktrees whose path contains `/.claude/worktrees/` are harness-owned.
  `gw clean` never removes them and `gw repos` does not count them. If you
  created a checkout with an agent harness, clean it with that harness, not
  `gw`.
- A branch checked out in one worktree cannot be checked out again. A failed
  fast-forward usually means another worktree holds the branch; name it in
  the report instead of forcing anything.
- `.gw.toml` lives at the layout root beside `.bare`, outside every checkout.
  It is machine-local and is not part of the repository. Never commit it.
- Fetch failures during `gw add` or `gw clean` print a warning and continue
  against cached refs. Say so when reporting results; "no merged PR found"
  may mean the lookup never ran.
- Restoration data is printed, not stored in a file. Capture clean's stdout
  when applying a plan.
- Do not use `git branch -D` or `git worktree remove` to clean up. They skip
  the published-commits and dirty-tree checks. Removal policy belongs to
  `gw clean`.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success, including a dry run with nothing to do. |
| 1 | An operation failed (not a layout, git failure, lookup failure). |
| 2 | Usage or configuration error, including a bad `.gw.toml`. |
| 3 | Partial destructive failure (some removals succeeded, some failed). |

Check the exit code before parsing output, and report the code alongside any
result.

## Setting up a new layout

Only when the repository has no checkout yet and the user asks for one:

```bash
gw init <remote-url> --dir <root> --branch <branch> --dry-run   # preview
gw init <remote-url> --dir <root> --branch <branch>
```

It fails if `<root>/.bare/HEAD` already exists. For flags, environment
variables, and the full safety model, see the project documentation:
[CLI reference](https://github.com/ffluk3/gw/blob/main/docs/reference/cli.md),
[JSON output](https://github.com/ffluk3/gw/blob/main/docs/reference/json.md),
and [clean safety rules](https://github.com/ffluk3/gw/blob/main/docs/explanation/clean-safety.md).
