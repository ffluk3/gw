//! Safety invariants, checked over generated states.
//!
//! These are the properties the tool must never violate regardless of what the
//! repository looks like. They are stated positively ("never destroys X unless
//! Y held") so a counterexample names the exact fact that was ignored.

use gw_core::classify::classify_clean;
use gw_core::{
    Activity, ActivitySource, BranchName, BranchSnapshot, ChangeObservation, ChangeState, Decision,
    Dirty, Layout, Policy, ProviderId, ReachableSet, RepoSnapshot, Sha, Strength, Target, TicketId,
    TicketObservation, TicketStatus, WorktreeKind, WorktreeSnapshot,
};
use std::path::PathBuf;

/// Small deterministic generator: enough entropy to explore interactions, and
/// no dev-dependency for a fixture this simple.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        let index = usize::try_from(self.next().rem_euclid(items.len() as u64))
            .expect("index is bounded by the slice length");
        &items[index]
    }

    fn bool(&mut self) -> bool {
        self.next().is_multiple_of(2)
    }
}

fn provider(raw: &str) -> ProviderId {
    ProviderId::parse(raw).expect("valid provider id")
}

/// A ticket observation with a randomized state, including the two states that
/// must never justify destruction on their own: unknown, and mention-strength.
fn ticket_observation(rng: &mut Rng) -> TicketObservation {
    let status = match rng.next() % 4 {
        0 => TicketStatus::Closed {
            resolution: Some("Done".into()),
        },
        1 => TicketStatus::Open,
        2 => TicketStatus::NotChecked,
        _ => TicketStatus::Unknown {
            detail: "probe failed".into(),
        },
    };
    TicketObservation {
        provider: provider("jira"),
        id: TicketId::parse("PROJ-1").expect("valid id"),
        strength: if rng.bool() {
            Strength::Identity
        } else {
            Strength::Mention
        },
        status,
        summary: None,
    }
}

fn change_observation(rng: &mut Rng) -> ChangeObservation {
    let state = match rng.next() % 4 {
        0 => ChangeState::Merged { reference: None },
        1 => ChangeState::Open { reference: None },
        2 => ChangeState::Unknown {
            detail: "probe failed".into(),
        },
        _ => ChangeState::Merged { reference: None },
    };
    ChangeObservation {
        provider: provider("github"),
        strength: if rng.bool() {
            Strength::Identity
        } else {
            Strength::Mention
        },
        selector: "head_branch".into(),
        state,
    }
}

fn sha_for(n: u64) -> Sha {
    // 40 hex characters, deterministic per index.
    Sha::parse(&format!("{n:040x}")).expect("valid sha")
}

fn snapshot(rng: &mut Rng, day: i64) -> (RepoSnapshot, Vec<Sha>) {
    // A fixed pool of "remote-reachable" commits that some branches point at.
    let reachable: Vec<Sha> = (0..4).map(sha_for).collect();
    let unpublished: Vec<Sha> = (100..104).map(sha_for).collect();

    let kinds = [
        WorktreeKind::Linked,
        WorktreeKind::Harness,
        WorktreeKind::Prunable,
    ];
    let dirty_states = [Dirty::Clean, Dirty::Dirty, Dirty::Unknown];

    let count = (rng.next() % 5) as usize;
    let mut worktrees = Vec::new();
    let mut protected = std::collections::BTreeSet::new();

    for i in 0..count {
        let branch = BranchName::parse(&format!("PROJ-{i}")).unwrap();
        let is_protected = rng.bool();
        if is_protected {
            protected.insert(branch.clone());
        }
        let head = if rng.bool() {
            Some(rng.pick(&reachable).clone())
        } else {
            Some(rng.pick(&unpublished).clone())
        };
        let age_days = i64::try_from(rng.next() % 30).expect("0..30 fits in i64");
        worktrees.push(WorktreeSnapshot {
            path: PathBuf::from(format!("/repo/worktrees/PROJ-{i}")),
            branch: Some(branch),
            head,
            kind: *rng.pick(&kinds),
            dirty: *rng.pick(&dirty_states),
            activity: Activity {
                age_secs: age_days * day,
                source: *rng.pick(&[ActivitySource::CommitDate, ActivitySource::Unknown]),
            },
            tickets: if rng.bool() {
                vec![ticket_observation(rng)]
            } else {
                Vec::new()
            },
            changes: if rng.bool() {
                vec![change_observation(rng)]
            } else {
                Vec::new()
            },
        });
    }

    let mut orphan_count = (rng.next() % 4) as usize;
    let mut orphans = Vec::new();
    while orphan_count > 0 {
        orphan_count -= 1;
        let i = rng.next() % 8;
        let branch = BranchName::parse(&format!("ORPHAN-{i}")).unwrap();
        let is_protected = rng.bool();
        if is_protected {
            protected.insert(branch.clone());
        }
        orphans.push(BranchSnapshot {
            name: branch,
            worktree: None,
            tip: Some(if rng.bool() {
                rng.pick(&reachable).clone()
            } else {
                rng.pick(&unpublished).clone()
            }),
            commit_age_secs: Some(i64::try_from(rng.next() % 30).expect("0..30 fits in i64") * day),
        });
    }

    // Every worktree branch is protected from ref deletion, but only the
    // default branch is protected from worktree removal.
    // (worktree branches are already in `protected`)
    for wt in &worktrees {
        if let Some(branch) = &wt.branch {
            protected.insert(branch.clone());
        }
    }

    let snap = RepoSnapshot {
        layout: Layout::new(PathBuf::from("/repo")),
        default_branch: Some(BranchName::parse("develop").unwrap()),
        head_branch: Some(BranchName::parse("develop").unwrap()),
        protected,
        reachable: ReachableSet::new(reachable),
        worktrees,
        orphans,
    };
    (snap, unpublished)
}

fn policy() -> Vec<Policy> {
    vec![
        Policy::default(),
        Policy {
            clean_closed_tickets: true,
            ..Policy::default()
        },
        Policy {
            delete_branches: false,
            ..Policy::default()
        },
        Policy {
            stale_after_secs: 0,
            ..Policy::default()
        },
        Policy {
            dry_run: true,
            ..Policy::default()
        },
    ]
}

fn run(seed: u64, day: i64, policy_index: usize) -> (RepoSnapshot, gw_core::Plan) {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let (snap, _) = snapshot(&mut rng, day);
    let policies = policy();
    let p = policies[policy_index % policies.len()].clone();
    let plan = classify_clean(&snap, &p);
    (snap, plan)
}

const DAY: i64 = 86_400;

#[test]
fn never_removes_a_dirty_or_unreadable_worktree() {
    for seed in 0..200 {
        for pi in 0..5 {
            let (snap, plan) = run(seed, DAY, pi);
            for (target, decision) in &plan.entries {
                if !matches!(decision, Decision::Remove { .. }) {
                    continue;
                }
                let Target::Worktree { path, .. } = target else {
                    continue;
                };
                let wt = snap
                    .worktrees
                    .iter()
                    .find(|w| &w.path == path)
                    .expect("plan targets a known worktree");
                assert_eq!(
                    wt.dirty,
                    Dirty::Clean,
                    "seed={seed} policy={pi} removed {path:?} without a clean status"
                );
            }
        }
    }
}

#[test]
fn never_removes_unpublished_commits() {
    for seed in 0..200 {
        for pi in 0..5 {
            let (snap, plan) = run(seed, DAY, pi);
            for (target, decision) in &plan.entries {
                if !matches!(decision, Decision::Remove { .. }) {
                    continue;
                }
                let Target::Worktree { path, .. } = target else {
                    continue;
                };
                let wt = snap
                    .worktrees
                    .iter()
                    .find(|w| &w.path == path)
                    .expect("known worktree");
                let head = wt.head.as_ref().expect("removal requires a known head");
                assert!(
                    snap.reachable.contains(head),
                    "seed={seed} policy={pi} removed {path:?} while its tip was unpublished"
                );
            }
        }
    }
}

#[test]
fn never_removes_a_protected_branch() {
    for seed in 0..200 {
        for pi in 0..5 {
            let (snap, plan) = run(seed, DAY, pi);
            for (target, decision) in &plan.entries {
                if !matches!(decision, Decision::Remove { .. }) {
                    continue;
                }
                let branch = target.branch_name().expect("removal has a branch");
                assert!(
                    !snap.is_worktree_protected(branch),
                    "seed={seed} policy={pi} removed protected branch {branch}"
                );
            }
        }
    }
}

#[test]
fn never_touches_the_harness_worktrees() {
    for seed in 0..200 {
        for pi in 0..5 {
            let (snap, plan) = run(seed, DAY, pi);
            for (target, decision) in &plan.entries {
                if !decision.is_destructive() {
                    continue;
                }
                let Target::Worktree { path, .. } = target else {
                    continue;
                };
                let wt = snap.worktrees.iter().find(|w| &w.path == path).unwrap();
                assert_ne!(
                    wt.kind,
                    WorktreeKind::Harness,
                    "seed={seed} policy={pi} planned destruction of a harness worktree"
                );
            }
        }
    }
}

#[test]
fn never_deletes_branches_unless_enabled() {
    let no_delete = Policy {
        delete_branches: false,
        ..Policy::default()
    };
    for seed in 0u64..200 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let (snap, _) = snapshot(&mut rng, DAY);
        let plan = classify_clean(&snap, &no_delete);
        for (_, decision) in &plan.entries {
            assert!(
                !matches!(decision, Decision::Delete { .. }),
                "seed={seed} deleted a branch with delete_branches disabled"
            );
        }
    }
}

#[test]
fn never_deletes_a_protected_or_unpublished_branch() {
    for seed in 0..200 {
        for pi in 0..5 {
            let (snap, plan) = run(seed, DAY, pi);
            for (target, decision) in &plan.entries {
                let Decision::Delete { tip, .. } = decision else {
                    continue;
                };
                let Target::Branch { name } = target else {
                    panic!("Delete must target a branch, got {target:?}");
                };
                assert!(
                    !snap.is_protected(name),
                    "seed={seed} policy={pi} deleted protected branch {name}"
                );
                assert!(
                    snap.reachable.contains(tip),
                    "seed={seed} policy={pi} deleted {name} with unpublished tip"
                );
            }
        }
    }
}

#[test]
fn classification_is_deterministic() {
    for seed in 0..50 {
        let (_, a) = run(seed, DAY, 0);
        let (_, b) = run(seed, DAY, 0);
        assert_eq!(a, b, "seed={seed} produced different plans across runs");
    }
}

#[test]
fn closed_tickets_are_only_removed_when_opted_in() {
    for seed in 0u64..200 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let (snap, _) = snapshot(&mut rng, DAY);
        let plan = classify_clean(&snap, &Policy::default());
        for (target, decision) in &plan.entries {
            let Target::Worktree { path, .. } = target else {
                continue;
            };
            let wt = snap.worktrees.iter().find(|w| &w.path == path).unwrap();
            let Some(resolution) = wt.closed_ticket_resolution() else {
                continue;
            };
            assert!(!resolution.is_empty());
            // With a closed ticket and no opt-in, the only legitimate removals
            // are ones justified by a stronger, already-enabled reason.
            if matches!(decision, Decision::Remove { .. }) {
                let justified = wt.has_merged_change() || wt.activity.age_secs >= 7 * DAY;
                assert!(
                    justified,
                    "seed={seed} removed a closed-ticket worktree without --closed and no other reason"
                );
            }
            // A closed ticket only reaches the ticket rule once every
            // protective and stronger signal has been cleared: clean, known
            // head that is published, not merged, and not stale. In that state
            // and without the opt-in it must surface as a recommendation.
            let reaches_ticket_rule = wt.kind == WorktreeKind::Linked
                && !snap.is_worktree_protected(wt.branch.as_ref().expect("linked has a branch"))
                && wt.dirty == Dirty::Clean
                && wt.head.as_ref().is_some_and(|h| snap.reachable.contains(h))
                && !wt.has_merged_change()
                && !(wt.activity.source != ActivitySource::Unknown
                    && wt.activity.age_secs >= 7 * DAY);
            if reaches_ticket_rule {
                assert!(
                    decision.is_recommendation(),
                    "seed={seed} closed ticket produced {decision:?} instead of a recommendation"
                );
            }
        }
    }
}

// ---- convention and provider evidence --------------------------------------
//
// The facts below are what a provider reported, and the properties say what
// such a fact may never do. They are separate from the worktree properties
// above because they hold even when every other fact would allow removal.

/// A worktree whose only remaining reason to remove it is a provider fact.
fn clean_stale_worktree(
    tickets: Vec<TicketObservation>,
    changes: Vec<ChangeObservation>,
) -> WorktreeSnapshot {
    let head = sha_for(1);
    WorktreeSnapshot {
        path: PathBuf::from("/repo/worktrees/PROJ-1"),
        branch: Some(BranchName::parse("PROJ-1").unwrap()),
        head: Some(head.clone()),
        kind: WorktreeKind::Linked,
        dirty: Dirty::Clean,
        activity: Activity {
            age_secs: 30 * DAY,
            source: ActivitySource::CommitDate,
        },
        tickets,
        changes,
    }
}

fn snapshot_with(worktree: WorktreeSnapshot) -> RepoSnapshot {
    RepoSnapshot {
        layout: Layout::new(PathBuf::from("/repo")),
        default_branch: Some(BranchName::parse("develop").unwrap()),
        head_branch: Some(BranchName::parse("develop").unwrap()),
        protected: Default::default(),
        reachable: ReachableSet::new(vec![sha_for(1)]),
        worktrees: vec![worktree],
        orphans: vec![],
    }
}

fn ticket(strength: Strength, status: TicketStatus) -> TicketObservation {
    TicketObservation {
        provider: provider("jira"),
        id: TicketId::parse("PROJ-1").unwrap(),
        strength,
        status,
        summary: None,
    }
}

fn change(strength: Strength, state: ChangeState) -> ChangeObservation {
    ChangeObservation {
        provider: provider("github"),
        strength,
        selector: "head_branch".into(),
        state,
    }
}

const RESOLVE: i64 = 30 * DAY;

#[test]
fn a_mention_strength_ticket_never_closes_a_worktree() {
    let snap = snapshot_with(clean_stale_worktree(
        vec![ticket(
            Strength::Mention,
            TicketStatus::Closed {
                resolution: Some("Done".into()),
            },
        )],
        vec![],
    ));
    let plan = classify_clean(
        &snap,
        &Policy {
            clean_closed_tickets: true,
            ..Policy::default()
        },
    );
    for (_, decision) in &plan.entries {
        assert!(
            !matches!(decision.reason(), gw_core::Reason::TicketClosed { .. }),
            "mention evidence produced a ticket-closed verdict: {decision:?}"
        );
    }
}

#[test]
fn a_mention_strength_change_never_merges_a_worktree() {
    let snap = snapshot_with(clean_stale_worktree(
        vec![],
        vec![change(
            Strength::Mention,
            ChangeState::Merged { reference: None },
        )],
    ));
    for (_, decision) in &classify_clean(&snap, &Policy::default()).entries {
        assert_ne!(
            decision.reason(),
            &gw_core::Reason::MergedPr,
            "mention evidence justified a merge: {decision:?}"
        );
    }
}

#[test]
fn an_unknown_ticket_never_produces_a_ticket_closed_verdict() {
    let snap = snapshot_with(clean_stale_worktree(
        vec![ticket(
            Strength::Identity,
            TicketStatus::Unknown {
                detail: "tracker down".into(),
            },
        )],
        vec![],
    ));
    for (_, decision) in &classify_clean(
        &snap,
        &Policy {
            clean_closed_tickets: true,
            ..Policy::default()
        },
    )
    .entries
    {
        assert!(
            !matches!(decision.reason(), gw_core::Reason::TicketClosed { .. }),
            "unreadable ticket became closed: {decision:?}"
        );
    }
}

/// Two providers, one says merged and one cannot answer. The disagreement is
/// not evidence, so the merge reason must not appear.
#[test]
fn a_contested_change_never_produces_a_merged_verdict() {
    let snap = snapshot_with(clean_stale_worktree(
        vec![],
        vec![
            change(Strength::Identity, ChangeState::Merged { reference: None }),
            change(
                Strength::Identity,
                ChangeState::Unknown {
                    detail: "provider failed".into(),
                },
            ),
        ],
    ));
    for (_, decision) in &classify_clean(&snap, &Policy::default()).entries {
        assert_ne!(
            decision.reason(),
            &gw_core::Reason::MergedPr,
            "a contested change justified removal: {decision:?}"
        );
    }
}

/// A closed ticket and an open change both appear; neither may be dropped in
/// favour of the other.
#[test]
fn a_contested_ticket_never_produces_a_ticket_closed_verdict() {
    let snap = snapshot_with(clean_stale_worktree(
        vec![
            ticket(
                Strength::Identity,
                TicketStatus::Closed {
                    resolution: Some("Done".into()),
                },
            ),
            ticket(Strength::Identity, TicketStatus::Open),
        ],
        vec![],
    ));
    for (_, decision) in &classify_clean(
        &snap,
        &Policy {
            clean_closed_tickets: true,
            ..Policy::default()
        },
    )
    .entries
    {
        assert!(
            !matches!(decision.reason(), gw_core::Reason::TicketClosed { .. }),
            "a contested ticket became closed: {decision:?}"
        );
    }
}

/// The removal that a strong, uncontested fact does authorize still happens.
/// Without this the properties above could pass by never removing anything.
#[test]
fn an_uncontested_merged_change_still_removes() {
    let snap = snapshot_with(clean_stale_worktree(
        vec![],
        vec![change(
            Strength::Identity,
            ChangeState::Merged { reference: None },
        )],
    ));
    let plan = classify_clean(&snap, &Policy::default());
    assert!(
        plan.entries
            .iter()
            .any(|(_, d)| d.reason() == &gw_core::Reason::MergedPr),
        "a clean, published, merged worktree was kept: {plan:?}"
    );
    assert!(plan.entries.iter().all(|(_, d)| match d {
        Decision::Remove { .. } => true,
        _ => d.reason() != &gw_core::Reason::Stale { age_secs: RESOLVE },
    }));
}
