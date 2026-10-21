# Remove stale and merged worktrees

Use this guide to remove worktrees and branches you no longer need.

## Look at the plan first

```bash
gw clean --dry-run
```

`gw clean` prints the plan and changes nothing. Read every row.

## Apply the plan

```bash
gw clean --yes
```

`--yes` confirms without a prompt. Without `--yes`, `gw` applies the plan only
when stdout is a terminal. A piped or scripted run stays a dry run.

## Change what counts as stale

The default stale limit is 7 days since the last commit:

```bash
gw clean --stale-days 14 --yes
```

## Remove worktrees whose ticket is closed

By default, `gw clean` lists closed-ticket worktrees as recommendations. It
does not remove them. To remove them:

```bash
gw clean --closed --yes
```

## Delete stale orphan branches

```bash
gw clean --delete-branches --yes
```

An orphan branch is a local branch that no worktree checks out. `gw` deletes it
only when its commits are all reachable from a remote, it is not protected, and
its last commit is older than the stale limit.

## Skip the GitHub lookup

```bash
gw clean --skip-github --yes
```

Without GitHub, `gw` cannot see merged PRs, so it removes less. You can also
set `GW_SKIP_GITHUB=1`.

## Restore something you deleted

`gw clean` prints a restore command for each deleted branch:

```text
Deleted branches can be restored with:
   git branch PROJ-1 9df03e38ae0
```

See [Restore a deleted branch](restore-a-branch.md).

## Read the safety rules

Before your first real run, read
[`gw clean` safety rules](../explanation/clean-safety.md). It lists every
condition that must be true before `gw` destroys anything.
