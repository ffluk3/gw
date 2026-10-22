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

#[cfg(test)]
mod coverage_tests {
    use super::*;

    fn sha() -> Sha {
        Sha::parse("9df03e38ae0").unwrap()
    }

    fn branch_target() -> Target {
        Target::Branch {
            name: BranchName::parse("PROJ-1").unwrap(),
        }
    }

    fn worktree_target() -> Target {
        Target::Worktree {
            path: PathBuf::from("/repo/worktrees/PROJ-1"),
            branch: Some(BranchName::parse("PROJ-1").unwrap()),
        }
    }

    #[test]
    fn target_accessors_cover_both_shapes() {
        assert!(branch_target().path().is_none());
        assert_eq!(branch_target().branch_name().unwrap().as_str(), "PROJ-1");
        assert_eq!(
            worktree_target().path().unwrap(),
            &PathBuf::from("/repo/worktrees/PROJ-1")
        );
        let detached = Target::Worktree {
            path: PathBuf::from("/repo/worktrees/x"),
            branch: None,
        };
        assert!(detached.branch_name().is_none());
    }

    #[test]
    fn every_decision_reports_its_reason() {
        let decisions = [
            Decision::Remove {
                reason: Reason::MergedPr,
            },
            Decision::Prune {
                reason: Reason::Prunable,
            },
            Decision::Delete {
                reason: Reason::Stale { age_secs: 1 },
                tip: sha(),
            },
            Decision::Keep {
                reason: Reason::Protected,
            },
            Decision::Skip {
                reason: Reason::DetachedHead,
            },
            Decision::Recommend {
                reason: Reason::NoMergedPr,
            },
        ];
        for decision in decisions {
            let _ = decision.reason();
        }
    }

    #[test]
    fn only_remove_and_delete_are_destructive() {
        assert!(Decision::Remove {
            reason: Reason::MergedPr
        }
        .is_destructive());
        assert!(Decision::Delete {
            reason: Reason::Stale { age_secs: 1 },
            tip: sha()
        }
        .is_destructive());
        for decision in [
            Decision::Prune {
                reason: Reason::Prunable,
            },
            Decision::Keep {
                reason: Reason::Protected,
            },
            Decision::Skip {
                reason: Reason::DetachedHead,
            },
            Decision::Recommend {
                reason: Reason::NoMergedPr,
            },
        ] {
            assert!(!decision.is_destructive());
            assert_eq!(
                decision.is_recommendation(),
                matches!(decision, Decision::Recommend { .. })
            );
        }
    }

    #[test]
    fn counts_account_for_every_action() {
        let mut plan = Plan::new();
        assert!(plan.is_empty());
        plan.push(
            worktree_target(),
            Decision::Remove {
                reason: Reason::MergedPr,
            },
        );
        plan.push(
            worktree_target(),
            Decision::Prune {
                reason: Reason::Prunable,
            },
        );
        plan.push(
            branch_target(),
            Decision::Delete {
                reason: Reason::Stale { age_secs: 1 },
                tip: sha(),
            },
        );
        plan.push(
            branch_target(),
            Decision::Recommend {
                reason: Reason::TicketClosed {
                    resolution: "Fixed".into(),
                },
            },
        );
        plan.push(
            branch_target(),
            Decision::Keep {
                reason: Reason::Protected,
            },
        );
        plan.push(
            worktree_target(),
            Decision::Skip {
                reason: Reason::HarnessWorktree,
            },
        );

        let counts = plan.counts();
        assert_eq!(counts.to_remove, 1);
        assert_eq!(counts.to_prune, 1);
        assert_eq!(counts.to_delete, 1);
        assert_eq!(counts.recommended, 1);
        assert_eq!(counts.kept, 1);
        assert_eq!(counts.skipped, 1);
        assert_eq!(plan.destructive().count(), 2);
        assert!(!plan.is_empty());
    }

    #[test]
    fn a_default_plan_is_empty() {
        assert!(Plan::default().is_empty());
    }
}
