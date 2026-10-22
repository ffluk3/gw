# Conventions and providers

`gw` does not assume that you use JIRA, that you use GitHub, or that a ticket,
a branch, and a pull request are the same thing. It separates two questions
that a worktree manager has to answer:

- **What is this branch about?** A *convention* answers this from the branch
  name alone.
- **Who can tell me more?** A *provider* answers this by talking to a system.

The separation exists because the two change for different reasons. A team
moves from JIRA to Linear, or from GitHub to GitLab, without changing what its
branch names mean. Another team keeps the same tools but names branches
differently. If either fact were compiled in, every change would be a release.

## The split

```text
branch name ──convention rules──▶ ticket ids ──provider──▶ ticket state
            └──────────────────▶ change query ─provider──▶ change state
```

The convention is pure. It is a total function from a branch name to a set of
lookups, and it lives in `gw-core` beside the rest of the decision logic, so it
is testable without a repository, a network, or a vendor.

A provider is an adapter. It knows how to speak to one system and nothing
about why it was asked.

## The built-in convention

With no configuration, `gw` uses this convention:

| Capability | Rule |
| --- | --- |
| ticket | A JIRA-shaped key (`[A-Z][A-Z0-9]+-[0-9]+`) embedded in the branch name |
| change | A pull request whose head branch has the same name as the branch |

This is the 1:1:1 mapping earlier versions compiled in: one branch, one ticket,
one pull request. It stays the default because it is a sane starting point, not
because it is required.

The `[jira] project` setting narrows the first rule to a set of project
prefixes. That is why a layout can say "only `PROJ` keys count" without
changing anything else.

## Evidence strength

Each rule declares how strongly a match proves ownership of a branch.

- **identity** — the branch *is* the ticket, or the change *is* the branch.
- **mention** — the branch or change merely refers to the ticket.

Only identity evidence may justify destruction. A worktree that mentions
`PROJ-1` in passing is not the worktree for `PROJ-1`, and removing it would be
the exact mistake the [safety rules](clean-safety.md) exist to prevent. Mention
evidence may inform display and, at most, a recommendation.

## Contested facts are not evidence

A branch can produce more than one strong observation. If two providers
disagree — one says the change is merged, another cannot read it — the
disagreement is not a reason to destroy anything. `gw` treats a signal as
usable only when every identity-strength observation for it is known. Anything
else falls through to the weaker reasons, which still protect unpublished
commits and dirty trees.

This is the same principle as `unknown` cleanliness, applied one level up.

## Providers

A provider is active only when it can work. A tracker with no base URL or no
credentials is inactive, not failing: nothing is reported, and no decision
changes. `gw` records *why* a provider is inactive so a surprising "no PR
found" can be traced to `gh` not being installed.

Provider failures never become facts. A failed lookup produces an `unknown`
observation, which cannot justify removal. The command continues and reports
what it could observe.

## What the convention does not do

The convention does not run commands, read files, or call the network. It only
decides which questions to ask. Everything that can fail stays in the adapter
layer, where failures already have a name and a `warning:` prefix.
