# The decision model

`gw clean` does not decide and act in one pass. It separates the decision from
the action. This separation makes the destructive command testable and
reviewable.

## Four stages

1. **Snapshot.** The adapter layer collects the facts: worktrees, cleanliness,
   activity, reachable commits, protected branches, ticket observations, and
   change observations. This is the only stage that does I/O.
2. **Verdict.** A pure function finds the first applicable fact for each
   worktree, in precedence order.
3. **Decision.** A pure function maps each verdict to a decision. It uses the
   policy for the one case that depends on a flag (closed tickets).
4. **Plan.** The decisions are collected into a plan. `--dry-run` prints the
   plan, and the executor applies the plan.

The executor never derives a decision again. The dry run is exactly what runs.

## The verdict is the policy

The verdict enum is exhaustive over the facts that `gw` recognizes. A match on
the enum has no default arm, so a new fact cannot compile until someone writes
its consequence. The shell implementation expressed the same rule as an
ordered `if`/`elif` ladder in two passes, where the order was implicit.

Precedence is as important as the cases. `gw` tests each protecting fact before
each destroying fact. [The safety rules](clean-safety.md) give the order.

## Decisions carry their evidence

A decision is more than a verb. It carries the structured reason, and a delete
carries the tip commit. This gives three properties:

- the same reason becomes a table cell, a JSON field, or a CI log line
- `gw` cannot plan a branch deletion without the data to restore the branch
- a test can assert a decision exactly

`Prune` is separate from `Remove`. Pruning the metadata of a worktree whose
directory is already gone destroys no checkout and no ref. Thus the same
confirmations do not gate it.

`Recommend` is separate from `Delete`. It describes work that is worth doing
but that the current policy does not enable. It destroys nothing, so it needs
no restore data.

## Why `Unknown` is a value, not a default

Each fact that `gw` could not observe is explicit. A missing cleanliness probe
is `unknown`, not `clean`. Missing activity is `unknown`, not zero. A failed
ticket or change lookup is `unknown`, not "nothing found". Thus the classifier
can fail closed without special cases. The protecting verdicts are ordinary
enum members, tested in the ordinary order.

Ticket and change state arrive as *observations*: each one names the provider
that produced it and the strength of the association that asked for it. The
verdict reads a fold over the identity-strength observations, so the decision
logic never learns which vendor answered. See
[conventions and providers](conventions.md).

## The stable output contract

The plan has a JSON form that is the contract for the tests. Column widths,
emoji, and colors never appear in it. Scripts use that shape. Humans use the
table.
