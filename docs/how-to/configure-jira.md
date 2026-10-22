# Use `gw` with JIRA

Use this guide to add ticket summaries to `gw status` and ticket state to
`gw clean`.

## Set the base URL

Put the URL in `~/.gw.toml` to use it everywhere, or in `.gw.toml` at a layout
root to scope it to one repository:

```toml
[convention.providers.jira]
base_url = "https://acme.atlassian.net"
```

`GW_JIRA_BASE_URL` works too, and it wins when the file also sets a base URL.
A `[convention.providers.jira]` entry must exist in a file for the variable to
activate JIRA. The base URL must use `https://`; `gw` rejects plain HTTP.

## Set credentials

Credentials come from the environment only. `gw` never reads them from
`.gw.toml`.

```bash
export GW_JIRA_USER=me@acme.com
export GW_JIRA_API_TOKEN=...
```

The `GW_` prefix avoids a collision with other tools that read a bare
`JIRA_API_TOKEN`.

## Limit the ticket prefixes

Without configuration, each key shaped like `ABC-123` counts. To accept only
your project:

```toml
[[convention.ticket_rules]]
provider = "jira"
selector = "embedded"
pattern = '[A-Z][A-Z0-9]+-[0-9]+'
allow_projects = ["PROJ", "OPS"]
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
