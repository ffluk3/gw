use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::Serialize;

use crate::layout::Layout;
use crate::model::{BranchName, Sha, TicketKey};

/// The working-tree cleanliness of a checkout.
///
/// `Unknown` exists because `git status` can fail (index lock, permissions,
/// broken worktree) and an empty stdout from a failed command is
/// indistinguishable from "clean" if you model this as a `bool`. The
/// classifier treats `Unknown` as dirty so a failed probe never authorizes
/// deletion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Dirty {
    Clean,
    Dirty,
    Unknown,
}

impl Dirty {
    #[must_use]
    pub fn is_clean(self) -> bool {
        matches!(self, Dirty::Clean)
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Dirty::Clean => "clean",
            Dirty::Dirty => "dirty",
            Dirty::Unknown => "unknown",
        }
    }
}

/// How a worktree entry relates to the repo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeKind {
    /// The bare repository itself, listed by `git worktree list`.
    Main,
    /// A normal linked worktree.
    Linked,
    /// An ephemeral checkout owned by the agent harness.
    Harness,
    /// Marked `prunable` by git: its directory is gone or invalid.
    Prunable,
}

/// What a staleness measurement was taken from.
///
/// The shell implementation measured pass 1 by file mtime and pass 2 by commit
/// date, so the same tree could be stale on one path and fresh on the other.
/// Carrying the source in the type forces the rule to be explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivitySource {
    FileMtime,
    CommitDate,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Activity {
    pub age_secs: i64,
    pub source: ActivitySource,
}

impl Activity {
    #[must_use]
    pub fn unknown() -> Self {
        Self {
            age_secs: 0,
            source: ActivitySource::Unknown,
        }
    }

    #[must_use]
    pub fn is_older_than(&self, threshold_secs: i64) -> bool {
        self.source != ActivitySource::Unknown && self.age_secs >= threshold_secs
    }
}

/// JIRA state for the ticket a branch refers to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum TicketStatus {
    /// No ticket key in the branch name, or JIRA lookup disabled.
    NotChecked,
    /// Ticket is not in a done status category.
    Open,
    /// Ticket is done, with its resolution if one is set.
    Closed { resolution: Option<String> },
}

impl TicketStatus {
    #[must_use]
    pub fn closed_resolution(&self) -> Option<&str> {
        match self {
            TicketStatus::Closed { resolution } => {
                Some(resolution.as_deref().unwrap_or("Resolved"))
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorktreeSnapshot {
    pub path: PathBuf,
    /// `None` means detached HEAD.
    pub branch: Option<BranchName>,
    pub head: Option<Sha>,
    pub kind: WorktreeKind,
    pub dirty: Dirty,
    pub activity: Activity,
    pub ticket: TicketStatus,
    /// Whether GitHub reports a merged PR for this branch.
    pub merged_pr: bool,
}

impl WorktreeSnapshot {
    /// A worktree is removable only when the facts needed to justify removal
    /// were all observed successfully.
    #[must_use]
    pub fn status_observed(&self) -> bool {
        self.dirty != Dirty::Unknown
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BranchSnapshot {
    pub name: BranchName,
    /// The worktree currently checking out this branch, if any.
    pub worktree: Option<PathBuf>,
    pub tip: Option<Sha>,
    pub commit_age_secs: Option<i64>,
}

/// Every commit reachable from any remote ref.
///
/// Backed by a sorted vector so a million-commit monorepo costs ~20 MB instead
/// of ~48 MB for a `HashSet`, while lookups stay O(log n) rather than the
/// shell's per-branch `grep` over a temporary file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ReachableSet {
    sorted: Vec<Sha>,
}

impl ReachableSet {
    #[must_use]
    pub fn new(mut commits: Vec<Sha>) -> Self {
        commits.sort_unstable();
        commits.dedup();
        Self { sorted: commits }
    }

    #[must_use]
    pub fn contains(&self, sha: &Sha) -> bool {
        self.sorted.binary_search(sha).is_ok()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.sorted.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sorted.is_empty()
    }
}

/// All facts the classifier is allowed to consider. Built by the adapter layer
/// before any decision is made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepoSnapshot {
    pub layout: Layout,
    pub default_branch: Option<BranchName>,
    /// The branch checked out by the bare repo itself.
    pub head_branch: Option<BranchName>,
    /// Branches that must never be deleted, because a ref delete would strand
    /// a checkout: the default branch, HEAD, every branch checked out in a
    /// worktree, plus configured extras.
    pub protected: BTreeSet<BranchName>,
    pub reachable: ReachableSet,
    pub worktrees: Vec<WorktreeSnapshot>,
    pub orphans: Vec<BranchSnapshot>,
}

impl RepoSnapshot {
    /// Whether deleting this branch's ref is forbidden.
    #[must_use]
    pub fn is_protected(&self, branch: &BranchName) -> bool {
        self.protected.contains(branch)
    }

    /// Whether this branch's *worktree* must never be removed.
    ///
    /// Deliberately narrower than [`Self::is_protected`]: every branch checked
    /// out in a worktree is protected from ref deletion, but only the default
    /// branch and the bare repo's own HEAD are protected from worktree removal.
    /// Conflating the two makes every worktree permanently unremovable.
    #[must_use]
    pub fn is_worktree_protected(&self, branch: &BranchName) -> bool {
        self.default_branch.as_ref() == Some(branch) || self.head_branch.as_ref() == Some(branch)
    }

    #[must_use]
    pub fn ticket_for(&self, branch: &BranchName) -> Option<TicketKey> {
        branch.ticket_key()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(s: &str) -> Sha {
        Sha::parse(s).unwrap()
    }

    #[test]
    fn reachable_set_lookups() {
        let set = ReachableSet::new(vec![sha("bbbb"), sha("aaaa"), sha("bbbb")]);
        assert_eq!(set.len(), 2);
        assert!(set.contains(&sha("aaaa")));
        assert!(!set.contains(&sha("cccc")));
    }

    #[test]
    fn unknown_dirty_is_not_clean() {
        assert!(!Dirty::Unknown.is_clean());
        assert!(Dirty::Clean.is_clean());
    }

    #[test]
    fn unknown_activity_is_never_stale() {
        let a = Activity::unknown();
        assert!(!a.is_older_than(0));
    }

    #[test]
    fn activity_threshold_is_inclusive() {
        let a = Activity {
            age_secs: 7 * 86_400,
            source: ActivitySource::FileMtime,
        };
        assert!(a.is_older_than(7 * 86_400));
        assert!(!a.is_older_than(8 * 86_400));
    }
}
