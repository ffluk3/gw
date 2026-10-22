# Providers

A provider answers questions about tickets or changes. `gw` ships two, and a
`command` provider lets you connect any other tool without a new release.

A provider is *active* only when it can work. An inactive provider is not an
error: it reports no observations, changes no decision, and `gw convention
show` says why it is inactive.

## Built-in providers

### `jira` — tickets

Reads a base URL from `GW_JIRA_BASE_URL` when it is set and non-empty,
otherwise from `[convention.providers.jira] base_url` in any configuration
file. A `jira` provider entry must exist for the environment to activate the
provider. Credentials come from `GW_JIRA_USER` and `GW_JIRA_API_TOKEN` only.

| Setting | Meaning | Default |
| --- | --- | --- |
| `base_url` | JIRA base URL. Must be `https://`. | — |
| `done_status_category` | The status category that means closed. | `done` |

Uses one batched JQL request per 50 identifiers. A ticket the tracker does not
return, or a response with no status category, is `unknown`, never `open` or
`closed`.

### `github` — changes

Requires the `gh` CLI, authenticated with `gh auth login`, and an origin that
is a GitHub remote.

Implements `head_branch` only. It looks for a merged pull request whose head
branch is exactly the branch's name. The other selectors produce `unknown`
observations rather than a guess.

A non-GitHub origin makes this provider report an error, which becomes an
`unknown` observation. It does not change any decision.

## `kind = "command"`

Run a local program that speaks a small JSON protocol on stdin and stdout.

```toml
[convention.providers.linear]
kind = "command"
command = "gw-linear-provider"          # or a list: ["/path/to/tool", "--json"]
```

The program is local, named by you, and never fetched. It runs with your
permissions; it is not sandboxed.

### Request

`gw` writes one JSON document to the program's stdin:

```json
{
  "protocol": 1,
  "capability": "ticket",
  "repo": { "slug": "acme/widgets", "remote": "git@github.com:acme/widgets.git" },
  "queries": [
    { "kind": "ticket", "id": "PROJ-1" }
  ]
}
```

For changes, `capability` is `change` and each query is:

```json
{ "kind": "head_branch", "branch": "PROJ-1", "ticket": "PROJ-1" }
```

`kind` is the selector name. `ticket` is the branch's primary ticket, or
`null`.

### Response

The program writes one JSON document to stdout and exits 0.

Tickets:

```json
{
  "protocol": 1,
  "results": [
    { "id": "PROJ-1", "state": "closed", "resolution": "Done", "summary": "Add rate limiting" }
  ]
}
```

Changes:

```json
{
  "protocol": 1,
  "results": [
    { "branch": "PROJ-1", "state": "merged", "reference": "#12" }
  ]
}
```

| Field | Required | Meaning |
| --- | --- | --- |
| `protocol` | yes | Must be `1`. |
| `results` | no | One entry per query. A missing entry becomes `unknown`. |

Ticket `state`: `open`, `closed`. Anything else is `unknown`.

Change `state`: `merged`, `open`. Anything else is `unknown`.

Results are matched by `id` for tickets. For `head_branch` queries they are
matched by `branch`; for the ticket-keyed selectors, by `id`.

### Failure handling

These all produce `unknown` observations and never "not merged" or "no
ticket":

- a non-zero exit
- stdout that is not a protocol-1 document
- an unsupported protocol version
- a result with a missing or unfamiliar state
- no result for a query that was asked

`unknown` blocks the verdict it would have justified. A worktree is never
removed because a provider could not answer.

## Selecting a change

Three selectors can appear as a query `kind`. A provider chooses which it can
answer; the convention decides which are asked for.

| Selector | Query carries | What a host usually maps it to |
| --- | --- | --- |
| `head_branch` | `branch` | A change whose source branch is that name. |
| `contains_commit` | `branch`, `ticket` | A merged change containing one of the branch's commits. Use this when a host squashes and the head branch is gone. |
| `mention_ticket` | `branch`, `ticket` | A change whose title or body refers to the ticket. Weaker: pair it with `strength = "mention"` unless the host guarantees the association. |

A provider that cannot answer a selector it was asked for must return
`unknown`, or omit the result. It must not return `open` or `merged` as a
stand-in. "I cannot tell" and "not merged" lead to opposite decisions, and only
one of them is safe to guess wrong.

The request for every selector carries `branch`, so a provider can answer
positionally even when it ignores `ticket`.

## Writing a provider

A provider only needs to answer what it was asked. It does not decide what a
branch means, whether a match is strong enough to matter, or whether anything
should be removed. Those are the convention's job and the classifier's job, and
keeping them out of the provider is what makes the convention portable.

Print diagnostics to stderr. stdout is the protocol channel.

### Adding a built-in provider

An adapter in the binary implements `TicketProvider` or `ChangeProvider` and
registers itself in `Registry::build` when its configuration is complete. The
same rules apply: return one observation per query, and make an unanswerable
query `unknown`. Nothing in the classifier changes when you add one.
