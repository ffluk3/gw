//! Pure selection over a classified plan.
//!
//! Interactive cleaning lets a person pick rows from a plan that
//! `classify_clean` already produced. Selection is a filter, not a decision:
//! it never re-runs the rules and never turns a protected row into work.

use std::collections::BTreeSet;
use std::fmt;

use crate::decision::{Decision, Plan, Target};

/// Whether a person may pick this row.
///
/// There is no `_` arm: a new `Decision` variant fails to compile until its
/// selectability is decided here.
#[must_use]
pub fn is_selectable(target: &Target, decision: &Decision) -> bool {
    match decision {
        Decision::Remove { .. } | Decision::Prune { .. } | Decision::Delete { .. } => true,
        // A recommendation promotes to `Remove`, which needs no restore data.
        // A branch recommendation would promote to a `Delete` without a tip,
        // so it is never selectable.
        Decision::Recommend { .. } => matches!(target, Target::Worktree { .. }),
        Decision::Keep { .. } | Decision::Skip { .. } => false,
    }
}

/// Why a selection was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionError {
    /// The index is past the end of the plan.
    OutOfRange { index: usize },
    /// The index names a row that may not be picked.
    NotSelectable { index: usize },
}

impl fmt::Display for SelectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SelectionError::OutOfRange { index } => {
                write!(f, "selection {index} is outside the plan")
            }
            SelectionError::NotSelectable { index } => {
                write!(f, "selection {index} names a row that cannot be selected")
            }
        }
    }
}

impl std::error::Error for SelectionError {}

impl Plan {
    /// Positions of the rows a person may pick, in plan order.
    #[must_use]
    pub fn selectable_indices(&self) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, (target, decision))| is_selectable(target, decision))
            .map(|(i, _)| i)
            .collect()
    }
}

/// The plan reduced to the picked rows, in original order.
///
/// `Recommend` on a worktree becomes `Remove` with the same reason. Other
/// selectable decisions pass through unchanged, including a `Delete` tip. The
/// first bad index fails the whole call.
pub fn apply_selection(plan: &Plan, picked: &BTreeSet<usize>) -> Result<Plan, SelectionError> {
    let mut out = Plan::new();
    for &index in picked {
        let (target, decision) = plan
            .entries
            .get(index)
            .ok_or(SelectionError::OutOfRange { index })?;
        if !is_selectable(target, decision) {
            return Err(SelectionError::NotSelectable { index });
        }
        let decision = match decision {
            Decision::Recommend { reason } => Decision::Remove {
                reason: reason.clone(),
            },
            other => other.clone(),
        };
        out.push(target.clone(), decision);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::decision::Reason;
    use crate::model::{BranchName, Sha};

    fn wt(name: &str) -> Target {
        Target::Worktree {
            path: PathBuf::from(format!("/repo/worktrees/{name}")),
            branch: Some(BranchName::parse(name).unwrap()),
        }
    }

    fn br(name: &str) -> Target {
        Target::Branch {
            name: BranchName::parse(name).unwrap(),
        }
    }

    fn closed() -> Reason {
        Reason::TicketClosed {
            resolution: "Done".into(),
        }
    }

    fn tip() -> Sha {
        Sha::parse("9df03e38ae0").unwrap()
    }

    fn plan() -> Plan {
        let mut p = Plan::new();
        p.push(
            wt("a"),
            Decision::Remove {
                reason: Reason::MergedPr,
            },
        );
        p.push(
            wt("b"),
            Decision::Keep {
                reason: Reason::Protected,
            },
        );
        p.push(wt("c"), Decision::Recommend { reason: closed() });
        p.push(
            wt("d"),
            Decision::Skip {
                reason: Reason::HasUncommittedChanges,
            },
        );
        p.push(
            wt("e"),
            Decision::Prune {
                reason: Reason::Prunable,
            },
        );
        p.push(
            br("f"),
            Decision::Delete {
                reason: Reason::Stale { age_secs: 9 },
                tip: tip(),
            },
        );
        p.push(br("g"), Decision::Recommend { reason: closed() });
        p
    }

    fn set(items: &[usize]) -> BTreeSet<usize> {
        items.iter().copied().collect()
    }

    #[test]
    fn each_decision_has_the_expected_selectability() {
        assert_eq!(plan().selectable_indices(), vec![0, 2, 4, 5]);
    }

    #[test]
    fn a_recommended_worktree_becomes_a_remove_with_the_same_reason() {
        let out = apply_selection(&plan(), &set(&[2])).unwrap();
        assert_eq!(
            out.entries,
            vec![(wt("c"), Decision::Remove { reason: closed() })]
        );
    }

    #[test]
    fn a_recommended_branch_is_rejected() {
        assert_eq!(
            apply_selection(&plan(), &set(&[6])),
            Err(SelectionError::NotSelectable { index: 6 })
        );
    }

    #[test]
    fn delete_passes_through_with_its_tip() {
        let out = apply_selection(&plan(), &set(&[5])).unwrap();
        assert_eq!(out.entries[0], plan().entries[5]);
    }

    #[test]
    fn keep_and_skip_are_rejected() {
        assert_eq!(
            apply_selection(&plan(), &set(&[1])),
            Err(SelectionError::NotSelectable { index: 1 })
        );
        assert_eq!(
            apply_selection(&plan(), &set(&[0, 3])),
            Err(SelectionError::NotSelectable { index: 3 })
        );
    }

    #[test]
    fn an_index_past_the_end_is_rejected() {
        assert_eq!(
            apply_selection(&plan(), &set(&[99])),
            Err(SelectionError::OutOfRange { index: 99 })
        );
    }

    #[test]
    fn order_is_preserved_and_empty_selection_is_empty() {
        let out = apply_selection(&plan(), &set(&[4, 0])).unwrap();
        let targets: Vec<_> = out.entries.iter().map(|(t, _)| t.clone()).collect();
        assert_eq!(targets, vec![wt("a"), wt("e")]);
        assert!(apply_selection(&plan(), &set(&[])).unwrap().is_empty());
    }

    #[test]
    fn errors_render_a_message() {
        assert!(SelectionError::OutOfRange { index: 3 }
            .to_string()
            .contains('3'));
        assert!(SelectionError::NotSelectable { index: 4 }
            .to_string()
            .contains("cannot be selected"));
    }
}
