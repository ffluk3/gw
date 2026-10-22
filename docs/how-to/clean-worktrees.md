# Remove stale and merged worktrees

Use this guide to remove worktrees and branches that you no longer need.

## Look at the plan first

```bash
gw clean --dry-run
```

`gw clean` prints the plan and changes nothing. Read each row.

## Apply the plan

```bash
gw clean --yes
```

`--yes` confirms without a prompt. Without `--yes`, `gw` applies the plan only
when stdout is a terminal and `--json` is absent. A piped run, a scripted run,
or a JSON run stays a dry run.

## Change the stale limit

The default stale limit is 7 days after the last commit:

```bash
gw clean --stale-days 14 --yes
```

## Remove worktrees with a closed ticket

By default, `gw clean` lists worktrees with a closed ticket as
recommendations. It does not remove them. To remove them:

```bash
gw clean --closed --yes
```

## Delete stale orphan branches

```bash
gw clean --delete-branches --yes
```

An orphan branch is a local branch that no worktree checks out. `gw` deletes
the branch only when each of these conditions is true:

- each commit is on a remote
- the branch is not protected
- the last commit is older than the stale limit

## Skip the GitHub lookup

```bash
gw clean --skip-github --yes
```

Without GitHub, `gw` cannot see merged PRs, so it removes fewer worktrees. You
can also set `GW_SKIP_GITHUB=1`.

## Restore an object that you deleted

`gw clean` prints a restore command for each deleted branch:

```text
Deleted branches can be restored with:
   git branch PROJ-1 9df03e38ae0
```

See [Restore a deleted branch](restore-a-branch.md).

## Read the safety rules

Before your first real run, read the
[`gw clean` safety rules](../explanation/clean-safety.md). The page gives each
condition that must be true before `gw` destroys anything.
