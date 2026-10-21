# Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success. This includes a dry run with nothing to do. |
| 1 | An operation failed. |
| 2 | You used a command or a flag incorrectly. This also covers unusable configuration, such as an unknown `.gw.toml` key. |
| 3 | Some, but not all, destructive actions failed. |

## How the codes map to errors

| Error kind | Exit code |
| --- | --- |
| Usage error, including bad `.gw.toml` | 2 |
| Layout not found, or wrong layout | 1 |
| Failed `git` command | 1 |
| Failed JIRA or GitHub lookup | 1 |
| Any other error | 1 |

A partial result is possible only in `gw clean`, when at least one removal or
deletion succeeded and at least one failed.

## Dry runs

A dry run exits 0 even when the plan contains destructive entries. The plan is
the result; nothing was attempted.
