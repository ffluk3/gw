//! The interactive picker for `gw clean --interactive`.
//!
//! The terminal widget sits behind `Selector` so everything that decides what
//! may be chosen stays testable without a pty. The widget returns positions
//! into the list it was shown; `pick` maps them back to plan rows and hands
//! them to `gw_core::apply_selection`, which refuses anything protected.

use std::collections::BTreeSet;

use gw_core::render::human::describe_reason;
use gw_core::{apply_selection, Decision, Plan, Target};

use crate::error::Error;

/// One line offered to the user.
pub struct Choice {
    pub label: String,
}

pub trait Selector {
    /// Positions into `choices` the user marked. `None` means they cancelled.
    fn choose(&self, choices: &[Choice]) -> Result<Option<Vec<usize>>, Error>;
}

/// The real prompt. Nothing is preselected: destruction is opt-in per row.
pub struct TerminalSelector;

impl Selector for TerminalSelector {
    fn choose(&self, choices: &[Choice]) -> Result<Option<Vec<usize>>, Error> {
        dialoguer::MultiSelect::new()
            .with_prompt("Select worktrees to act on (Space marks, Enter applies, Esc cancels)")
            .items(choices.iter().map(|c| c.label.as_str()))
            .interact_on_opt(&console::Term::stdout())
            .map_err(|e| Error::Other(e.to_string()))
    }
}

fn action(decision: &Decision) -> &'static str {
    match decision {
        Decision::Remove { .. } | Decision::Recommend { .. } => "remove",
        Decision::Prune { .. } => "prune",
        Decision::Delete { .. } => "delete",
        Decision::Keep { .. } => "keep",
        Decision::Skip { .. } => "skip",
    }
}

fn label(target: &Target, decision: &Decision) -> String {
    let name = match target {
        Target::Worktree { branch, path } => branch
            .as_ref()
            .map_or_else(|| path.display().to_string(), |b| b.as_str().to_string()),
        Target::Branch { name } => name.as_str().to_string(),
    };
    format!(
        "{name} — {} ({})",
        action(decision),
        describe_reason(decision.reason())
    )
}

/// Offer the selectable rows and return the plan reduced to the picks.
///
/// An empty result means nothing selectable, a cancel, or an empty pick; the
/// caller reports "nothing changed" in each case.
pub fn pick(plan: &Plan, selector: &dyn Selector) -> Result<Plan, Error> {
    let indices = plan.selectable_indices();
    if indices.is_empty() {
        return Ok(Plan::new());
    }
    let choices: Vec<Choice> = indices
        .iter()
        .map(|&i| {
            let (target, decision) = &plan.entries[i];
            Choice {
                label: label(target, decision),
            }
        })
        .collect();
    let Some(positions) = selector.choose(&choices)? else {
        return Ok(Plan::new());
    };
    let mut picked = BTreeSet::new();
    for position in positions {
        let index = indices.get(position).copied().ok_or_else(|| {
            Error::Other(format!(
                "selector returned position {position} out of range"
            ))
        })?;
        picked.insert(index);
    }
    apply_selection(plan, &picked).map_err(|e| Error::Other(e.to_string()))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::PathBuf;

    use gw_core::{BranchName, Reason, Sha};

    use super::*;

    struct Fake {
        answer: Option<Vec<usize>>,
        seen: RefCell<Vec<String>>,
        calls: RefCell<usize>,
    }

    impl Fake {
        fn new(answer: Option<Vec<usize>>) -> Self {
            Self {
                answer,
                seen: RefCell::new(Vec::new()),
                calls: RefCell::new(0),
            }
        }
    }

    impl Selector for Fake {
        fn choose(&self, choices: &[Choice]) -> Result<Option<Vec<usize>>, Error> {
            *self.calls.borrow_mut() += 1;
            *self.seen.borrow_mut() = choices.iter().map(|c| c.label.clone()).collect();
            Ok(self.answer.clone())
        }
    }

    fn wt(name: &str) -> Target {
        Target::Worktree {
            path: PathBuf::from(format!("/repo/worktrees/{name}")),
            branch: Some(BranchName::parse(name).unwrap()),
        }
    }

    fn plan() -> Plan {
        let mut p = Plan::new();
        p.push(
            wt("keepme"),
            Decision::Keep {
                reason: Reason::Protected,
            },
        );
        p.push(
            wt("merged"),
            Decision::Remove {
                reason: Reason::MergedPr,
            },
        );
        p.push(
            wt("dirty"),
            Decision::Skip {
                reason: Reason::HasUncommittedChanges,
            },
        );
        p.push(
            Target::Worktree {
                path: PathBuf::from("/repo/worktrees/detached"),
                branch: None,
            },
            Decision::Prune {
                reason: Reason::Prunable,
            },
        );
        p.push(
            wt("closed"),
            Decision::Recommend {
                reason: Reason::TicketClosed {
                    resolution: "Done".into(),
                },
            },
        );
        p.push(
            Target::Branch {
                name: BranchName::parse("orphan").unwrap(),
            },
            Decision::Delete {
                reason: Reason::Stale { age_secs: 9 },
                tip: Sha::parse("9df03e38ae0").unwrap(),
            },
        );
        p
    }

    #[test]
    fn picks_map_back_to_plan_rows() {
        let fake = Fake::new(Some(vec![2, 0]));
        let out = pick(&plan(), &fake).unwrap();
        let rows: Vec<_> = out.entries.iter().map(|(t, _)| t.clone()).collect();
        assert_eq!(rows, vec![wt("merged"), wt("closed"),]);
        assert!(matches!(out.entries[1].1, Decision::Remove { .. }));
    }

    #[test]
    fn read_only_rows_are_never_offered_and_labels_explain() {
        let fake = Fake::new(Some(vec![]));
        let out = pick(&plan(), &fake).unwrap();
        assert!(out.is_empty());
        let seen = fake.seen.borrow();
        assert_eq!(seen.len(), 4);
        assert!(!seen
            .iter()
            .any(|l| l.contains("keepme") || l.contains("dirty")));
        assert!(seen[0].contains("merged") && seen[0].contains("remove"));
        assert!(seen[1].contains("/repo/worktrees/detached") && seen[1].contains("prune"));
        assert!(seen[2].contains("closed") && seen[2].contains("remove"));
        assert!(seen[3].contains("orphan") && seen[3].contains("delete"));
    }

    #[test]
    fn cancelling_yields_an_empty_plan() {
        let fake = Fake::new(None);
        assert!(pick(&plan(), &fake).unwrap().is_empty());
    }

    #[test]
    fn no_selectable_rows_never_calls_the_selector() {
        let mut p = Plan::new();
        p.push(
            wt("a"),
            Decision::Keep {
                reason: Reason::Protected,
            },
        );
        let fake = Fake::new(Some(vec![0]));
        assert!(pick(&p, &fake).unwrap().is_empty());
        assert_eq!(*fake.calls.borrow(), 0);
    }

    #[test]
    fn an_out_of_range_position_is_an_error() {
        let fake = Fake::new(Some(vec![40]));
        assert!(matches!(pick(&plan(), &fake), Err(Error::Other(_))));
    }

    #[test]
    fn read_only_decisions_are_named_keep_and_skip() {
        let r = || Reason::Protected;
        assert_eq!(action(&Decision::Keep { reason: r() }), "keep");
        assert_eq!(action(&Decision::Skip { reason: r() }), "skip");
    }
}
