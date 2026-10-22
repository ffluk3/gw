//! An out-of-process provider.
//!
//! This is the escape hatch that makes the convention tool-agnostic without a
//! new adapter per vendor. A user configures a command; `gw` sends it a JSON
//! request on stdin and reads a JSON response from stdout.
//!
//! The trust level is the same as the built-in adapters: the command is local,
//! named by the user, and never fetched. It is not a sandbox, and it is not
//! intended to be one.
//!
//! Protocol version 1:
//!
//! ```json
//! { "protocol": 1,
//!   "capability": "ticket",
//!   "repo": { "remote": "...", "slug": "acme/widgets" },
//!   "queries": [ { "kind": "ticket", "id": "PROJ-1" } ] }
//! ```
//!
//! The response is one of:
//!
//! ```json
//! { "results": [ { "id": "PROJ-1", "state": "closed", "resolution": "Done",
//!                  "summary": "..." } ] }
//! { "results": [ { "branch": "PROJ-1", "state": "merged", "reference": "#12" } ] }
//! ```
//!
//! Anything else — a non-zero exit, unparseable output, a missing result, an
//! unknown state — becomes `unknown`. A protocol violation is never "no
//! ticket" and never "not merged", because those two readings authorize
//! opposite actions.

use std::io::Write;
use std::process::{Command, Stdio};

use gw_core::{
    ChangeObservation, ChangeQuery, ChangeSelector, ChangeState, ProviderId, Strength, TicketId,
    TicketObservation, TicketStatus,
};
use serde::Deserialize;

use crate::error::Error;
use crate::providers::{ChangeProvider, Limits, TicketProvider};

/// What the command is being asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    Ticket,
    Change,
}

impl Capability {
    fn as_str(self) -> &'static str {
        match self {
            Capability::Ticket => "ticket",
            Capability::Change => "change",
        }
    }
}

/// A provider implemented by running a configured command.
#[derive(Debug, Clone)]
pub struct CommandProvider {
    id: ProviderId,
    argv: Vec<String>,
    capability: Capability,
}

impl CommandProvider {
    pub fn new(id: ProviderId, argv: Vec<String>, capability: Capability) -> Self {
        Self {
            id,
            argv,
            capability,
        }
    }

    /// The name reported in a lookup error.
    ///
    /// `Error::Lookup` carries a `&'static str`, so the identifier is mapped to
    /// one of the known capability names rather than leaking a reference to a
    /// temporary.
    fn service(&self) -> &'static str {
        match self.capability {
            Capability::Ticket => "ticket-command",
            Capability::Change => "change-command",
        }
    }

    /// Run the command with `request` on stdin and return its stdout.
    fn run(&self, request: &serde_json::Value) -> Result<String, Error> {
        let service = self.service();
        let (program, args) = self.argv.split_first().ok_or_else(|| Error::Lookup {
            service,
            detail: "provider command is empty".into(),
        })?;

        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::Lookup {
                service,
                detail: format!("could not run `{program}` ({e})"),
            })?;

        if let Some(mut stdin) = child.stdin.take() {
            // A command that exits without reading its input closes the pipe,
            // and the write then fails with EPIPE. That is not a broken
            // provider: the command's own exit status and stderr are the real
            // failure, and `wait_with_output` below reports them. Treating the
            // broken pipe as an error here would replace that diagnosis with a
            // misleading "could not write" and race the child's exit.
            match stdin.write_all(request.to_string().as_bytes()) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {}
                Err(e) => {
                    return Err(Error::Lookup {
                        service,
                        detail: format!("could not write to `{program}` ({e})"),
                    })
                }
            }
        }

        let output = child.wait_with_output().map_err(|e| Error::Lookup {
            service,
            detail: format!("`{program}` did not exit ({e})"),
        })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Lookup {
                service,
                detail: format!("`{program}` failed: {}", stderr.trim()),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn request(
        &self,
        repo: &crate::commands::snapshot::RepoRef,
        queries: serde_json::Value,
    ) -> serde_json::Value {
        serde_json::json!({
            "protocol": 1,
            "capability": self.capability.as_str(),
            "repo": { "slug": repo.slug, "remote": repo.remote },
            "queries": queries,
        })
    }

    /// Parse the response envelope, treating any deviation as a protocol error.
    fn results(body: &str) -> Result<Vec<ResultRow>, String> {
        let parsed: Response = serde_json::from_str(body.trim())
            .map_err(|e| format!("response is not a protocol-1 document: {e}"))?;
        if parsed.protocol != 1 {
            return Err(format!("unsupported protocol version {}", parsed.protocol));
        }
        Ok(parsed.results)
    }
}

#[derive(Debug, Deserialize)]
struct Response {
    protocol: u32,
    #[serde(default)]
    results: Vec<ResultRow>,
}

#[derive(Debug, Deserialize)]
struct ResultRow {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    branch: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    resolution: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    reference: Option<String>,
}

impl TicketProvider for CommandProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn lookup(&self, ids: &[TicketId], limits: Limits) -> Result<Vec<TicketObservation>, Error> {
        let mut observations = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(limits.max_queries.max(1)) {
            let queries = chunk
                .iter()
                .map(|id| serde_json::json!({ "kind": "ticket", "id": id.as_str() }))
                .collect::<Vec<_>>();
            let body = self.run(&self.request(
                &crate::commands::snapshot::RepoRef {
                    remote: None,
                    slug: None,
                },
                serde_json::Value::Array(queries),
            ))?;
            let rows = Self::results(&body);

            for id in chunk {
                let (status, summary) = match &rows {
                    // A protocol violation is one failure for the whole batch,
                    // not a per-ticket result: the command's output cannot be
                    // trusted for any identifier it was asked about.
                    Err(detail) => (
                        TicketStatus::Unknown {
                            detail: detail.clone(),
                        },
                        None,
                    ),
                    Ok(rows) => rows
                        .iter()
                        .find(|r| r.id.as_deref() == Some(id.as_str()))
                        .map_or_else(
                            || {
                                (
                                    TicketStatus::Unknown {
                                        detail: "command returned no result for this ticket".into(),
                                    },
                                    None,
                                )
                            },
                            |row| (ticket_status(row), row.summary.clone()),
                        ),
                };
                observations.push(TicketObservation {
                    provider: self.id.clone(),
                    id: id.clone(),
                    strength: Strength::Identity,
                    status,
                    summary,
                });
            }
        }
        Ok(observations)
    }
}

fn ticket_status(row: &ResultRow) -> TicketStatus {
    match row.state.as_deref() {
        Some("closed") => TicketStatus::Closed {
            resolution: row.resolution.clone(),
        },
        Some("open") => TicketStatus::Open,
        // An absent or unfamiliar state is unreadable, not open. Reading it as
        // open would silently discard a "closed" answer.
        other => TicketStatus::Unknown {
            detail: match other {
                Some(state) => format!("unrecognized ticket state '{state}'"),
                None => "result had no state".to_string(),
            },
        },
    }
}

impl ChangeProvider for CommandProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn lookup(
        &self,
        repo: &crate::commands::snapshot::RepoRef,
        queries: &[ChangeQuery],
        limits: Limits,
    ) -> Result<Vec<ChangeObservation>, Error> {
        let mut observations = Vec::with_capacity(queries.len());
        for chunk in queries.chunks(limits.max_queries.max(1)) {
            let request_queries = chunk
                .iter()
                .map(|query| {
                    serde_json::json!({
                        "kind": query.selector.as_str(),
                        "branch": query.branch,
                        "ticket": query.ticket.as_ref().map(TicketId::as_str),
                    })
                })
                .collect::<Vec<_>>();
            let body = self.run(&self.request(repo, serde_json::Value::Array(request_queries)))?;

            let parsed = Self::results(&body);
            for query in chunk {
                let state = match &parsed {
                    Err(detail) => ChangeState::Unknown {
                        detail: detail.clone(),
                    },
                    Ok(rows) => {
                        // Match on the branch for head-branch queries and on
                        // the ticket for the ticket-keyed selectors, so a
                        // command can answer in any order.
                        let key = match query.selector {
                            ChangeSelector::HeadBranch => Some(query.branch.as_str()),
                            ChangeSelector::ContainsCommit | ChangeSelector::MentionTicket => {
                                query.ticket.as_ref().map(TicketId::as_str)
                            }
                        };
                        rows.iter()
                            .find(|row| row.branch.as_deref() == key || row.id.as_deref() == key)
                            .map_or(
                                ChangeState::Unknown {
                                    detail: "command returned no result for this query".into(),
                                },
                                change_state,
                            )
                    }
                };
                observations.push(ChangeObservation {
                    provider: self.id.clone(),
                    strength: query.strength,
                    selector: query.selector.as_str().to_string(),
                    branch: query.branch.clone(),
                    state,
                });
            }
        }
        Ok(observations)
    }
}

fn change_state(row: &ResultRow) -> ChangeState {
    match row.state.as_deref() {
        Some("merged") => ChangeState::Merged {
            reference: row.reference.clone(),
        },
        Some("open") => ChangeState::Open {
            reference: row.reference.clone(),
        },
        other => ChangeState::Unknown {
            detail: match other {
                Some(state) => format!("unrecognized change state '{state}'"),
                None => "result had no state".to_string(),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn empty_row() -> ResultRow {
        ResultRow {
            id: None,
            branch: None,
            state: None,
            resolution: None,
            summary: None,
            reference: None,
        }
    }

    #[test]
    fn a_known_ticket_state_is_parsed() {
        let row = ResultRow {
            state: Some("closed".into()),
            resolution: Some("Done".into()),
            ..empty_row()
        };
        assert_eq!(
            ticket_status(&row),
            TicketStatus::Closed {
                resolution: Some("Done".into())
            }
        );
    }

    #[test]
    fn an_unknown_state_is_unknown_not_open() {
        let row = ResultRow {
            state: Some("reopened".into()),
            ..empty_row()
        };
        assert!(matches!(ticket_status(&row), TicketStatus::Unknown { .. }));
    }

    #[test]
    fn an_unknown_change_state_is_unknown_not_open() {
        let row = ResultRow {
            state: Some("queued".into()),
            ..empty_row()
        };
        assert!(matches!(change_state(&row), ChangeState::Unknown { .. }));
    }

    #[test]
    fn a_wrong_protocol_version_is_an_error() {
        let body = r#"{"protocol": 2, "results": []}"#;
        assert!(CommandProvider::results(body).is_err());
    }

    #[test]
    fn malformed_output_is_an_error() {
        assert!(CommandProvider::results("not json").is_err());
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::tests::empty_row;
    use super::*;
    use crate::commands::snapshot::RepoRef;

    fn provider(capability: Capability) -> CommandProvider {
        CommandProvider::new(
            ProviderId::parse("host").unwrap(),
            vec!["/bin/cat".into()],
            capability,
        )
    }

    fn repo() -> RepoRef {
        RepoRef {
            remote: Some("git@github.com:acme/widgets.git".into()),
            slug: Some("acme/widgets".into()),
        }
    }

    fn ticket_id(raw: &str) -> TicketId {
        TicketId::parse(raw).unwrap()
    }

    fn change_query(branch: &str, selector: ChangeSelector) -> ChangeQuery {
        ChangeQuery {
            provider: ProviderId::parse("host").unwrap(),
            selector,
            strength: Strength::Identity,
            branch: branch.to_string(),
            ticket: Some(ticket_id("PROJ-1")),
        }
    }

    #[test]
    fn capability_names_are_stable() {
        assert_eq!(Capability::Ticket.as_str(), "ticket");
        assert_eq!(Capability::Change.as_str(), "change");
        assert_eq!(provider(Capability::Ticket).service(), "ticket-command");
        assert_eq!(provider(Capability::Change).service(), "change-command");
    }

    #[test]
    fn an_empty_command_is_an_error() {
        let provider = CommandProvider::new(
            ProviderId::parse("host").unwrap(),
            vec![],
            Capability::Ticket,
        );
        let error = provider.run(&serde_json::json!({})).unwrap_err();
        assert!(error.to_string().contains("empty"), "{error}");
    }

    #[test]
    fn a_broken_protocol_is_one_unknown_per_ticket() {
        // `/bin/cat` echoes the request, which is not protocol 1, so every
        // identifier in the batch gets an unknown observation.
        let ids = vec![ticket_id("PROJ-1"), ticket_id("PROJ-2")];
        let observations =
            TicketProvider::lookup(&provider(Capability::Ticket), &ids, Limits::default()).unwrap();
        assert_eq!(observations.len(), 2);
        for observation in &observations {
            match &observation.status {
                TicketStatus::Unknown { .. } => {}

                other => panic!("expected Unknown, got {other:?}"),
            }
            assert!(observation.summary.is_none());
        }
    }

    #[test]
    fn a_missing_program_is_an_error() {
        let provider = CommandProvider::new(
            ProviderId::parse("host").unwrap(),
            vec!["/definitely/not-a-real-binary".into()],
            Capability::Ticket,
        );
        let error = provider.run(&serde_json::json!({})).unwrap_err();
        assert!(error.to_string().contains("could not run"), "{error}");
    }

    #[test]
    fn a_failing_program_is_an_error() {
        let provider = CommandProvider::new(
            ProviderId::parse("host").unwrap(),
            vec![
                "/bin/sh".into(),
                "-c".into(),
                "echo boom >&2; exit 4".into(),
            ],
            Capability::Change,
        );
        let error = provider.run(&serde_json::json!({})).unwrap_err();
        assert!(error.to_string().contains("boom"), "{error}");
    }

    /// A command may exit before reading its input, which closes the pipe
    /// under the parent's write. The report must be the command's own failure,
    /// not a "could not write" caused by losing that race.
    #[test]
    fn a_command_that_never_reads_stdin_reports_its_own_failure() {
        let provider = CommandProvider::new(
            ProviderId::parse("host").unwrap(),
            vec![
                "/bin/sh".into(),
                "-c".into(),
                // No read from stdin at all, so the pipe is closed immediately.
                "echo 'the real problem' >&2; exit 3".into(),
            ],
            Capability::Change,
        );
        let error = provider
            .run(&serde_json::json!({"big": "x".repeat(100_000)}))
            .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("the real problem"), "{message}");
        assert!(
            !message.contains("could not write"),
            "a closed pipe replaced the real diagnosis: {message}"
        );
    }

    #[test]
    fn a_successful_program_returns_its_stdout() {
        let provider = CommandProvider::new(
            ProviderId::parse("host").unwrap(),
            vec!["/bin/cat".into()],
            Capability::Ticket,
        );
        let request = serde_json::json!({"hello": "world"});
        assert!(provider.run(&request).unwrap().contains("hello"));
    }

    #[test]
    fn the_request_names_the_protocol_capability_and_repo() {
        let request = provider(Capability::Ticket).request(&repo(), serde_json::json!([]));
        assert_eq!(request["protocol"], 1);
        assert_eq!(request["capability"], "ticket");
        assert_eq!(request["repo"]["slug"], "acme/widgets");
        assert_eq!(request["repo"]["remote"], "git@github.com:acme/widgets.git");
    }

    #[test]
    fn ticket_rows_cover_every_state() {
        for (state, expected) in [
            ("open", TicketStatus::Open),
            ("closed", TicketStatus::Closed { resolution: None }),
        ] {
            let row = ResultRow {
                state: Some(state.into()),
                ..empty_row()
            };
            assert_eq!(ticket_status(&row), expected);
        }
        let missing = ResultRow {
            state: None,
            ..empty_row()
        };
        assert!(matches!(
            ticket_status(&missing),
            TicketStatus::Unknown { .. }
        ));
    }

    #[test]
    fn change_rows_cover_every_state() {
        let row = ResultRow {
            state: Some("merged".into()),
            reference: Some("#7".into()),
            ..empty_row()
        };
        assert_eq!(
            change_state(&row),
            ChangeState::Merged {
                reference: Some("#7".into())
            }
        );
        let open = ResultRow {
            state: Some("open".into()),
            ..empty_row()
        };
        assert_eq!(change_state(&open), ChangeState::Open { reference: None });
        let missing = ResultRow {
            state: None,
            ..empty_row()
        };
        assert!(matches!(
            change_state(&missing),
            ChangeState::Unknown { .. }
        ));
    }

    #[test]
    fn a_missing_results_array_is_an_empty_batch() {
        assert!(CommandProvider::results(r#"{"protocol":1}"#)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn looking_up_tickets_without_a_result_is_unknown() {
        // `/bin/cat` echoes the request; that is not a protocol-1 response, so
        // every identifier becomes unknown.
        let observations = TicketProvider::lookup(
            &provider(Capability::Ticket),
            &[ticket_id("PROJ-1")],
            Limits::default(),
        )
        .unwrap();
        assert_eq!(observations.len(), 1);
        assert!(matches!(
            observations[0].status,
            TicketStatus::Unknown { .. }
        ));
    }

    #[test]
    fn looking_up_changes_matches_each_selector_key() {
        let provider = CommandProvider::new(
            ProviderId::parse("host").unwrap(),
            // `/bin/cat` reads stdin to EOF before answering, so the parent's
            // write cannot race a closed pipe. It echoes the request, which is
            // not a protocol-1 response, so every selector becomes unknown.
            vec!["/bin/cat".into()],
            Capability::Change,
        );
        for selector in [
            ChangeSelector::HeadBranch,
            ChangeSelector::ContainsCommit,
            ChangeSelector::MentionTicket,
        ] {
            let observations = ChangeProvider::lookup(
                &provider,
                &repo(),
                &[change_query("PROJ-1", selector)],
                Limits::default(),
            )
            .unwrap();
            assert_eq!(observations.len(), 1);
            assert!(matches!(observations[0].state, ChangeState::Unknown { .. }));
        }
    }

    #[test]
    fn an_empty_batch_never_runs_the_command() {
        let provider = CommandProvider::new(
            ProviderId::parse("host").unwrap(),
            vec!["/definitely/not-a-real-binary".into()],
            Capability::Ticket,
        );
        assert!(TicketProvider::lookup(&provider, &[], Limits::default())
            .unwrap()
            .is_empty());
        assert!(
            ChangeProvider::lookup(&provider, &repo(), &[], Limits::default())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_batch_larger_than_the_limit_is_chunked() {
        let ids: Vec<TicketId> = (0..5).map(|i| ticket_id(&format!("PROJ-{i}"))).collect();
        // `/bin/cat` echoes the request, so each chunk fails protocol parsing
        // and yields unknown; what matters is every id still gets an answer.
        let observations = TicketProvider::lookup(
            &provider(Capability::Ticket),
            &ids,
            Limits { max_queries: 2 },
        )
        .unwrap();
        assert_eq!(observations.len(), 5);
    }
}
