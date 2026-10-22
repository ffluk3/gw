# Configuration reference

Configuration has three levels: command-line flags and their environment
bindings, a per-layout file, and a user-global file. Precedence is highest
first:

1. command-line flag (or the environment variable that binds to it)
2. `<root>/.gw.toml` — the layout file
3. `~/.gw.toml` — the user-global file, or the file named by `GW_CONFIG`
4. the built-in convention

## The two files

| File | Scope | Purpose |
| --- | --- | --- |
| `~/.gw.toml` | Every layout on the machine | A recipe: the tools you use and how your branches name tickets. |
| `<root>/.gw.toml` | One layout | A specialization: a project list, a different pattern, a different base URL. |

Both sit outside a checkout, so neither is committed with the work. Put the
layout file at the layout root, beside `.bare`.

Credentials never go in either file. They go in the environment.

## How the files merge

The files merge field by field, not file by file:

| Value | Rule |
| --- | --- |
| A scalar (`base_url`, `done_status_category`, `strength`) | The higher level wins. |
| A provider table | Merged field by field under the same provider name. |
| `ticket_rules`, `change_rules` | The higher level **replaces** the list when it declares one, and inherits the lower level's list when it does not. `[]` explicitly clears the list. |

This is what makes a recipe useful: the global file declares the tools once,
and each layout states only what differs for it.

## `[jira]`

The original table. It is translated into the convention, so it composes with
`[convention]` rather than competing with it.

```toml
[jira]
project = "PROJ"                                # a string...
# project = ["PROJ", "OPS"]                     # ...or a list
base_url = "https://acme.atlassian.net"
```

| Key | Type | Meaning |
| --- | --- | --- |
| `jira.project` | string or list of strings | Project prefixes that a branch can reference. Empty or absent means any `[A-Z][A-Z0-9]+-[0-9]+` key. |
| `jira.base_url` | string | JIRA base URL. Must be `https://`. |

`jira.project` narrows the ticket rules that do not already name their own
`allow_projects`. A `[convention.ticket_rules]` entry is more specific and
keeps its own list. `--jira-project` / `GW_JIRA_PROJECT` replaces the list
from both files.

## `[convention]`

Declares providers and the ordered rules that map a branch to a lookup.

```toml
[convention.providers.jira]
kind = "jira"
base_url = "https://acme.atlassian.net"
done_status_category = "done"                    # default

[[convention.ticket_rules]]
provider = "jira"
selector = "embedded"
pattern = '[A-Z][A-Z0-9]+-[0-9]+'
allow_projects = ["PROJ", "OPS"]                 # optional
strength = "identity"                            # default

[[convention.change_rules]]
provider = "github"
selector = "head_branch"
strength = "identity"                            # default
```

### `[convention.providers.<id>]`

| Key | Type | Meaning |
| --- | --- | --- |
| `kind` | string | Adapter to use. Defaults to the provider's own name. |
| `base_url` | string | Base URL, for providers that have one. |
| `done_status_category` | string | The status category that counts as closed. Defaults to `done`. |
| `command` | string or list | Executable and fixed arguments, for `kind = "command"`. |

### Ticket rules

| Key | Type | Meaning |
| --- | --- | --- |
| `provider` | string | Which provider answers. |
| `selector` | string | `embedded` is the only selector today. |
| `pattern` | string | A regular expression applied to the branch name. The named group `id`, or the first capture group, is the ticket identifier. Without a group, the whole match is. |
| `allow_projects` | list | Accept only these leading prefixes, compared case-insensitively. Empty means any. |
| `strength` | string | `identity` or `mention`. See [evidence strength](../explanation/conventions.md#evidence-strength). |

Rules are evaluated in order. The first rule that matches a branch supplies
its primary ticket. An uncompilable `pattern` stops the command with exit
code 2 rather than silently matching nothing.

### Change rules

| Key | Type | Meaning |
| --- | --- | --- |
| `provider` | string | Which provider answers. |
| `selector` | string | `head_branch`, `contains_commit`, or `mention_ticket`. |
| `strength` | string | `identity` or `mention`. |

| Selector | Meaning | Implemented by |
| --- | --- | --- |
| `head_branch` | The change's head branch has the branch's exact name. | `github` |
| `contains_commit` | The host reports whether a commit is in a merged change. | none yet |
| `mention_ticket` | A change whose text mentions the ticket. Requires a ticket. | none yet |

A selector that no provider implements produces an `unknown` observation. It
never produces "no merged change", because those mean different things.

## `gw convention show`

Prints the convention that is actually in force, where each part came from,
and whether each provider is active. `--json` gives the same information in
the stable shape.

## `gw convention explain <branch>`

Prints each rule, whether it matched, and what would be queried. Use it when a
branch resolves to the wrong ticket, or to nothing.

## Environment variables

Each variable is optional.

| Variable | Meaning | Default |
| --- | --- | --- |
| `GW_REPO` | Layout root, `.bare` path, `owner/name`, or unique name. Same as `--repo`. | discovery from the current directory |
| `GW_CONFIG` | Use this file as the global configuration instead of `~/.gw.toml`. | `~/.gw.toml` |
| `GW_NO_GLOBAL_CONFIG` | Any value suppresses the global file. | — |
| `GW_CD` | Set to `1` to make a bare `gw add` change the directory of the shell. The shell integration reads this variable, not the binary. | — |
| `GW_REPOS_ROOT` | Directory that holds layouts. `gw` expands a leading `~/`. | `~/repos` |
| `GW_STALE_DAYS` | Stale limit in days. | `7` |
| `GW_PROTECT` | Comma-separated branch names that `gw` must never delete. | — |
| `GW_JIRA_PROJECT` | Comma-separated project keys. Same as `--jira-project`. | — |
| `GW_JIRA_BASE_URL` | JIRA base URL. Same as `--jira-url`. | — |
| `GW_JIRA_USER` | JIRA user name. `gw` also reads `JIRA_USER`. | — |
| `GW_JIRA_API_TOKEN` | JIRA API token. `gw` also reads `JIRA_API_TOKEN`. | — |
| `GW_SKIP_PROVIDERS` | Comma-separated provider names. Same as `--skip-provider`. | — |
| `GW_SKIP_GITHUB` | Any value skips the code-host lookup. Same as `--skip-provider github`. | — |
| `NO_COLOR` | Any value disables color and emoji. Same as `--no-color`. | — |

## Branch protection

`gw clean` always protects:

- the default branch of the repository
- the branch that the `HEAD` of the bare repository checks out
- each branch that a worktree checks out

`GW_PROTECT` adds more branches. `gw` never deletes a protected branch.
Worktree removal has narrower protection. Only the default branch and the bare
`HEAD` are protected from removal, because otherwise `gw` could never remove a
worktree.

## JIRA behavior

A provider runs only when it can work. For JIRA, that means a base URL and
both credentials. When they are present, `gw` sends one batched JQL request for
the ticket keys that it found:

```text
key in (PROJ-1,PROJ-2,...)
```

It requests `status`, `resolution`, and `summary` for up to 50 tickets per
request. A ticket in the configured done category counts as closed. When no
resolution name is set, the report uses `Resolved`.

A ticket the tracker does not return is `unknown`, not `open`. A response
without a status category is `unknown`, not `closed`.

Credentials go to `curl` through a mode-0600 configuration file, so they do not
appear in the process list.
