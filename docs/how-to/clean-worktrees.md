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

## Choose which worktrees to remove

```bash
gw clean --interactive
```

1. Read the table that `gw` prints.
2. Use the arrow keys to move in the list.
3. Press Space to mark a row.
4. Press Enter to apply the marked rows, or Esc to cancel.

The list has no marked rows at the start. The list contains only rows that
`gw` can act on, and it includes worktrees with a closed ticket. `gw` never
offers a dirty, unpublished, protected, or recently active worktree.

`--interactive` needs a terminal. It cannot be used with `--yes`, `--json`, or
`--delete-branches`. If you mark a `prune` row, `gw` drops the metadata of all
prunable worktrees, not only that row.

## Change the stale limit

The default stale limit is 7 days after the last commit. To change it for one
run:

```bash
gw clean --stale-days 14 --yes
```

To change it for everyone who uses this layout, put it in `.gw.toml` at the
layout root:

```toml
[clean]
stale_after = "5w"
```

A duration is a number and a unit: `s`, `m`, `h`, `d`, `w`, or `mo`. Write
`"3d"` for three days. A flag or `GW_STALE_DAYS` still wins over the file for
one run.

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

## Skip a lookup

```bash
gw clean --skip-provider github --yes
gw clean --skip-provider jira,github --yes
```

A skipped provider reports nothing, so `gw` removes fewer worktrees: without
change information it cannot see a merge, and without ticket information it
cannot see a closed ticket. Nothing is removed on a missing fact.

A skipped provider is not asked, and its absence never counts as a fact.

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
