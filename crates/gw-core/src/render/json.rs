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

pub fn to_json(plan: &Plan) -> JsonPlan<'_> {
    JsonPlan {
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

pub fn to_json_string(plan: &Plan) -> String {
    serde_json::to_string_pretty(&to_json(plan))
        .unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e))
}

/// A branch name for display, collapsing the detached state.
pub fn display_branch(branch: Option<&BranchName>) -> String {
    branch
        .map(|b| b.as_str().to_string())
        .unwrap_or_else(|| "(detached)".to_string())
}
