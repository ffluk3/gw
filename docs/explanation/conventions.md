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
one pull request.

The `allow_projects` field on that rule narrows it to a set of project
prefixes. That is why a layout can say "only `PROJ` keys count" without
changing anything else.

## Why the default is one branch to one ticket

The 1:1:1 mapping is not the only convention `gw` can express; the recipes
below show how to replace it. It is the default because it is the shape that
makes a worktree directory predictable for two different consumers.

**An agent that fans out.** A coordinating agent reads a list of tickets,
provisions one worktree for each, and hands a worker agent the path. The
mapping has to be a total function: any ticket key produces exactly one
directory, the same one on every run, with no coordination between workers.
`gw add --print-path PROJ-1234` is that function. Two workers cannot collide,
because a second request for the same ticket reuses the worktree instead of
creating a second checkout.

**An engineer with several tickets open.** The directory name tells you which
ticket a checkout belongs to, so you change tasks by changing directory rather
than by stashing. The same property means the layout itself shows what you have
in flight, without a separate list.

### What the constraint buys at cleanup time

Keeping the mapping tight is what makes the size of the worktree directory
manageable, because it ties cleanup to the one signal that covers work that
never became a pull request.

After the checks that protect unpublished commits and uncommitted changes,
`gw clean` can remove a worktree for three reasons:

- **Merged change.** A pull request was merged. This misses branches that were
  abandoned before anyone opened a PR.
- **Stale.** The last commit is older than `--stale-days` (7 by default). This
  is a proxy for "abandoned", and it is wrong in both directions: it keeps dead
  work that was touched recently, and it removes live work that is waiting on
  review.
- **Closed ticket.** The ticket is resolved. This is the precise signal. The
  work is done or dropped, whether or not it ever reached a pull request.

Only the third one bounds the directory against something you actually track.
That is the argument for the constraint: a branch per ticket, and a ticket
state to measure it by.

Closed-ticket removal is off by default. `gw clean` reports the worktrees it
would remove with `--closed` and removes nothing until you pass the flag. See
[the order of the checks](clean-safety.md).

### Where the claim stops

The default is a recommendation, not an invariant.

- The convention is configuration. A layout can drop ticket rules entirely
  (`ticket_rules = []`), and the [recipes](../how-to/convention-recipes.md)
  show a tracker with no ticket concept. What is lost is the closed-ticket
  reason; merged-change and staleness cleanup still work.
- A closed ticket is the last reason to destroy anything. Unpublished commits,
  an uncommitted change, an unreadable status, and a contested provider answer
  all come first. Closing a ticket does not remove a worktree that still holds
  work.

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

## Portability in practice

A provider needs only to answer the questions it was asked. It does not know
that its answers are compared against another provider's, and it does not
decide whether a match matters. Two consequences follow:

- A team can add GitLab or Linear with configuration and a small local
  program, without a new adapter in `gw` and without changing the classifier.
- The decision rules are identical for every tool. Reviewing them once covers
  every combination of tracker and code host.

The built-in providers are the JIRA and GitHub adapters, because those are what
the original tool supported. They have no special status in the model: `jira`
and `github` are simply the names of two providers that ship in the binary.

## What the convention does not do

The convention does not run commands, read files, or call the network. It only
decides which questions to ask. Everything that can fail stays in the adapter
layer, where failures already have a name and a `warning:` prefix.
