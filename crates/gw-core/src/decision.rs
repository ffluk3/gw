use std::path::PathBuf;

use serde::Serialize;

use crate::model::{BranchName, Sha};

/// What an action applies to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Target {
    Worktree {
        path: PathBuf,
        branch: Option<BranchName>,
    },
    Branch {
        name: BranchName,
    },
}

impl Target {
    #[must_use]
    pub fn branch_name(&self) -> Option<&BranchName> {
        match self {
            Target::Worktree { branch, .. } => branch.as_ref(),
            Target::Branch { name } => Some(name),
        }
    }

    #[must_use]
    pub fn path(&self) -> Option<&PathBuf> {
        match self {
            Target::Worktree { path, .. } => Some(path),
            Target::Branch { .. } => None,
        }
    }
}

/// Why a decision was made. Structured rather than preformatted so the same
/// fact renders as a table cell, a JSON field, or a CI log line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "reason")]
pub enum Reason {
    Protected,
    HarnessWorktree,
    DetachedHead,
    Prunable,
    HasUnpublishedCommits,
    HasUncommittedChanges,
    StatusUnknown,
    RecentlyActive { age_secs: i64 },
    Stale { age_secs: i64 },
    MergedPr,
    TicketClosed { resolution: String },
    NoMergedPr,
    NoMergedPrTicketClosed { resolution: String },
}

/// What should happen to a target.
///
/// `Delete` carries the tip commit as a required field: a branch deletion
/// cannot be constructed without the data needed to restore it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum Decision {
    /// Remove a checked-out worktree directory.
    Remove {
        reason: Reason,
    },
    /// Drop metadata for a worktree whose directory is already gone. This
    /// destroys no checkout and no ref, so it is not gated by the same
    /// confirmations as `Remove` or `Delete`.
    Prune {
        reason: Reason,
    },
    Delete {
        reason: Reason,
        tip: Sha,
    },
    Keep {
        reason: Reason,
    },
    Skip {
        reason: Reason,
    },
    /// Worth doing, but not enabled by the current policy (e.g. a closed
    /// ticket without `--closed`). Nothing is destroyed, so unlike `Delete`
    /// this does not need the restore payload.
    Recommend {
        reason: Reason,
    },
}

impl Decision {
    #[must_use]
    pub fn reason(&self) -> &Reason {
        match self {
            Decision::Remove { reason }
            | Decision::Prune { reason }
            | Decision::Keep { reason }
            | Decision::Skip { reason }
            | Decision::Delete { reason, .. }
            | Decision::Recommend { reason, .. } => reason,
        }
    }

    #[must_use]
    pub fn is_destructive(&self) -> bool {
        matches!(self, Decision::Remove { .. } | Decision::Delete { .. })
    }

    #[must_use]
    pub fn is_recommendation(&self) -> bool {
        matches!(self, Decision::Recommend { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Plan {
    pub entries: Vec<(Target, Decision)>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    /// Targets the plan wants to destroy.
    pub to_remove: usize,
    pub to_prune: usize,
    pub to_delete: usize,
    /// Targets that are safe but not enabled by policy.
    pub recommended: usize,
    pub kept: usize,
    pub skipped: usize,
}

impl Plan {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn push(&mut self, target: Target, decision: Decision) {
        self.entries.push((target, decision));
    }

    pub fn destructive(&self) -> impl Iterator<Item = &(Target, Decision)> {
        self.entries.iter().filter(|(_, d)| d.is_destructive())
    }

    #[must_use]
    pub fn counts(&self) -> Counts {
        let mut counts = Counts::default();
        for (_, d) in &self.entries {
            match d {
                Decision::Remove { .. } => counts.to_remove += 1,
                Decision::Prune { .. } => counts.to_prune += 1,
                Decision::Delete { .. } => counts.to_delete += 1,
                Decision::Recommend { .. } => counts.recommended += 1,
                Decision::Keep { .. } => counts.kept += 1,
                Decision::Skip { .. } => counts.skipped += 1,
            }
        }
        counts
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for Plan {
    fn default() -> Self {
        Self::new()
    }
}
