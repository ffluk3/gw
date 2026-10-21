# Use `gw` with JIRA

Use this guide to add ticket summaries to `gw status` and ticket state to
`gw clean`.

## Set the base URL

Put the URL in `.gw.toml` at the layout root, beside `.bare`:

```toml
[jira]
base_url = "https://acme.atlassian.net"
```

Or pass it for one command:

```bash
gw --jira-url https://acme.atlassian.net status
```

The base URL must use `https://`. Plain HTTP is rejected.

## Set credentials

Credentials come from the environment only. They are never read from
`.gw.toml`.

```bash
export GW_JIRA_USER=me@acme.com
export GW_JIRA_API_TOKEN=...
```

`JIRA_USER` and `JIRA_API_TOKEN` also work. The `GW_` names win when both are
set.

## Limit which branch prefixes are tickets

Without configuration, any key shaped like `ABC-123` counts. To accept only
your project:

```toml
[jira]
project = ["PROJ", "OPS"]
```

Matching is case-insensitive. A branch such as `feature/PROJ-123-thing`
matches. A key whose prefix is not in the list does not count.

## Confirm it works

```bash
gw status
```

The `SUMMARY` column fills in. A failed lookup prints a warning and leaves the
column empty. It does not fail the command.

## What `gw` asks JIRA for

One batched JQL request for all ticket keys in the layout, asking for
`status`, `resolution`, and `summary`. A ticket in the `done` status category
counts as closed. See the [configuration reference](../reference/configuration.md)
for the full key list.
