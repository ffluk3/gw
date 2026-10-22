# Use a convention recipe

A recipe is a convention you put in `~/.gw.toml` so it applies to every
layout on the machine. Each layout can then state only what differs for it.

Check the result with `gw convention show` in any layout, and one branch's
mapping with `gw convention explain <branch>`.

The examples below are starting points. Change the provider names, patterns,
and commands to match your tools.

## JIRA and GitHub, one to one

The default. Writing it out is useful when you want to add a project list
globally. See [why the default is one branch to one
ticket](../developer/conventions.md#why-the-default-is-one-branch-to-one-ticket)
before you replace it:

```toml
# ~/.gw.toml
[convention.providers.jira]
base_url = "https://example.atlassian.net"

[[convention.ticket_rules]]
provider = "jira"
selector = "embedded"
pattern = '[A-Z][A-Z0-9]+-[0-9]+'
allow_projects = ["PROJ", "OPS"]
```

`allow_projects` narrows which branch prefixes count. A branch named
`feature/PROJ-12-rate-limit` resolves to `PROJ-12`, and a pull request whose
head branch is that same branch name counts as its change.

## Only the branch name matters

No tracker, and a change is identified by the branch name alone:

```toml
# ~/.gw.toml
[[convention.ticket_rules]]
provider = "none"

[[convention.change_rules]]
provider = "github"
selector = "head_branch"
```

An empty rule list is written as `ticket_rules = []`:

```toml
[convention]
ticket_rules = []
change_rules = [
  { provider = "github", selector = "head_branch" },
]
```

With no ticket rule, `gw status` shows no ticket and no summary. Cleanup by
merged change and by staleness still works. Cleanup by closed ticket does not,
because there is no ticket to be closed.

## GitLab

GitLab is not built in. Point a change rule at a command provider that asks
the GitLab API. The provider does not need to understand the convention; it
only answers head-branch queries.

```toml
# ~/.gw.toml
[convention.providers.gitlab]
kind = "command"
command = "my-gitlab-provider"

[[convention.change_rules]]
provider = "gitlab"
selector = "head_branch"
```

`my-gitlab-provider` reads a protocol-1 request on stdin and prints a
protocol-1 response on stdout. See [the provider reference](../reference/providers.md#kind-command).

## Linear

Linear identifiers look different, so the pattern changes and the provider
answers to the captured identifier.

```toml
# ~/.gw.toml
[convention.providers.linear]
kind = "command"
command = "my-linear-provider"

[[convention.ticket_rules]]
provider = "linear"
selector = "embedded"
pattern = 'eng-(?P<id>[0-9]+)'
```

The named group `id` is the identifier. `feature/eng-42-fix` resolves to `42`.

## Two identifiers on one branch

Rules are tried in order and the first match supplies the primary ticket.
A branch can carry a JIRA key and a Linear identifier:

```toml
[[convention.ticket_rules]]
provider = "jira"
selector = "embedded"
pattern = '[A-Z][A-Z0-9]+-[0-9]+'

[[convention.ticket_rules]]
provider = "linear"
selector = "embedded"
pattern = 'eng-(?P<id>[0-9]+)'
```

`PROJ-7-eng-42` resolves to `PROJ-7` because that rule is first. Swap the
rules to prefer Linear.

## Weak matches

A rule can declare that a match is only a mention. Mention evidence shows in
status but never removes a worktree:

```toml
[[convention.change_rules]]
provider = "github"
selector = "mention_ticket"
strength = "mention"
```

Use `mention` when the association is a hint rather than a fact. Use
`identity` only when the match proves the branch is that ticket or that
change.

## Per-layout overrides

Put the layout file beside `.bare`:

```toml
# <root>/.gw.toml
[[convention.ticket_rules]]
provider = "jira"
selector = "embedded"
pattern = '[A-Z][A-Z0-9]+-[0-9]+'
allow_projects = ["PROJ"]
```

A rule list in the layout file replaces the global list for that layout. An absent
key inherits the global value. `ticket_rules = []` clears an inherited rule
list.

## Disabling a lookup for one run

```bash
gw clean --skip-provider github
gw status --skip-provider jira,github
```

Vendors rate-limit, and a provider that is down should not slow down a run you
already know the answer to.
