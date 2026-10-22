# Developer Manual

These pages are for people changing `gw`. They assume you are comfortable with
git internals and Rust. If you only want to *use* `gw`, the
[User Manual](../explanation/index.md) is the shorter path.

## Read in this order

1. **[Architecture notes](architecture.md)** — why the workspace is two crates,
   and what belongs in each.
2. **[The decision model](decision-model.md)** — snapshot, verdict, decision,
   plan. The split that makes the destructive command testable.
3. **[Repositories and discovery](layout.md)** — how `gw` decides which layout
   it is addressing.
4. **[How a branch finds its ticket](conventions.md)** — branch-name
   conventions, providers, and evidence strength.

Then, before you change behavior:

- **[Testing and coverage](testing.md)** — how the suite is built, and the 95%
  line-coverage gate CI enforces.
- **[Writing the docs](docs-style.md)** — the rules these pages are held to.

## The contract in one place

The thing to keep true is the [command-line reference](../reference/cli.md) and
the [JSON shape](../reference/json.md). Tables, column widths, emoji, and color
are not contracts; scripts target JSON.
