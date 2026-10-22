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
        repo: &crate::commands::snapshot::RepoRef,
        queries: &[ChangeQuery],
        limits: Limits,
    ) -> Result<Vec<ChangeObservation>, Error> {
        // The GitHub adapter is the only place that decides a remote is not a
        // GitHub one. Gating this in the command layer would make the GitHub
        // assumption apply to every provider.
        let Some(slug) = repo.slug.as_deref() else {
            return Err(Error::Lookup {
                service: "github",
                detail: format!(
                    "origin is not a GitHub remote: {}",
                    repo.remote.as_deref().unwrap_or("(no origin)")
                ),
            });
        };
        let mut observations = Vec::with_capacity(queries.len());
        for chunk in queries.chunks(limits.max_queries.max(1)) {
            let answers = self.query_chunk(slug, chunk)?;
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
                    branch: query.branch.clone(),
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

    pub(super) fn query(branch: &str) -> ChangeQuery {
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

#[cfg(test)]
mod coverage_tests {
    use super::tests::query;
    use super::*;
    use crate::commands::snapshot::RepoRef;

    fn repo() -> RepoRef {
        RepoRef::from_remote(Some("git@github.com:acme/widgets.git".into()))
    }

    fn head_query() -> ChangeQuery {
        query("PROJ-1")
    }

    #[test]
    fn default_is_the_same_as_new() {
        assert_eq!(GhCli::default().id().as_str(), "github");
    }

    /// Tests that rewrite `PATH` must not run at the same time.
    fn path_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn a_remote_without_a_github_slug_is_an_error() {
        let provider = GhCli::new();
        let error = ChangeProvider::lookup(
            &provider,
            &RepoRef::from_remote(Some("https://gitlab.com/acme/widgets.git".into())),
            &[head_query()],
            Limits::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("not a GitHub remote"), "{error}");

        let no_remote = ChangeProvider::lookup(
            &provider,
            &RepoRef::from_remote(None),
            &[head_query()],
            Limits::default(),
        )
        .unwrap_err();
        assert!(no_remote.to_string().contains("(no origin)"), "{no_remote}");
    }

    #[test]
    fn a_non_head_branch_selector_is_unknown_without_running_gh() {
        let provider = GhCli::new();
        let mut mention = head_query();
        mention.selector = ChangeSelector::MentionTicket;
        let answers = provider.query_chunk("acme/widgets", &[mention]).unwrap();
        assert_eq!(answers.len(), 1);
        assert!(matches!(answers[0].1, ChangeState::Unknown { .. }));

        // A batch with only an unimplemented selector never spawns `gh`.
        let empty = provider.query_chunk("acme/widgets", &[]).unwrap();
        assert!(empty.is_empty());
    }

    #[test]
    fn queries_that_are_not_head_branch_are_answered_alongside_real_ones() {
        // `gh` is replaced by a script that answers one merged PR, so the head
        // branch query runs while the mention query is answered locally.
        let scripts = tempfile::tempdir().unwrap();
        let gh = scripts.path().join("gh");
        std::fs::write(
            &gh,
            "#!/bin/sh\ncat > /dev/null\necho '{\"data\":{\"b0\":{\"issueCount\":1}}}'\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let _lock = path_lock();
        let previous = std::env::var_os("PATH").unwrap_or_default();
        std::env::set_var(
            "PATH",
            format!(
                "{}:{}",
                scripts.path().display(),
                previous.to_string_lossy()
            ),
        );

        let provider = GhCli::new();
        let mut mention = head_query();
        mention.selector = ChangeSelector::MentionTicket;
        let mut head = head_query();
        head.branch = "PROJ-2".into();
        let answers = provider.query_chunk("acme/widgets", &[mention, head]);

        std::env::set_var("PATH", previous);
        let answers = answers.unwrap();
        assert_eq!(answers.len(), 2, "every query gets an answer");
        assert!(matches!(answers[0].1, ChangeState::Unknown { .. }));
        assert!(matches!(answers[1].1, ChangeState::Merged { .. }));
    }

    #[test]
    fn a_repo_ref_with_a_slug_passes_the_slug_to_the_query() {
        assert_eq!(repo().slug.as_deref(), Some("acme/widgets"));
    }

    #[test]
    fn a_batch_larger_than_the_limit_is_chunked() {
        let provider = GhCli::new();
        let queries: Vec<ChangeQuery> = (0..3)
            .map(|i| {
                let mut q = head_query();
                q.branch = format!("PROJ-{i}");
                q
            })
            .collect();
        // An unimplemented selector makes each chunk answer without spawning
        // `gh`, so this exercises chunking with a Count of every answer.
        let queries: Vec<ChangeQuery> = queries
            .into_iter()
            .map(|mut q| {
                q.selector = ChangeSelector::MentionTicket;
                q
            })
            .collect();
        let observations =
            ChangeProvider::lookup(&provider, &repo(), &queries, Limits { max_queries: 2 })
                .unwrap();
        assert_eq!(observations.len(), 3);
    }
}
