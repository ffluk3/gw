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
| `open` | Not in the done status category |
| `closed` | In the done status category |
| `unknown` | A lookup ran and its answer could not be trusted |

A closed ticket also has `resolution`, which can be `null`. An `unknown` ticket
also has `detail`.

`strength`:

| Value | Meaning |
| --- | --- |
| `identity` | The rule identifies the work. May justify removal. |
| `mention` | The rule only associates a topic. Never justifies removal. |

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
      "ticket_state": { "status": "closed", "resolution": "Done" },
      "tickets": [
        {
          "provider": "jira",
          "id": "PROJ-123",
          "strength": "identity",
          "status": { "status": "closed", "resolution": "Done" },
          "summary": "Add rate limiting"
        }
      ]
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
| `ticket_state` | object | The primary ticket's state. See above. |
| `tickets` | array | Every ticket observation for the branch, one per matching rule. Empty when no rule matched. |

`tickets` is additive: `ticket`, `summary`, and `ticket_state` keep their
previous meaning and describe the primary (identity-strength) observation.

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
      "ticket_providers": ["jira"],
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
| `ticket_providers` | array | Ticket providers named by that layout's own rules. |
| `jira_projects` | array | Project filters that layout's ticket rules declare. Empty for a provider that has no project concept. |

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
because the shell wrapper uses that contract. `--cd` does the same: the path is
the only line on stdout. Standalone, `--cd` may also print a warning on stderr
that it cannot change the caller's directory.

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
A script uses `applied` to tell a plan from a run. Do not infer this value from
flags or from the exit code. It means "the plan ran", not "something was
destroyed": a prune-only plan can be `applied: true` with `to_remove` and
`to_delete` both zero.

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

`stale`, `recently_active`, and `ticket_closed` have extra fields (`age_secs`
or `resolution`) with `reason`.

## `gw convention show --json`

```json
{
  "ticket_rules": [
    {
      "provider": "jira",
      "selector": "embedded",
      "strength": "identity",
      "detail": "pattern [A-Z][A-Z0-9]+-[0-9]+; projects: PROJ"
    }
  ],
  "ticket_rules_origin": "layout",
  "change_rules": [
    { "provider": "github", "selector": "head_branch", "strength": "identity" }
  ],
  "change_rules_origin": "built-in",
  "providers": [
    { "id": "github", "active": true, "reason": null },
    { "id": "jira", "active": false, "reason": "set a base URL in .gw.toml and GW_JIRA_USER / GW_JIRA_API_TOKEN" }
  ],
  "settings": {
    "stale_after_secs": 3024000,
    "stale_after_origin": "layout",
    "protect": ["release"],
    "protect_origin": "layout",
    "skip": [],
    "skip_origin": "built-in",
    "repos_root": null,
    "repos_root_origin": "built-in"
  },
  "sources": [
    "/home/me/.gw.toml",
    "/home/me/repos/acme/widgets/worktrees/main/.gw.toml",
    "/home/me/repos/acme/widgets/.gw.toml"
  ]
}
```

| Field | Type | Notes |
| --- | --- | --- |
| `ticket_rules` | array | In evaluation order. |
| `ticket_rules_origin` | string | `built-in`, `global`, `default-branch`, or `layout`. |
| `change_rules` | array | In evaluation order. |
| `change_rules_origin` | string | Same values as above. |
| `providers` | array | The built-in `jira` and `github` providers, whether each is active, and why not. Declared `kind = "command"` providers are not listed. |
| `settings` | object | The resolved `[clean]`, `[providers]`, and `[discovery]` values. |
| `sources` | array | Files read, lowest precedence first. |

### `settings`

The object is additive; adding it changed no existing field.

| Field | Type | Notes |
| --- | --- | --- |
| `stale_after_secs` | integer | The limit that will be used, after precedence. Never `null`. |
| `stale_after_origin` | string | Where it came from: `built-in`, `global`, `default-branch`, or `layout`. |
| `protect` | array | Extra protected branches from the files. |
| `protect_origin` | string | Where the list came from: `built-in`, `global`, `default-branch`, or `layout`. |
| `skip` | array | Providers the files ask to skip. This is the file layer; `providers[].active` for the built-in providers reflects a flag or environment override. |
| `skip_origin` | string | Where the file list came from. |
| `repos_root` | string or null | The configured repos root, when the global file sets one. |
| `repos_root_origin` | string | Where it came from. |

## `gw convention explain --json`

```json
{
  "branch": "feature/PROJ-77-thing",
  "tickets": [
    { "provider": "jira", "id": "PROJ-77", "strength": "identity", "selector": "embedded" }
  ],
  "changes": [
    {
      "provider": "github",
      "selector": "head_branch",
      "strength": "identity",
      "branch": "feature/PROJ-77-thing",
      "ticket": "PROJ-77"
    }
  ],
  "rules": [
    {
      "rule": "ticket_rules[0]",
      "matched": true,
      "strength": "identity",
      "provider": "jira",
      "provider_active": true
    }
  ]
}
```

`rules` lists every rule, including the ones that did not match, so a branch
that resolves to nothing can be traced.
