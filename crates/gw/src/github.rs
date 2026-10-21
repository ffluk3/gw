use std::collections::HashSet;
use std::process::{Command, Stdio};

use gw_core::BranchName;

use crate::error::Error;

/// The maximum number of branch filters to put in one GraphQL document.
/// GitHub limits query node count per request; chunking keeps large worktree
/// directories from silently producing an oversized query.
const BATCH: usize = 50;

pub trait GithubBackend {
    /// Return the subset of branches that have at least one merged PR.
    fn merged_prs(&self, repo: &str, branches: &[BranchName]) -> Result<HashSet<String>, Error>;
}

/// Batched `gh api graphql` lookup.
///
/// Branch names travel as GraphQL variables rather than being interpolated into
/// the query text, so a branch name can never reshape the document.
pub struct GhCli;

impl GhCli {
    fn query_chunk(&self, repo: &str, chunk: &[BranchName]) -> Result<Vec<String>, Error> {
        // `search` takes a single query argument, so the repo filter travels
        // inside the search string; branch names stay typed variables.
        let mut args: Vec<String> = vec![
            "api".into(),
            "graphql".into(),
            "-f".into(),
            format!("query={}", build_query(chunk.len())),
        ];

        for (i, branch) in chunk.iter().enumerate() {
            let q = format!("repo:{repo} head:{} is:pr is:merged", branch.as_str());
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
                    "could not run `gh` ({}). Install the GitHub CLI and run `gh auth login`.",
                    e
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

        let mut merged = Vec::new();
        for (i, branch) in chunk.iter().enumerate() {
            let count = parsed
                .get("data")
                .and_then(|d| d.get(format!("b{i}")))
                .and_then(|v| v.get("issueCount"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            if count > 0 {
                merged.push(branch.as_str().to_string());
            }
        }
        Ok(merged)
    }
}

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

impl GithubBackend for GhCli {
    fn merged_prs(&self, repo: &str, branches: &[BranchName]) -> Result<HashSet<String>, Error> {
        let mut merged = HashSet::new();
        for chunk in branches.chunks(BATCH) {
            for name in self.query_chunk(repo, chunk)? {
                merged.insert(name);
            }
        }
        Ok(merged)
    }
}

/// A backend that never finds a merged PR, used by `--skip-github`.
pub struct NoGithub;

impl GithubBackend for NoGithub {
    fn merged_prs(&self, _repo: &str, _branches: &[BranchName]) -> Result<HashSet<String>, Error> {
        Ok(HashSet::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
