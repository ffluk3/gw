# Parse `gw` output in a script

Use this guide when a script reads `gw` output. Use JSON, never the table.

## Turn on JSON

```bash
gw status --json
gw repos --json
gw clean --dry-run --json
```

`--json` is a global flag. It works with every command.

`gw` also treats a non-terminal stdout as scripted for `gw clean`, which means
`gw clean` stays a dry run there even without `--yes`.

## Know the stable contract

The JSON shape is the contract. Column widths, emoji, and colors are not.

### `gw status --json`

```json
{
  "worktrees": [
    {
      "branch": "PROJ-123",
      "path": "worktrees/PROJ-123",
      "age_secs": 7200,
      "dirty": true,
      "dirty_state": "dirty",
      "ticket": "PROJ-123",
      "summary": "Add rate limiting",
      "kind": "linked",
      "ticket_state": { "status": "open" }
    }
  ]
}
```

### `gw repos --json`

```json
{
  "repos": [
    {
      "repo": "acme/widgets",
      "path": "/home/me/repos/acme/widgets",
      "worktrees": 4,
      "dirty": null,
      "last_commit_age_secs": 7200,
      "jira_projects": ["PROJ"]
    }
  ]
}
```

`dirty` is `null` unless you pass `--dirty`.

### `gw clean --json`

```json
{
  "entries": [
    {
      "target": { "kind": "worktree", "path": "...", "branch": "PROJ-1" },
      "action": "remove",
      "reason": "merged_pr",
      "description": "PR merged"
    }
  ],
  "counts": { "to_remove": 1, "to_prune": 0, "to_delete": 0, "recommended": 0, "kept": 3, "skipped": 0 }
}
```

## Handle exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success, including a dry run with nothing to do. |
| 1 | An operation failed. |
| 2 | You used a command or a flag incorrectly. |
| 3 | Some destructive actions failed. |

Check the exit code before you parse the body.

## Example

```bash
if ! gw clean --dry-run --json > plan.json; then
  echo "gw clean failed" >&2
  exit 1
fi

jq '.counts.to_remove' plan.json
```
