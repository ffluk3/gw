use std::path::PathBuf;

use crate::decision::{Decision, Plan, Reason, Target};
use crate::model::BranchName;
use crate::policy::Policy;
use crate::snapshot::{RepoSnapshot, WorktreeKind};

use super::verdict::{decide, verdict};

/// Build the full clean plan without touching anything.
///
/// This is the only function that decides what gets destroyed. `gw clean
/// --dry-run` prints this plan and `gw clean` executes it; the executor never
/// re-derives a decision.
pub fn classify_clean(snap: &RepoSnapshot, policy: &Policy) -> Plan {
    let mut plan = Plan::new();

    for wt in &snap.worktrees {
        // The bare repo itself is not a candidate.
        if wt.kind == WorktreeKind::Main {
            continue;
        }
        let v = verdict(wt, snap, policy);
        let decision = decide(v, policy);
        plan.push(
            Target::Worktree {
                path: wt.path.clone(),
                branch: wt.branch.clone(),
            },
            decision,
        );
    }

    if policy.delete_branches {
        for orphan in &snap.orphans {
            if let Some((target, decision)) = classify_orphan(orphan, snap, policy) {
                plan.push(target, decision);
            }
        }
    }

    plan
}

fn classify_orphan(
    orphan: &crate::snapshot::BranchSnapshot,
    snap: &RepoSnapshot,
    policy: &Policy,
) -> Option<(Target, Decision)> {
    // A branch checked out in a worktree is handled by the worktree pass.
    if orphan.worktree.is_some() {
        return None;
    }
    if snap.is_protected(&orphan.name) {
        return None;
    }
    let target = Target::Branch {
        name: orphan.name.clone(),
    };
    let Some(tip) = &orphan.tip else {
        return Some((
            target,
            Decision::Keep {
                reason: Reason::StatusUnknown,
            },
        ));
    };
    if !snap.reachable.contains(tip) {
        return Some((
            target,
            Decision::Keep {
                reason: Reason::HasUnpublishedCommits,
            },
        ));
    }
    let stale = orphan
        .commit_age_secs
        .is_some_and(|age| age >= policy.stale_after_secs);
    if stale {
        return Some((
            target,
            Decision::Delete {
                reason: Reason::Stale {
                    age_secs: orphan.commit_age_secs.unwrap_or_default(),
                },
                tip: tip.clone(),
            },
        ));
    }
    Some((
        target,
        Decision::Keep {
            reason: Reason::RecentlyActive {
                age_secs: orphan.commit_age_secs.unwrap_or_default(),
            },
        },
    ))
}

/// Convenience for callers that only care about worktrees with a real branch.
pub fn removable_worktree_paths(plan: &Plan) -> Vec<PathBuf> {
    plan.destructive()
        .filter_map(|(t, _)| match t {
            Target::Worktree { path, .. } => Some(path.clone()),
            Target::Branch { .. } => None,
        })
        .collect()
}

pub fn branches_to_delete(plan: &Plan) -> Vec<(BranchName, crate::model::Sha)> {
    plan.destructive()
        .filter_map(|(t, d)| match (t, d) {
            (Target::Branch { name }, Decision::Delete { tip, .. }) => {
                Some((name.clone(), tip.clone()))
            }
            _ => None,
        })
        .collect()
}
