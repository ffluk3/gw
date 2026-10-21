# The decision model

`gw clean` does not decide and act in one pass. It separates the decision from
the action. That separation is what makes the destructive command testable and
reviewable.

## Four stages

1. **Snapshot.** The adapter layer gathers facts: worktrees, cleanliness,
   activity, reachable commits, protected branches, ticket state, merged-PR
   state. This is the only stage that does I/O.
2. **Verdict.** A pure function finds the first applicable fact about each
   worktree, in precedence order.
3. **Decision.** A pure function maps each verdict to a decision, using the
   policy for the one case that depends on a flag (closed tickets).
4. **Plan.** The decisions are collected into a plan. `--dry-run` prints it,
   and the executor applies it.

The executor never re-derives a decision. What you see in the dry run is
exactly what runs.

## The verdict is the policy

The verdict enum is exhaustive over the facts `gw` recognizes. Matching on it
has no default arm, so a new fact cannot compile until someone writes down its
consequence. The shell implementation expressed the same rule as an ordered
`if`/`elif` ladder spread across two passes, where the ordering was implicit.

Precedence matters as much as the cases. Every protecting fact is tested
before every destroying fact. [The safety rules](clean-safety.md) list the
order.

## Decisions carry their evidence

A decision is more than a verb. It carries the structured reason, and a delete
carries the tip commit. That gives three properties:

- the same reason renders as a table cell, a JSON field, or a CI log line
- a branch deletion cannot be planned without the data to restore it
- a decision can be asserted against exactly in a test

`Prune` is separate from `Remove` because pruning metadata for a worktree whose
directory is already gone destroys no checkout and no ref. It therefore is not
gated by the same confirmations.

`Recommend` is separate from `Delete` because it describes work worth doing
that the current policy does not enable. Nothing is destroyed, so it needs no
restore payload.

## Why `Unknown` is a value, not a default

Every fact that could not be observed is explicit. A missing cleanliness probe
is `unknown`, not `clean`. Missing activity is `unknown`, not zero. This is
what lets the classifier fail closed without special cases: the protecting
verdicts are ordinary enum members, tested in the ordinary order.

## The stable output contract

The plan has a JSON form that is the contract tests assert against. Column
widths, emoji, and colors never appear in it. Scripts target that shape; humans
target the table.
