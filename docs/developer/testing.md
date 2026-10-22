# Testing and coverage

The tests exist to protect three things: the classifier's safety order, the JSON
contract, and the fail-closed rule that turns an unreadable answer into
"unknown". Coverage is how we notice when one of those stops being exercised.

## Run the suite

```bash
cargo test --all
```

Two kinds of test share that command.

- **Unit tests** live beside the module. In `gw-core` they are the only tests:
  the crate is pure, so a fact can be constructed directly.
- **End-to-end tests** live in `crates/gw/tests/cli.rs`. They build a real bare
  repository, real worktrees, and a separate origin, then run the real `gw`
  binary. There is no mock of git, because git's own worktree semantics are
  what the tool is built on.

The coverage gate runs the same suite. Do not add a separate set of tests for
it.

## The coverage gate

```bash
cargo llvm-cov --all --fail-under-lines 95
```

CI fails below 95% line coverage. The number comes from `cargo-llvm-cov`, which
instrumented the build. Two details keep it honest.

1. **The spawned binary is measured.** The end-to-end tests start `gw` as a
   child process. `cargo-llvm-cov` merges the profile that child writes, so the
   command layer counts even though the test process never calls it in-process.
2. **External commands are faked at the process boundary.** The GitHub adapter
   runs `gh`; the JIRA adapter runs `curl`. Those tests put a stand-in script
   early on `PATH`, and the real adapter builds the arguments and parses the
   response. A missing, failing, or malformed response must still produce
   `Unknown`. Those tests prove the fail-closed rule, so keep them even if a
   unit test could cover the same lines more cheaply.

## Fakes inside the crate

`GitBackend` is a trait so a fake stays small. When a test needs one, put it in
the `coverage_tests` module and make every method the code under test does not
call `unreachable!()`, not `unimplemented!()`. Both panic; only `unreachable!()`
says the method is outside the path, and it marks the intent for the next
person.

Do not list a trait method as covered just because a fake exists. A fake is not
the real adapter.

## Environment mutation

`cargo test` runs tests in parallel, and the process environment is shared. A
test that sets `PATH`, `TMPDIR`, `HOME`, `GW_REPOS_ROOT`, `GW_CONFIG`, or a JIRA
credential must hold a lock for as long as the value differs. See
`repos::tests::env_guard` for the pattern.

Prefer an absolute program path where the test does not need the override.
`/bin/cat` cannot be redirected by a concurrent test that cleared `PATH`.

## What to assert

State the rule, not the implementation. "A provider that violates the protocol
never reads as merged" is a test worth having; "the parser calls `from_str`" is
not.

For a new safety rule, add the property that says what must never happen.
`crates/gw-core/tests/props.rs` holds the generated-state invariants, phrased
positively so a counterexample names the fact that was ignored.

## Next

Read the [decision model](decision-model.md) to see which rules the tests are
protecting, or [Writing the docs](docs-style.md) before you edit this page.
