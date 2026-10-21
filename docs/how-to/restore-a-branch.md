# Restore a deleted branch

Use this guide to undo a branch deletion from `gw clean`.

## Find the restore command

`gw clean` prints the tip commit of each branch it deletes:

```text
Deleted branches can be restored with:
   git branch PROJ-1 9df03e38ae0
```

Run that command from inside the bare repository:

```bash
git -C <root>/.bare branch PROJ-1 9df03e38ae0
```

## Recreate the worktree

```bash
gw add PROJ-1
```

## Find the commit again if you lost the output

The commit is in the reflog of `refs/heads`:

```bash
git -C <root>/.bare reflog --all
```

The branch deletion is not a remote deletion. If the branch was pushed, a fetch
also brings it back:

```bash
git -C <root>/.bare fetch origin PROJ-1:refs/heads/PROJ-1
```

## Why `gw` keeps the tip in its output

The classifier carries the tip commit as a required field of every delete
decision. A branch deletion cannot be planned without the data to restore it.
See [the decision model](../explanation/decision-model.md).
