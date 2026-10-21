use serde::Serialize;

use crate::decision::{Decision, Plan, Target};
use crate::model::BranchName;
use crate::render::human::describe_reason;

/// The stable machine-readable shape of a plan.
///
/// This is the output contract tests assert against. Presentation details
/// (column widths, emoji, colors) never appear here.
#[derive(Debug, Serialize)]
pub struct JsonPlan<'a> {
    /// Whether the plan was executed. `false` means this is the plan only:
    /// nothing was removed or deleted.
    pub applied: bool,
    pub entries: Vec<JsonEntry<'a>>,
    pub counts: crate::decision::Counts,
}

#[derive(Debug, Serialize)]
pub struct JsonEntry<'a> {
    pub target: &'a Target,
    #[serde(flatten)]
    pub decision: &'a Decision,
    pub description: String,
}

/// Render a plan as the stable machine-readable shape.
///
/// `applied` must say whether the plan was executed. A caller that only
/// rendered the plan passes `false`; the executor passes `true`. The field is
/// what lets a script tell a dry run from a destructive one without inferring
/// it from flags or the exit code.
#[must_use]
pub fn to_json(plan: &Plan, applied: bool) -> JsonPlan<'_> {
    JsonPlan {
        applied,
        entries: plan
            .entries
            .iter()
            .map(|(target, decision)| JsonEntry {
                target,
                decision,
                description: describe_reason(decision.reason()),
            })
            .collect(),
        counts: plan.counts(),
    }
}

#[must_use]
pub fn to_json_string(plan: &Plan, applied: bool) -> String {
    serde_json::to_string_pretty(&to_json(plan, applied))
        .unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
}

/// A branch name for display, collapsing the detached state.
#[must_use]
pub fn display_branch(branch: Option<&BranchName>) -> String {
    branch.map_or_else(|| "(detached)".to_string(), |b| b.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decision::{Decision, Plan, Reason, Target};
    use crate::model::{BranchName, Sha};

    fn plan() -> Plan {
        Plan {
            entries: vec![(
                Target::Branch {
                    name: BranchName::parse("PROJ-1").unwrap(),
                },
                Decision::Delete {
                    reason: Reason::Stale { age_secs: 100 },
                    tip: Sha::parse("9df03e38ae0").unwrap(),
                },
            )],
        }
    }

    #[test]
    fn applied_flag_is_rendered() {
        let planned: serde_json::Value =
            serde_json::from_str(&to_json_string(&plan(), false)).unwrap();
        assert_eq!(planned["applied"], serde_json::json!(false));

        let applied: serde_json::Value =
            serde_json::from_str(&to_json_string(&plan(), true)).unwrap();
        assert_eq!(applied["applied"], serde_json::json!(true));
    }

    #[test]
    fn an_empty_plan_still_carries_the_flag() {
        let empty = Plan { entries: vec![] };
        let doc: serde_json::Value = serde_json::from_str(&to_json_string(&empty, false)).unwrap();
        assert_eq!(doc["applied"], serde_json::json!(false));
        assert_eq!(doc["entries"], serde_json::json!([]));
    }
}
