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
#[must_use]
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
#[must_use]
pub fn removable_worktree_paths(plan: &Plan) -> Vec<PathBuf> {
    plan.destructive()
        .filter_map(|(t, _)| match t {
            Target::Worktree { path, .. } => Some(path.clone()),
            Target::Branch { .. } => None,
        })
        .collect()
}

#[must_use]
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

#[cfg(test)]
mod coverage_tests {
    use super::*;
    use crate::model::Sha;
    use crate::snapshot::{Activity, BranchSnapshot, Dirty, ReachableSet, WorktreeSnapshot};
    use std::collections::BTreeSet;

    fn sha(raw: &str) -> Sha {
        Sha::parse(raw).unwrap()
    }

    fn snapshot(orphans: Vec<BranchSnapshot>) -> RepoSnapshot {
        RepoSnapshot {
            layout: crate::layout::Layout::new(std::path::PathBuf::from("/repo")),
            default_branch: Some(BranchName::parse("develop").unwrap()),
            head_branch: None,
            protected: BTreeSet::new(),
            reachable: ReachableSet::new(vec![sha("aaaa")]),
            worktrees: Vec::new(),
            orphans,
        }
    }

    fn orphan(name: &str, tip: Option<&str>, worktree: Option<&str>) -> BranchSnapshot {
        BranchSnapshot {
            name: BranchName::parse(name).unwrap(),
            worktree: worktree.map(std::path::PathBuf::from),
            tip: tip.map(sha),
            commit_age_secs: Some(86_400 * 30),
        }
    }

    fn deleting_policy() -> Policy {
        Policy {
            delete_branches: true,
            ..Policy::default()
        }
    }

    #[test]
    fn orphans_are_ignored_without_the_opt_in() {
        let snap = snapshot(vec![orphan("PROJ-1", Some("aaaa"), None)]);
        let policy = Policy {
            delete_branches: false,
            ..Policy::default()
        };
        assert!(classify_clean(&snap, &policy).is_empty());
    }

    #[test]
    fn a_checked_out_branch_is_not_an_orphan_candidate() {
        let snap = snapshot(vec![orphan(
            "PROJ-1",
            Some("aaaa"),
            Some("/repo/worktrees/PROJ-1"),
        )]);
        assert!(classify_clean(&snap, &deleting_policy()).is_empty());
    }

    #[test]
    fn a_protected_branch_is_not_a_candidate() {
        let mut snap = snapshot(vec![orphan("develop", Some("aaaa"), None)]);
        snap.protected.insert(BranchName::parse("develop").unwrap());
        assert!(classify_clean(&snap, &deleting_policy()).is_empty());
    }

    #[test]
    fn an_orphan_without_a_tip_is_status_unknown() {
        let snap = snapshot(vec![orphan("PROJ-1", None, None)]);
        let plan = classify_clean(&snap, &deleting_policy());
        assert!(matches!(
            plan.entries[0].1,
            Decision::Keep {
                reason: Reason::StatusUnknown
            }
        ));
    }

    #[test]
    fn an_orphan_with_unpublished_commits_is_kept() {
        let snap = snapshot(vec![orphan("PROJ-1", Some("bbbb"), None)]);
        let plan = classify_clean(&snap, &deleting_policy());
        assert!(matches!(
            plan.entries[0].1,
            Decision::Keep {
                reason: Reason::HasUnpublishedCommits
            }
        ));
    }

    #[test]
    fn a_stale_published_orphan_is_deleted_with_its_tip() {
        let snap = snapshot(vec![orphan("PROJ-1", Some("aaaa"), None)]);
        let plan = classify_clean(&snap, &deleting_policy());
        match &plan.entries[0].1 {
            Decision::Delete { tip, reason } => {
                assert_eq!(tip.as_str(), "aaaa");
                assert!(matches!(reason, Reason::Stale { .. }));
            }
            other => panic!("expected Delete, got {other:?}"),
        }
        assert_eq!(
            removable_worktree_paths(&plan),
            Vec::<std::path::PathBuf>::new()
        );
        let deleted = branches_to_delete(&plan);
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0].1.as_str(), "aaaa");
    }

    #[test]
    fn a_recent_published_orphan_is_kept() {
        let mut branch = orphan("PROJ-1", Some("aaaa"), None);
        branch.commit_age_secs = Some(60);
        let snap = snapshot(vec![branch]);
        let plan = classify_clean(&snap, &deleting_policy());
        assert!(matches!(
            plan.entries[0].1,
            Decision::Keep {
                reason: Reason::RecentlyActive { .. }
            }
        ));
        assert!(branches_to_delete(&plan).is_empty());
    }

    #[test]
    fn a_bare_worktree_is_never_a_candidate_and_removals_are_collected() {
        let mut snap = snapshot(vec![]);
        snap.worktrees.push(WorktreeSnapshot {
            path: std::path::PathBuf::from("/repo/.bare"),
            branch: Some(BranchName::parse("develop").unwrap()),
            head: Some(sha("aaaa")),
            kind: WorktreeKind::Main,
            dirty: Dirty::Unknown,
            activity: Activity::unknown(),
            tickets: Vec::new(),
            changes: Vec::new(),
        });
        snap.worktrees.push(WorktreeSnapshot {
            path: std::path::PathBuf::from("/repo/worktrees/PROJ-9"),
            branch: Some(BranchName::parse("PROJ-9").unwrap()),
            head: Some(sha("aaaa")),
            kind: WorktreeKind::Linked,
            dirty: Dirty::Clean,
            activity: Activity::unknown(),
            tickets: Vec::new(),
            changes: vec![crate::snapshot::ChangeObservation {
                provider: crate::convention::ProviderId::parse("github").unwrap(),
                strength: crate::convention::Strength::Identity,
                selector: "head_branch".into(),
                branch: "PROJ-9".into(),
                state: crate::snapshot::ChangeState::Merged { reference: None },
            }],
        });

        let plan = classify_clean(&snap, &Policy::default());
        assert_eq!(plan.entries.len(), 1, "the bare entry was planned");
        assert_eq!(
            removable_worktree_paths(&plan),
            vec![std::path::PathBuf::from("/repo/worktrees/PROJ-9")]
        );
        assert!(branches_to_delete(&plan).is_empty());
    }
}
