# JSON output

`--json` is global. The shapes on this page are the stable contract that
scripts and tests can use. The table layout, column widths, emoji, and colors
are not part of the contract.

## Enumerations

These string values appear in more than one shape.

`kind` (worktree):

| Value | Meaning |
| --- | --- |
| `main` | The bare repository itself |
| `linked` | A normal linked worktree |
| `harness` | A worktree that the agent harness owns. `gw` never touches it. |
| `prunable` | Git reports the directory as prunable |

`dirty_state`:

| Value | Meaning |
| --- | --- |
| `clean` | No uncommitted changes |
| `dirty` | Uncommitted changes |
| `unknown` | `gw` could not read the status. `gw` treats this value as dirty when it decides removal. |

`ticket_state.status`:

| Value | Meaning |
| --- | --- |
| `not_checked` | No ticket key, or lookups are disabled |
| `open` | Not in the `done` status category |
| `closed` | In the `done` status category |

A closed ticket also has `resolution`, which can be `null`.

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
| `age_secs` | number | The age of the last commit. |
| `dirty` | boolean | `dirty_state` is not `clean`. |
| `dirty_state` | string | See above. |
| `ticket` | string or null | The ticket key in the branch name. |
| `summary` | string or null | The JIRA summary, when available. |
| `kind` | string | See above. |
| `ticket_state` | object | See above. |

`gw` does not list the bare repository itself.

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
| `worktrees` | number | Linked worktrees, not including harness worktrees. |
| `dirty` | number or null | `null` unless `--dirty`. |
| `last_commit_age_secs` | number or null | The newest commit across the worktrees. |
| `jira_projects` | array | Declared in the `.gw.toml` of that layout. |

## `gw add --json`

```json
{
  "branch": "PROJ-123",
  "path": "/repo/worktrees/PROJ-123",
  "created": true
}
```

| Field | Type | Notes |
| --- | --- | --- |
| `branch` | string | The branch that now has a worktree. |
| `path` | string | Absolute path of the worktree. |
| `created` | boolean | `false` when `gw` reused an existing worktree. |

`--print-path` overrides `--json`. It prints the bare path and nothing else,
because the shell wrapper uses that contract. `--cd` does the same, because the
shell function that changes the directory must receive the path and nothing
else.

## `gw init --json`

```json
{
  "remote": "https://github.com/acme/widgets.git",
  "root": "/home/me/repos/acme/widgets",
  "bare": "/home/me/repos/acme/widgets/.bare",
  "worktrees": "/home/me/repos/acme/widgets/worktrees",
  "branch": "main",
  "checkout": "/home/me/repos/acme/widgets/worktrees/main",
  "created": true
}
```

| Field | Type | Notes |
| --- | --- | --- |
| `remote` | string | The remote that `gw` cloned. |
| `root` | string | Absolute layout root. |
| `bare` | string | Absolute path of the bare repository. |
| `worktrees` | string | Absolute path of the worktrees directory. |
| `branch` | string or null | First checkout. `null` on a dry run. |
| `checkout` | string or null | Absolute path of the first checkout. `null` on a dry run. |
| `created` | boolean | `false` on a dry run. `gw` wrote nothing. |

## `gw clean --json`

```json
{
  "applied": false,
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

`applied` is `false` when the document is a plan and `true` when the plan ran.
A script uses `applied` to tell a dry run from a destructive run. Do not infer
this value from flags or from the exit code.

`target.kind` is `worktree` or `branch`.

`action`:

| Value | Meaning | Destructive |
| --- | --- | --- |
| `remove` | Remove a checked-out worktree | yes |
| `delete` | Delete a branch ref. Has `tip`. | yes |
| `prune` | Drop the metadata for a missing worktree | no |
| `keep` | Nothing to do | no |
| `skip` | Not eligible to manage | no |
| `recommend` | Removes under a different flag | no |

`reason`:

| Value | Meaning |
| --- | --- |
| `protected` | On the protection list |
| `harness_worktree` | The agent harness owns it |
| `detached_head` | No branch is checked out |
| `prunable` | Git reports the directory as prunable |
| `has_unpublished_commits` | Some commits are on no remote |
| `has_uncommitted_changes` | The worktree is dirty |
| `status_unknown` | `gw` could not read the state |
| `recently_active` | Newer than the stale limit |
| `stale` | Older than the stale limit |
| `merged_pr` | GitHub reports a merged PR |
| `ticket_closed` | The JIRA ticket is closed |
| `no_merged_pr` | `gw` found no reason to remove |
| `no_merged_pr_ticket_closed` | No merged PR, but the ticket is closed |

`stale`, `recently_active`, and `ticket_closed` have extra fields (`age_secs`
or `resolution`) with `reason`.
