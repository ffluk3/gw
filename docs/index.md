# gw

You're reviewing PROJ-101 when someone drops a bug in PROJ-102. You know the
drill: stash your review, switch branches, fix the bug, switch back, and hope
your stash still applies when you return.

`gw` gives every branch its own directory. You change tasks by changing folder.
Nothing is stashed, and nothing is lost.

```text
# Without gw
git stash
git switch PROJ-102
... fix the bug ...
git switch PROJ-101
git stash pop          # hope it still applies

# With gw
gw add PROJ-102
... fix the bug ...
cd ../PROJ-101         # still exactly as you left it
```

## The problem it solves

A clone gives you one working copy and one checked-out branch. A second ticket
means a second switch, and switches cost you: your edits have to go somewhere
(a stash, or a throwaway commit), your editor reloads every file, and your
running tests and dev server point at code you are no longer looking at. At the
end of the week you also have no quick answer to "what am I in the middle of?"

Git can already do better — but the setup is fiddly enough that most people
never adopt it. `gw` makes the better way the default and handles the
bookkeeping.

## What a layout is

`gw` manages a **layout**: one bare repository, plus one full checkout for each
branch.

```text
<root>/
  .bare/               the shared repository: history only, no files
  worktrees/<branch>/  one checkout per branch
```

A *bare repository* has no working copy of its own. It is just the shared
history that every checkout borrows from. Each `worktrees/<branch>/` directory
is a real checkout with its own `HEAD`, so `git status`, your editor, and your
builds all work there normally.

There is no "main" checkout that is special: every worktree behaves the same
way. And `gw` finds the layout by walking up from whatever directory you run it
in, so you never configure a repository path.

## Why not just use `git worktree`?

`gw` is a thin layer over git's own worktree feature. It does not replace
`git`, and it does not try to. Here is what it adds:

| You want to... | Plain git | `gw` |
| --- | --- | --- |
| Open a ticket you already opened | `git worktree add` fails: the branch is already checked out | `gw add` reuses the existing checkout |
| Avoid remembering paths | `git worktree add ../PROJ-1 -b PROJ-1 main` | `gw add PROJ-1` derives the directory from the branch name |
| Delete a branch that was pushed but never merged | `git branch -d` refuses; you fall back to `-D` and hope | Deletes only after proving every commit is on a remote |
| Know what to clean up | `git worktree list`, then judge each one yourself | `gw clean` prints a plan with a reason per row |
| See what is in flight | `git branch -vv`, `git stash list`, memory | `gw status` gives one table with age, dirtiness, and ticket |

Each row is a small convenience. Together they are the difference between
worktrees being a clever thing you never quite get around to using, and being
how you work every day.

## Cleanup that proves it is safe

Worktrees accumulate, and deleting one by habit eventually loses work. `gw
clean` is the only command that destroys anything, so it refuses to guess: a
worktree stays unless `gw` can *prove* the work is published and finished. If
`gw` cannot read a fact, it treats the worktree as unsafe and keeps it.

The full ordered list of checks is in [Is it safe to
delete?](explanation/clean-safety.md).

## Install

Install the latest release. You need `curl` and `tar`, not a Rust toolchain:

```bash
curl -fsSL https://raw.githubusercontent.com/ffluk3/gw/main/install.sh | bash
```

Then load the shell integration, which makes `gw add` actually change
directory:

```bash
echo 'source ~/.local/share/gw/gw.sh' >> ~/.zshrc
```

## Where to go next

| If you want to... | Read |
| --- | --- |
| learn by doing, from zero | [Get started](tutorials/getting-started.md) — about ten minutes |
| finish a specific task | [How-to guides](how-to/index.md) |
| look up a flag, variable, or output shape | [Reference](reference/index.md) |
| understand why `gw` is built this way | [User Manual](explanation/index.md) |
| change `gw` itself | [Developer Manual](developer/index.md) |
| put `gw` in an agent workflow | [The `gw` skill](how-to/integrate-with-agents.md) |
