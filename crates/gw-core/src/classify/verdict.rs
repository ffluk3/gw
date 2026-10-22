use crate::decision::{Decision, Reason};
use crate::model::Sha;
use crate::policy::Policy;
use crate::snapshot::{ActivitySource, Dirty, RepoSnapshot, WorktreeKind, WorktreeSnapshot};

/// The first applicable fact about a worktree, in precedence order.
///
/// This enum is the policy. The shell implementation expressed the same rule as
/// an ordered `if`/`elif` ladder spread across two passes of `gwclean.sh`; here
/// the ordering is explicit and `decide` is exhaustive over it, so adding a new
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
