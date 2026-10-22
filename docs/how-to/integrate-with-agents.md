# Use `gw` in an agent workflow

An agent that edits several tasks at the same time needs a separate checkout
for each task. `gw` is a good fit. It gives one worktree for each branch, it
needs no stashing, and its cleanup policy does not destroy work that it cannot
prove is safe.

The repository includes an
[Agent Skill](https://agentskills.io) at
[`skills/gw/SKILL.md`](https://github.com/ffluk3/gw/blob/main/skills/gw/SKILL.md).
Install the skill so the agent loads the workflow only when a task needs it.

## Install the skill

Select one scope. The parent directory must stay named `gw`, because the `name`
of a skill must match its directory.

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

    If you have no checkout, download the file from the repository:

    ```bash
    mkdir -p ~/.claude/skills/gw
    curl -fsSL https://raw.githubusercontent.com/ffluk3/gw/main/skills/gw/SKILL.md \
      -o ~/.claude/skills/gw/SKILL.md
    ```

Commit the project-scoped copy if the team must share it. Pin the download to
a tag when the binary is pinned to a release.

## The skill instructions

The skill tells the agent to do these steps:

- Make sure that the directory is inside a layout with `gw status --json`. Do
  not treat a normal clone as a layout.
- Get a checkout with `gw add --print-path <branch>` and use the absolute path
  that it returns. Do not use `gwadd` or `--cd`. A `cd` in a child process does
  not survive, and a non-interactive tool call cannot use the shell function
  that performs the `cd`.
- Read state from `gw status --json` and `gw repos --json`, never from tables.
- Preview with `gw clean --dry-run --json`. Apply `--yes` only with human
  approval. A piped `gw clean` is a dry run by default, so an unattended agent
  cannot clean by accident.

## Manual equivalent

An agent without the skill must still run these three commands:

```bash
gw status --json
worktree="$(gw add --print-path PROJ-1234)"
gw clean --dry-run --json
```

See [Parse `gw` output in a script](script-with-json.md) for the stable JSON
contract. See [`gw clean` safety rules](../explanation/clean-safety.md) for
each condition that must be true before `gw` removes anything.
