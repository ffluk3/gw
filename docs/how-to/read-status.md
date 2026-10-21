# Read the status table

Use this guide to understand what `gw status` shows.

## Run it

```bash
gw status
```

```text
BRANCH              LAST COMMIT  DIRTY  STATE    SUMMARY
PROJ-47383            2d ago       ●               Add rate limiting
develop             1h ago
```

## Read each column

| Column | Meaning |
| --- | --- |
| `BRANCH` | The branch checked out in the worktree. |
| `LAST COMMIT` | Age of the newest commit in that worktree. `—` means `gw` could not read it. |
| `DIRTY` | `●` means uncommitted changes. A blank cell means clean. |
| `STATE` | `harness`, `prunable`, or `closed`, when applicable. |
| `SUMMARY` | The JIRA summary for the ticket key in the branch name. |

`harness` marks a worktree that the agent harness owns. `gw` never touches
those. `prunable` means git reports that the directory is gone.

## Skip the JIRA lookup

```bash
gw status --no-jira
```

Use this when JIRA is slow or not configured.

## Get machine-readable output

```bash
gw status --json
```

The JSON form is a stable contract. The table spacing is not. See
[Parse `gw` output in a script](script-with-json.md).

## Find out which repository you are in

The status table does not name the layout. To confirm it, use `gw repos`, which
prints the root it scans, or pass `--repo <root>` explicitly. If discovery
picks the wrong layout, `gw` will otherwise show you worktrees from a
repository you did not intend.
