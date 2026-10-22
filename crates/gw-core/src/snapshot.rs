use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::Serialize;

use crate::convention::{ProviderId, Strength};
use crate::layout::Layout;
use crate::model::{BranchName, Sha, TicketId};

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

/// Tracker state for a ticket.
///
/// `NotChecked` covers both "the branch has no ticket" and "lookups are
/// disabled". `Unknown` is distinct: a lookup ran and its answer could not be
/// trusted. The classifier treats the two the same way (neither closes a
/// ticket), but JSON distinguishes them so a caller can tell "not configured"
/// from "the tracker failed".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum TicketStatus {
    /// No ticket key in the branch name, or lookups are disabled.
    NotChecked,
    /// Ticket is not in a done status category.
    Open,
    /// Ticket is done, with its resolution if one is set.
    Closed { resolution: Option<String> },
    /// A lookup ran and did not produce a trustworthy answer.
    Unknown { detail: String },
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

    /// Whether this observation is trustworthy enough to inform a decision.
    #[must_use]
    pub fn is_known(&self) -> bool {
        !matches!(self, TicketStatus::Unknown { .. })
    }
}

/// One ticket observation, tagged with the provider that produced it and how
/// strongly the convention associated it with the branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TicketObservation {
    pub provider: ProviderId,
    pub id: TicketId,
    pub strength: Strength,
    pub status: TicketStatus,
    pub summary: Option<String>,
}

/// What a code host reports about a change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum ChangeState {
    /// A change was found and merged.
    Merged { reference: Option<String> },
    /// A change exists but is not merged.
    Open { reference: Option<String> },
    /// The provider failed or could not answer.
    Unknown { detail: String },
}

impl ChangeState {
    #[must_use]
    pub fn is_known(&self) -> bool {
        !matches!(self, ChangeState::Unknown { .. })
    }
}

/// One change observation, tied back to the query that produced it.
///
/// `branch` is part of the observation rather than only of the query because a
/// provider answers a batch positionally. Without it, attaching answers to
/// worktrees would key on provider and selector alone, and the second
/// worktree would receive the first worktree's answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangeObservation {
    pub provider: ProviderId,
    pub strength: Strength,
    pub selector: String,
    pub branch: String,
    pub state: ChangeState,
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
    /// Every ticket the convention associated with this branch.
    pub tickets: Vec<TicketObservation>,
    /// Every change lookup the convention asked for and its answer.
    pub changes: Vec<ChangeObservation>,
}

impl WorktreeSnapshot {
    /// A worktree is removable only when the facts needed to justify removal
    /// were all observed successfully.
    #[must_use]
    pub fn status_observed(&self) -> bool {
        self.dirty != Dirty::Unknown
    }

    /// The first identity-strength ticket observation.
    #[must_use]
    pub fn primary_ticket(&self) -> Option<&TicketObservation> {
        self.tickets.iter().find(|t| t.strength.is_identity())
    }

    /// The first identity-strength ticket observation whose state is known.
    #[must_use]
    pub fn primary_known_ticket(&self) -> Option<&TicketObservation> {
        self.tickets
            .iter()
            .find(|t| t.strength.is_identity() && t.status.is_known())
    }

    /// Whether a closed ticket may justify removal.
    ///
    /// Returns the resolution only when every identity-strength ticket
    /// observation is known *and* at least one is closed. A contested or
    /// unreadable answer yields `None`: ambiguity is not evidence, so the
    /// worktree falls through to weaker reasons rather than being removed on a
    /// fact the provider could not confirm.
    #[must_use]
    pub fn closed_ticket_resolution(&self) -> Option<&str> {
        let identity: Vec<&TicketObservation> = self
            .tickets
            .iter()
            .filter(|t| t.strength.is_identity())
            .collect();
        if identity.is_empty() || identity.iter().any(|t| !t.status.is_known()) {
            return None;
        }
        identity.iter().find_map(|t| t.status.closed_resolution())
    }

    /// Whether a merged change may justify removal.
    ///
    /// The same contested-fact rule as [`Self::closed_ticket_resolution`]:
    /// merge evidence counts only when every identity-strength change
    /// observation is known.
    #[must_use]
    pub fn has_merged_change(&self) -> bool {
        let identity: Vec<&ChangeObservation> = self
            .changes
            .iter()
            .filter(|c| c.strength.is_identity())
            .collect();
        !identity.is_empty()
            && identity.iter().all(|c| c.state.is_known())
            && identity
                .iter()
                .any(|c| matches!(c.state, ChangeState::Merged { .. }))
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

    /// The ticket a branch resolves to under the given convention.
    ///
    /// The convention is passed in rather than stored on the snapshot: the
    /// snapshot is a record of facts, and how a branch maps to a ticket is
    /// configuration, not an observation.
    #[must_use]
    pub fn ticket_for(
        &self,
        branch: &BranchName,
        convention: &crate::convention::Convention,
    ) -> Option<TicketId> {
        convention
            .resolve(branch.as_str())
            .primary_ticket()
            .map(|t| t.id.clone())
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
