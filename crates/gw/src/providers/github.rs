use std::process::{Command, Stdio};

use gw_core::{ChangeObservation, ChangeQuery, ChangeSelector, ChangeState, ProviderId};

use crate::error::Error;
use crate::providers::{ChangeProvider, Limits};

/// A GitHub adapter over the `gh` CLI.
///
/// Only [`ChangeSelector::HeadBranch`] is implemented. The other selectors are
/// reserved in the domain but have no GitHub mapping yet, so a query that uses
/// one yields `Unknown` rather than a guess. That keeps the failure visible
/// instead of silently reporting "no merged PR".
pub struct GhCli {
    id: ProviderId,
}

impl GhCli {
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: ProviderId::parse("github").expect("literal is a valid provider id"),
        }
    }

    fn query_chunk(
        &self,
        repo: &str,
        chunk: &[ChangeQuery],
    ) -> Result<Vec<(String, ChangeState)>, Error> {
        let head_branches: Vec<&ChangeQuery> = chunk
            .iter()
            .filter(|q| q.selector == ChangeSelector::HeadBranch)
            .collect();

        let mut answered: Vec<(String, ChangeState)> = Vec::with_capacity(chunk.len());
        for query in chunk {
            if query.selector != ChangeSelector::HeadBranch {
                answered.push((
                    query.key(),
                    ChangeState::Unknown {
                        detail: format!(
                            "provider github does not implement the {} selector",
                            query.selector.as_str()
                        ),
                    },
                ));
            }
        }

        if head_branches.is_empty() {
            return Ok(answered);
        }

        // `search` takes a single query argument, so the repo filter travels
        // inside the search string; branch names stay typed variables.
        let mut args: Vec<String> = vec![
            "api".into(),
            "graphql".into(),
            "-f".into(),
            format!("query={}", build_query(head_branches.len())),
        ];
        for (i, query) in head_branches.iter().enumerate() {
            let q = format!("repo:{repo} head:{} is:pr is:merged", query.branch);
            args.push("-f".into());
            args.push(format!("b{i}={q}"));
        }

        let output = Command::new("gh")
            .args(&args)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| Error::Lookup {
                service: "github",
                detail: format!(
                    "could not run `gh` ({e}). Install the GitHub CLI and run `gh auth login`."
                ),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Lookup {
                service: "github",
                detail: format!("`gh api graphql` failed: {}", stderr.trim()),
            });
        }

        let body = String::from_utf8_lossy(&output.stdout);
        let parsed: serde_json::Value = serde_json::from_str(&body).map_err(|e| Error::Lookup {
            service: "github",
            detail: format!("unexpected response: {e}"),
        })?;

        for (i, query) in head_branches.iter().enumerate() {
            let count = parsed
                .get("data")
                .and_then(|d| d.get(format!("b{i}")))
                .and_then(|v| v.get("issueCount"))
                .and_then(serde_json::Value::as_u64);
            let state = match count {
                Some(count) if count > 0 => ChangeState::Merged { reference: None },
                Some(_) => ChangeState::Open { reference: None },
                // A response without the count for a query we sent is not an
                // answer. Reporting `Open` would quietly disable merge cleanup.
                None => ChangeState::Unknown {
                    detail: "response carried no result for this query".into(),
                },
            };
            answered.push((query.key(), state));
        }
        Ok(answered)
    }
}

impl Default for GhCli {
    fn default() -> Self {
        Self::new()
    }
}

impl ChangeProvider for GhCli {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn lookup(
        &self,
        repo: &str,
        queries: &[ChangeQuery],
        limits: Limits,
    ) -> Result<Vec<ChangeObservation>, Error> {
        let mut observations = Vec::with_capacity(queries.len());
        for chunk in queries.chunks(limits.max_queries.max(1)) {
            let answers = self.query_chunk(repo, chunk)?;
            for query in chunk {
                let state = answers.iter().find(|(key, _)| key == &query.key()).map_or(
                    ChangeState::Unknown {
                        detail: "provider returned no answer".into(),
                    },
                    |(_, state)| state.clone(),
                );
                observations.push(ChangeObservation {
                    provider: self.id.clone(),
                    strength: query.strength,
                    selector: query.selector.as_str().to_string(),
                    state,
                });
            }
        }
        Ok(observations)
    }
}

/// GitHub limits query node count per request; chunking keeps large worktree
/// directories from silently producing an oversized query.
fn build_query(n: usize) -> String {
    let mut query = String::from("query(");
    for i in 0..n {
        if i > 0 {
            query.push(',');
        }
        query.push_str(&format!("$b{i}:String!"));
    }
    query.push_str(") {");
    for i in 0..n {
        query.push_str(&format!(
            " b{i}: search(query:$b{i}, type: ISSUE, first: 1) {{ issueCount }}"
        ));
    }
    query.push('}');
    query
}

#[cfg(test)]
mod tests {
    use super::*;
    use gw_core::{ChangeRule, Convention, Strength};

    fn query(branch: &str) -> ChangeQuery {
        Convention {
            ticket_rules: vec![],
            change_rules: vec![ChangeRule {
                provider: ProviderId::parse("github").unwrap(),
                selector: ChangeSelector::HeadBranch,
                strength: Strength::Identity,
            }],
        }
        .resolve(branch)
        .changes
        .remove(0)
    }

    #[test]
    fn query_declares_one_variable_per_branch() {
        let q = build_query(3);
        assert!(q.contains("$b0:String!"));
        assert!(q.contains("$b2:String!"));
        assert!(q.contains("b2: search(query:$b2"));
        assert!(
            !q.contains("$repo:String!"),
            "repo travels inside the search string"
        );
    }

    #[test]
    fn an_unimplemented_selector_is_unknown_not_open() {
        let mut q = query("PROJ-1");
        q.selector = ChangeSelector::MentionTicket;
        // The provider reports an unimplemented selector without running `gh`.
        let provider = GhCli::new();
        let answers = provider
            .query_chunk("acme/widgets", std::slice::from_ref(&q))
            .unwrap();
        assert_eq!(answers.len(), 1);
        assert!(matches!(answers[0].1, ChangeState::Unknown { .. }));
    }

    #[test]
    fn query_keys_are_stable() {
        assert_eq!(query("PROJ-1").key(), "github:head_branch:PROJ-1");
    }
}
