# JSON output

`--json` is global. The shapes on this page are the stable contract that
scripts and tests may rely on. Table layout, column widths, emoji, and colors
are not part of the contract.

## Enumerations

These string values appear in more than one shape.

`kind` (worktree):

| Value | Meaning |
| --- | --- |
| `main` | The bare repository itself |
| `linked` | A normal linked worktree |
| `harness` | An agent-harness worktree, which `gw` never touches |
| `prunable` | git reports the directory as prunable |

`dirty_state`:

| Value | Meaning |
| --- | --- |
| `clean` | No uncommitted changes |
| `dirty` | Uncommitted changes |
| `unknown` | The status could not be read. Treated as dirty when deciding removal. |

`ticket_state.status`:

| Value | Meaning |
| --- | --- |
| `not_checked` | No ticket key, or lookups disabled |
| `open` | Not in the `done` status category |
| `closed` | In the `done` status category |

A closed ticket also carries `resolution`, which may be `null`.

## `gw status --json`

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
      "ticket_state": { "status": "closed", "resolution": "Done" }
    }
  ]
}
```

| Field | Type | Notes |
| --- | --- | --- |
| `branch` | string | `"(detached)"` when there is no branch. |
| `path` | string | Relative to the layout root when possible. |
| `age_secs` | number | Age of the last commit. |
| `dirty` | boolean | `dirty_state` is not `clean`. |
| `dirty_state` | string | See above. |
| `ticket` | string or null | Ticket key found in the branch name. |
| `summary` | string or null | JIRA summary, when available. |
| `kind` | string | See above. |
| `ticket_state` | object | See above. |

The bare repository itself is not listed.

## `gw repos --json`

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

| Field | Type | Notes |
| --- | --- | --- |
| `repo` | string | `owner/name`, or `name` for a flat layout. |
| `path` | string | Absolute layout root. |
| `worktrees` | number | Linked worktrees, excluding harness worktrees. |
| `dirty` | number or null | `null` unless `--dirty`. |
| `last_commit_age_secs` | number or null | Newest commit across worktrees. |
| `jira_projects` | array | Declared in that layout's `.gw.toml`. |

## `gw clean --json`

```json
{
  "entries": [
    {
      "target": { "kind": "worktree", "path": "/repo/worktrees/PROJ-1", "branch": "PROJ-1" },
      "action": "remove",
      "reason": "merged_pr",
      "description": "PR merged"
    },
    {
      "target": { "kind": "branch", "name": "PROJ-2" },
      "action": "delete",
      "reason": "stale",
      "tip": "9df03e38ae0",
      "description": "stale (3w ago)"
    }
  ],
  "counts": {
    "to_remove": 1,
    "to_prune": 0,
    "to_delete": 1,
    "recommended": 0,
    "kept": 3,
    "skipped": 0
  }
}
```

`target.kind` is `worktree` or `branch`.

`action`:

| Value | Meaning | Destructive |
| --- | --- | --- |
| `remove` | Remove a checked-out worktree | yes |
| `delete` | Delete a branch ref; carries `tip` | yes |
| `prune` | Drop metadata for a missing worktree | no |
| `keep` | Nothing to do | no |
| `skip` | Not eligible to manage | no |
| `recommend` | Would remove under a different flag | no |

`reason`:

| Value | Meaning |
| --- | --- |
| `protected` | On the protection list |
| `harness_worktree` | Owned by the agent harness |
| `detached_head` | No branch checked out |
| `prunable` | git reports the directory as prunable |
| `has_unpublished_commits` | Some commits are on no remote |
| `has_uncommitted_changes` | The worktree is dirty |
| `status_unknown` | The state could not be read |
| `recently_active` | Newer than the stale limit |
| `stale` | Older than the stale limit |
| `merged_pr` | GitHub reports a merged PR |
| `ticket_closed` | The JIRA ticket is closed |
| `no_merged_pr` | No reason to remove was found |
| `no_merged_pr_ticket_closed` | No merged PR, but the ticket is closed |

`stale`, `recently_active`, and `ticket_closed` include extra fields
(`age_secs` or `resolution`) alongside `reason`.
