# Configuration reference

Configuration resolves from five sources. Precedence is highest first:

| Rank | Source | How it is resolved |
| --- | --- | --- |
| 1 | Command-line flag | clap. `--stale-days`, `--skip-provider`, `--no-color`, `--json`, `--repo`. |
| 2 | Environment bound to a flag | `GW_STALE_DAYS`, `GW_SKIP_PROVIDERS`, `NO_COLOR`, `GW_REPO`. Also the unbound `GW_JIRA_BASE_URL` and `GW_PROTECT`. |
| 3 | `<root>/.gw.toml` | The layout file. Read whenever it exists. |
| 4 | `<root>/worktrees/<default-branch>/.gw.toml` | Read only when `refs/remotes/origin/HEAD` resolves and the file exists. |
| 5 | `~/.gw.toml`, or the file named by `GW_CONFIG` | The global file. |
| base | Built-in convention and defaults | `Convention::built_in()` and `Policy::default()`. |

`GW_CONFIG` and `GW_NO_GLOBAL_CONFIG` select a file. They are not layers:

- `GW_CONFIG`, when set, *is* the file in slot 5. It replaces `~/.gw.toml`; it
  does not outrank the layout or default-branch files.
- `GW_NO_GLOBAL_CONFIG`, when set and `GW_CONFIG` is unset, removes slot 5. It
  does not remove slots 3 or 4.
- If both are set, `GW_CONFIG` wins.

There is exactly one way to configure a provider or a rule: `[convention]`.
No shorthand table exists beside it, so two files can never disagree about
where a value comes from. Three further tables configure the behavior around
conventions: `[clean]`, `[providers]`, and `[discovery]`.

## The three files

| File | Scope | Purpose |
| --- | --- | --- |
| `~/.gw.toml` | Every layout on the machine | A recipe: the tools you use and how your branches name tickets. |
| `<root>/worktrees/<default-branch>/.gw.toml` | One checkout of the default branch | A specialization carried with the repository. |
| `<root>/.gw.toml` | One layout | A specialization: a project list, a different pattern, a different base URL. |

The default-branch file is the one place a checkout's configuration can be
committed. Put the layout file at the layout root, beside `.bare`.

`<default-branch>` comes only from `refs/remotes/origin/HEAD`, resolved
through git. If that ref is unset or unreadable, `gw` skips the layer
silently. It is never guessed from `HEAD` or a branch name.

**Trust assumption.** A `.gw.toml` committed in the default branch is
repository content, and `gw` trusts it. That file may declare everything,
including `[convention.providers.<id>] kind = "command"`, which runs a local
program with your permissions, and `base_url`, which decides where JIRA
credentials are sent. It may also shorten `[clean] stale_after`, which makes an
interactive `gw clean` more aggressive. Review a `.gw.toml` in a repository you
clone the same way you review any other code that runs on your machine.

Credentials never go in a file. They go in the environment.

## How the files merge

The files merge field by field, not file by file:

| Value | Rule |
| --- | --- |
| A scalar (`base_url`, `done_status_category`, `strength`, `stale_after`, `repos_root`) | The higher level wins. |
| A provider table | Merged field by field under the same provider name, across all three files. |
| `ticket_rules`, `change_rules`, `protect`, `skip` | The higher level **replaces** the list when it declares one, and inherits the lower level's list when it does not. `[]` explicitly clears the list. |

This is what makes a recipe useful: the global file declares the tools once,
and each layout states only what differs for it. The default-branch file sits
between them and follows the same rules.

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
| `strength` | string | `identity` or `mention`. See [evidence strength](../developer/conventions.md#evidence-strength). |

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
| `head_branch` | The change's head branch has the branch's exact name. | `github`, `command` |
| `contains_commit` | The host reports whether a commit is in a merged change. | `command` only |
| `mention_ticket` | A change whose text mentions the ticket. Requires a ticket. | `command` only |

The built-in `github` adapter implements `head_branch`. The other two are
part of the protocol for a `command` provider to answer: the domain supports
them, and a provider that maps them to a host's search gets them without a
change to `gw`. When a provider cannot answer a selector, it reports `unknown`,
never "no merged change" — those mean different things.

## `[clean]`

Controls what `gw clean` is willing to destroy.

```toml
[clean]
stale_after = "5w"                 # default "7d"
protect = ["release", "hotfix"]    # added to the built-in set
```

| Key | Type | Meaning |
| --- | --- | --- |
| `stale_after` | duration string | A worktree with no activity within this window is a staleness candidate. Default `7d`. |
| `protect` | list | Extra branch names that `gw` never deletes. Added to the built-in protected set; it cannot remove a built-in protection. |

A duration is a positive whole number followed by `s`, `m`, `h`, `d`, `w`, or
`mo`. Write `"3d"` for three days and `"5w"` for five weeks. Zero is rejected:
it would make every worktree stale, which is never the intent. The same effect
is available deliberately through `--stale-days 0`.

An unreadable duration is a usage error (exit 2) that names the file and the
accepted units. `gw` never falls back to the default silently, because doing so
would destroy a different set of worktrees than the file asked for.

## `[providers]`

```toml
[providers]
skip = ["jira"]
```

`skip` names providers that this layout does not query. It is the file spelling
of `--skip-provider` and `GW_SKIP_PROVIDERS`. A non-empty flag or environment
list **replaces** the file list. An absent or empty flag list inherits the file
list. The replacement is fail-closed: skipping suppresses queries, so
re-enabling a provider can only add observations, and an observation that
fails is `unknown`, which blocks destruction.

`gw convention show --json` reports the file list in `settings.skip` and the
effective list after replacement in `providers[].active`.

This table is not `[convention.providers]`. `[convention.providers.<id>]`
*declares* a provider; `[providers] skip` *leaves one out* of the run.

## `[discovery]`

```toml
[discovery]
repos_root = "~/work/repos"
```

`repos_root` is the directory that holds layouts, used by `gw repos`,
`gw init`, and `--repo <owner>/<name>`. It is the file spelling of
`GW_REPOS_ROOT`.

This key belongs in the **global** file only. `gw` searches the repos root to
find a layout, so it must read this value before it can find the layout file.
A `[discovery] repos_root` inside a layout's `.gw.toml`, or inside the
default-branch file, is a usage error (exit 2), not a setting that silently
does nothing.

## `gw convention show`

Prints the convention that is actually in force, where each part came from,
and whether each provider is active. `--json` gives the same information in
the stable shape.

## `gw convention explain <branch>`

Prints each rule, whether it matched, and what would be queried. Use it when a
branch resolves to the wrong ticket, or to nothing.

## Environment variables

Each variable is optional. A variable binds to a flag, so it has the same
precedence as that flag.

Every setting that a team can share has a file spelling. The `File key` column
names it. A value from a flag or a variable wins over the file.

| Variable | File key | Meaning | Default |
| --- | --- | --- | --- |
| `GW_STALE_DAYS` | `[clean] stale_after` | Stale limit. The variable is whole days; the file accepts a duration such as `"5w"`. | `7d` |
| `GW_PROTECT` | `[clean] protect` | Branch names that `gw` must never delete. The variable is comma-separated. It **adds** to the file list and never removes a name. An invalid non-empty name is a usage error (exit 2). | — |
| `GW_SKIP_PROVIDERS` | `[providers] skip` | Providers to leave out of the run. Comma-separated on the variable. A non-empty list replaces the file list. | — |
| `GW_REPOS_ROOT` | `[discovery] repos_root` | Directory that holds layouts. `gw` expands a leading `~/` in either form. | `~/repos` |
| `GW_JIRA_BASE_URL` | `[convention.providers.jira] base_url` | JIRA base URL. The environment wins when both are set. An empty value falls back to the file. A `jira` provider entry must still exist. | — |

These variables have no file spelling, by design:

| Variable | Why it cannot live in a file |
| --- | --- |
| `GW_JIRA_USER`, `GW_JIRA_API_TOKEN` | Credentials. They stay in the environment so they are never written to a file that might be shared or committed. |
| `GW_REPO` | A per-invocation locator. `gw` must resolve it before it can find the layout, and therefore before it can read the layout file. |
| `GW_CONFIG`, `GW_NO_GLOBAL_CONFIG` | They select which configuration file to read, so they cannot be stored inside one. |
| `GW_CD` | Read by the shell integration in `shell/gw.sh`, not by the `gw` binary. `0` makes `gw add` stay in the current directory; `1` is the default behavior and enters the worktree. |
| `NO_COLOR` | Presentation only. It removes decorative emoji from human output. It never changes a decision, a table marker, or the JSON shape. The flag is `--no-color`. |

`GW_REPOS_ROOT` and `[discovery] repos_root` are the same setting. The file
form works only in the global file, for the same reason `GW_REPO` cannot be in
a file: the repos root is part of how `gw` finds a layout.

## Branch protection

`gw clean` always protects:

- the default branch of the repository
- the branch that the `HEAD` of the bare repository checks out
- each branch that a worktree checks out

`[clean] protect` and `GW_PROTECT` add more branches. The three files replace
the list as a unit, the same as every other list-valued setting. `GW_PROTECT`
is the deliberate exception: it is a monotone safety set, so it adds to the
resolved file list and never removes a name. A wrapper that exports
`GW_PROTECT=hotfix` must not silently unprotect a `release` that the layout
declared. To drop a file's protection, edit that file. `gw` never deletes a
protected branch.
Worktree removal has narrower protection. Only the default branch and the bare
`HEAD` are protected from removal, because otherwise `gw` could never remove a
worktree.

## JIRA behavior

A provider runs only when it can work. For JIRA, that means a base URL (in
`[convention.providers.jira]` or `GW_JIRA_BASE_URL`) and both credentials. When they are present, `gw` sends one batched JQL request for
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
