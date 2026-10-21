# Use `gw` in an agent workflow

An agent that edits several tasks at once needs an isolated checkout for each
one. `gw` is a good fit: one worktree per branch, no stashing, and a cleanup
policy that refuses to destroy work it cannot prove is safe.

The repository ships an
[Agent Skill](https://agentskills.io) at
[`skills/gw/SKILL.md`](https://github.com/ffluk3/gw/blob/main/skills/gw/SKILL.md).
Install it so the agent loads the workflow only when a task needs it.

## Install the skill

Choose one scope. The parent directory must stay named `gw`, because a skill's
`name` must match its directory.

=== "User, Claude Code"

    ```bash
    mkdir -p ~/.claude/skills
    cp -R skills/gw ~/.claude/skills/gw
    ```

=== "User, cross-client"

    ```bash
    mkdir -p ~/.agents/skills
    cp -R skills/gw ~/.agents/skills/gw
    ```

=== "Project"

    ```bash
    mkdir -p .claude/skills
    cp -R skills/gw .claude/skills/gw
    ```

=== "Release install"

    With no checkout, download the file from the repository:

    ```bash
    mkdir -p ~/.claude/skills/gw
    curl -fsSL https://raw.githubusercontent.com/ffluk3/gw/main/skills/gw/SKILL.md \
      -o ~/.claude/skills/gw/SKILL.md
    ```

Commit the project-scoped copy if the whole team should share it. Pin the
download to a tag when the binary is pinned to a release.

## What the skill tells the agent

- Confirm the directory is inside a layout with `gw status --json`; do not
  treat a normal clone as one.
- Get a checkout with `gw add --print-path <branch>` and use the returned
  absolute path. Do not use `gwadd`, because a `cd` in a child process does not
  survive.
- Read state from `gw status --json` and `gw repos --json`, never from tables.
- Preview with `gw clean --dry-run --json`, and apply `--yes` only with human
  approval. A piped `gw clean` is a dry run by default, so an unattended agent
  cannot clean by accident.

## Manual equivalent

An agent without the skill should still follow these three commands:

```bash
gw status --json
worktree="$(gw add --print-path PROJ-1234)"
gw clean --dry-run --json
```

See [Parse `gw` output in a script](script-with-json.md) for the stable JSON
contract, and [`gw clean` safety rules](../explanation/clean-safety.md) for
every condition that must hold before anything is removed.
