# Use `gw` with JIRA

Use this guide to add ticket summaries to `gw status` and ticket state to
`gw clean`.

## Set the base URL

Put the URL in `.gw.toml` at the layout root, beside `.bare`:

```toml
[jira]
base_url = "https://acme.atlassian.net"
```

Or pass the URL for one command:

```bash
gw --jira-url https://acme.atlassian.net status
```

The base URL must use `https://`. `gw` rejects plain HTTP.

## Set credentials

Credentials come from the environment only. `gw` never reads them from
`.gw.toml`.

```bash
export GW_JIRA_USER=me@acme.com
export GW_JIRA_API_TOKEN=...
```

`JIRA_USER` and `JIRA_API_TOKEN` also work. If you set both names, the `GW_`
names win.

## Limit the ticket prefixes

Without configuration, each key shaped like `ABC-123` counts. To accept only
your project:

```toml
[jira]
project = ["PROJ", "OPS"]
```

Matching ignores case. A branch such as `feature/PROJ-123-thing` matches. A key
with a prefix that is not in the list does not count.

## Make sure that JIRA works

```bash
gw status
```

The `SUMMARY` column fills in. A failed lookup prints a warning and leaves the
column empty. The lookup does not fail the command.

## The JIRA request

`gw` sends one batched JQL request for all ticket keys in the layout. The
request asks for `status`, `resolution`, and `summary`. A ticket in the `done`
status category counts as closed. See the
[configuration reference](../reference/configuration.md) for the full key list.
