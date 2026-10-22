use crate::decision::{Decision, Reason};
use crate::model::Sha;
use crate::policy::Policy;
use crate::snapshot::{ActivitySource, Dirty, RepoSnapshot, WorktreeKind, WorktreeSnapshot};

/// The first applicable fact about a worktree, in precedence order.
///
/// This enum is the policy. The earlier shell implementation expressed the same
/// rule as an ordered `if`/`elif` ladder spread across two passes; here the
/// ordering is explicit and `decide` is exhaustive over it, so adding a new
/// case is a compile error until its consequence is written down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict<'a> {
    Harness,
    Prunable,
    Detached,
    Protected,
    Unpublished { tip: Sha },
    Uncommitted,
    StatusUnknown,
    Merged,
    Stale { age_secs: i64 },
    TicketClosed { resolution: &'a str },
    NoSignal,
}

/// Determine the first applicable verdict for a worktree.
///
/// Order is deliberate: anything that protects work (harness ownership,
/// protection list, unpublished commits, uncommitted changes, unreadable
/// state) is tested before any reason to destroy it.
#[must_use]
pub fn verdict<'a>(
    wt: &'a WorktreeSnapshot,
    snap: &RepoSnapshot,
    policy: &'a Policy,
) -> Verdict<'a> {
    if wt.kind == WorktreeKind::Harness {
        return Verdict::Harness;
    }
    let Some(branch) = &wt.branch else {
        return Verdict::Detached;
    };
    // Protection is decided before anything that could destroy: a branch that
    // must never be lost should not even reach a removal verdict.
    if snap.is_worktree_protected(branch) {
        return Verdict::Protected;
    }
    // A prunable worktree has no directory left to lose. Pruning its metadata
    // is not a removal, so it does not require the cleanliness or publish
    // checks below.
    if wt.kind == WorktreeKind::Prunable {
        return Verdict::Prunable;
    }
    // A worktree whose HEAD could not be read is not safe to remove.
    let Some(tip) = &wt.head else {
        return Verdict::StatusUnknown;
    };
    if !snap.reachable.contains(tip) {
        return Verdict::Unpublished { tip: tip.clone() };
    }
    match wt.dirty {
        Dirty::Dirty => return Verdict::Uncommitted,
        Dirty::Unknown => return Verdict::StatusUnknown,
        Dirty::Clean => {}
    }
    if wt.has_merged_change() {
        return Verdict::Merged;
    }
    if wt.activity.source != ActivitySource::Unknown
        && wt.activity.age_secs >= policy.stale_after_secs
    {
        return Verdict::Stale {
            age_secs: wt.activity.age_secs,
        };
    }
    if let Some(resolution) = wt.closed_ticket_resolution() {
        return Verdict::TicketClosed { resolution };
    }
    Verdict::NoSignal
}

#[must_use]
pub fn decide(verdict: Verdict<'_>, policy: &Policy) -> Decision {
    match verdict {
        Verdict::Harness => Decision::Skip {
            reason: Reason::HarnessWorktree,
        },
        Verdict::Prunable => Decision::Prune {
            reason: Reason::Prunable,
        },
        Verdict::Detached => Decision::Skip {
            reason: Reason::DetachedHead,
        },
        Verdict::Protected => Decision::Keep {
            reason: Reason::Protected,
        },
        Verdict::Unpublished { .. } => Decision::Keep {
            reason: Reason::HasUnpublishedCommits,
        },
        Verdict::Uncommitted => Decision::Keep {
            reason: Reason::HasUncommittedChanges,
        },
        Verdict::StatusUnknown => Decision::Keep {
            reason: Reason::StatusUnknown,
        },
        Verdict::Merged => Decision::Remove {
            reason: Reason::MergedPr,
        },
        Verdict::Stale { age_secs } => Decision::Remove {
            reason: Reason::Stale { age_secs },
        },
        Verdict::TicketClosed { resolution } => {
            let reason = Reason::TicketClosed {
                resolution: resolution.to_string(),
            };
            if policy.clean_closed_tickets {
                Decision::Remove { reason }
            } else {
                Decision::Recommend { reason }
            }
        }
        Verdict::NoSignal => Decision::Keep {
            reason: Reason::NoMergedPr,
        },
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::*;
    use crate::layout::Layout;
    use crate::snapshot::{
        Activity, Dirty, ReachableSet, RepoSnapshot, TicketObservation, TicketStatus,
        WorktreeSnapshot,
    };
    use crate::{ProviderId, TicketId};
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    fn sha(raw: &str) -> Sha {
        Sha::parse(raw).unwrap()
    }

    fn snapshot() -> RepoSnapshot {
        RepoSnapshot {
            layout: Layout::new(PathBuf::from("/repo")),
            default_branch: Some(crate::model::BranchName::parse("develop").unwrap()),
            head_branch: None,
            protected: BTreeSet::new(),
            reachable: ReachableSet::new(vec![sha("aaaa")]),
            worktrees: Vec::new(),
            orphans: Vec::new(),
        }
    }

    fn worktree(branch: Option<&str>, kind: WorktreeKind) -> WorktreeSnapshot {
        WorktreeSnapshot {
            path: PathBuf::from("/repo/worktrees/x"),
            branch: branch.map(|b| crate::model::BranchName::parse(b).unwrap()),
            head: Some(sha("aaaa")),
            kind,
            dirty: Dirty::Clean,
            activity: Activity::unknown(),
            tickets: Vec::new(),
            changes: Vec::new(),
        }
    }

    fn policy() -> Policy {
        Policy::default()
    }

    #[test]
    fn a_harness_worktree_is_skipped() {
        let wt = worktree(Some("PROJ-1"), WorktreeKind::Harness);
        assert_eq!(verdict(&wt, &snapshot(), &policy()), Verdict::Harness);
        assert!(matches!(
            decide(Verdict::Harness, &policy()),
            Decision::Skip {
                reason: Reason::HarnessWorktree
            }
        ));
    }

    #[test]
    fn a_detached_head_is_skipped() {
        let wt = worktree(None, WorktreeKind::Linked);
        assert_eq!(verdict(&wt, &snapshot(), &policy()), Verdict::Detached);
        assert!(matches!(
            decide(Verdict::Detached, &policy()),
            Decision::Skip {
                reason: Reason::DetachedHead
            }
        ));
    }

    #[test]
    fn a_protected_default_branch_worktree_is_kept() {
        let wt = worktree(Some("develop"), WorktreeKind::Linked);
        assert_eq!(verdict(&wt, &snapshot(), &policy()), Verdict::Protected);
        assert!(matches!(
            decide(Verdict::Protected, &policy()),
            Decision::Keep {
                reason: Reason::Protected
            }
        ));
    }

    #[test]
    fn a_prunable_worktree_is_pruned() {
        let wt = worktree(Some("PROJ-1"), WorktreeKind::Prunable);
        assert_eq!(verdict(&wt, &snapshot(), &policy()), Verdict::Prunable);
        assert!(matches!(
            decide(Verdict::Prunable, &policy()),
            Decision::Prune {
                reason: Reason::Prunable
            }
        ));
    }

    #[test]
    fn a_worktree_without_a_head_is_status_unknown() {
        let mut wt = worktree(Some("PROJ-1"), WorktreeKind::Linked);
        wt.head = None;
        assert_eq!(verdict(&wt, &snapshot(), &policy()), Verdict::StatusUnknown);
    }

    #[test]
    fn a_closed_ticket_is_removed_only_when_the_policy_allows_it() {
        let mut wt = worktree(Some("PROJ-1"), WorktreeKind::Linked);
        wt.tickets.push(TicketObservation {
            provider: ProviderId::parse("jira").unwrap(),
            id: TicketId::parse("PROJ-1").unwrap(),
            strength: crate::convention::Strength::Identity,
            status: TicketStatus::Closed {
                resolution: Some("Done".into()),
            },
            summary: None,
        });
        // The worktree is clean, published, has no merged change, and its
        // activity source is unknown, so the closed ticket is the only signal.
        assert!(matches!(
            verdict(&wt, &snapshot(), &policy()),
            Verdict::TicketClosed { .. }
        ));

        assert!(matches!(
            decide(Verdict::TicketClosed { resolution: "Done" }, &policy()),
            Decision::Recommend { .. }
        ));

        let permissive = Policy {
            clean_closed_tickets: true,
            ..Policy::default()
        };
        assert!(matches!(
            decide(Verdict::TicketClosed { resolution: "Done" }, &permissive),
            Decision::Remove { .. }
        ));
    }

    #[test]
    fn no_signal_is_kept() {
        let wt = worktree(Some("PROJ-1"), WorktreeKind::Linked);
        assert_eq!(verdict(&wt, &snapshot(), &policy()), Verdict::NoSignal);
        assert!(matches!(
            decide(Verdict::NoSignal, &policy()),
            Decision::Keep {
                reason: Reason::NoMergedPr
            }
        ));
    }
}
