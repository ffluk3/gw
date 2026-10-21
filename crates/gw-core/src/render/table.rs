use crate::decision::{Decision, Plan, Target};
use crate::render::human::describe_reason;

/// Render a plan as an aligned text table.
///
/// The shell implementation piped tab-separated rows through `column -t`, so
/// emoji and wide characters misaligned. Here widths are computed on display
/// columns, where East Asian wide characters count as two.
#[must_use]
pub fn render_table(plan: &Plan) -> String {
    if plan.is_empty() {
        return "(no worktrees)\n".to_string();
    }

    let mut rows: Vec<[String; 4]> = Vec::with_capacity(plan.entries.len() + 1);
    rows.push([
        "BRANCH".into(),
        "ACTION".into(),
        "PATH".into(),
        "STATUS".into(),
    ]);

    for (target, decision) in &plan.entries {
        let (branch, path) = match target {
            Target::Worktree { path, branch } => (
                branch
                    .as_ref()
                    .map_or_else(|| "(detached)".into(), |b| b.as_str().to_string()),
                path.display().to_string(),
            ),
            Target::Branch { name } => (name.as_str().to_string(), "—".to_string()),
        };
        let action = match decision {
            Decision::Remove { .. } => "remove",
            Decision::Prune { .. } => "prune",
            Decision::Delete { .. } => "delete",
            Decision::Keep { .. } => "keep",
            Decision::Skip { .. } => "skip",
            Decision::Recommend { .. } => "recommend",
        };
        rows.push([
            branch,
            action.into(),
            path,
            describe_reason(decision.reason()),
        ]);
    }

    let mut widths = [0usize; 4];
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(display_width(cell));
        }
    }

    let mut out = String::new();
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            out.push_str(cell);
            if i + 1 < row.len() {
                let pad = widths[i].saturating_sub(display_width(cell)) + 2;
                out.push_str(&" ".repeat(pad));
            }
        }
        out.push('\n');
    }
    out
}

/// Render a single status line, used by `gw status`.
#[must_use]
pub fn render_status_row(
    branch: &str,
    age: &str,
    dirty: bool,
    ticket: Option<&str>,
    summary: Option<&str>,
) -> String {
    let marker = if dirty { "●" } else { " " };
    match (ticket, summary) {
        (Some(key), Some(text)) => format!("{branch}\t{age}\t{marker}\t{key}\t{text}"),
        (Some(key), None) => format!("{branch}\t{age}\t{marker}\t{key}\t"),
        _ => format!("{branch}\t{age}\t{marker}\t"),
    }
}

/// Number of terminal columns a string occupies, counting East Asian wide and
/// fullwidth characters as two and zero-width joiners/emoji modifiers as zero.
#[must_use]
pub fn display_width(s: &str) -> usize {
    use unicode_width::UnicodeWidthChar;
    s.chars()
        .filter(|c| !matches!(c, '\u{200d}' | '\u{fe0f}'))
        .map(|c| c.width().unwrap_or(0))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_plan_message() {
        assert_eq!(render_table(&Plan::new()), "(no worktrees)\n");
    }

    #[test]
    fn ascii_width() {
        assert_eq!(display_width("PROJ-1"), 6);
    }

    #[test]
    fn narrow_characters_are_one_column() {
        assert_eq!(display_width("abc"), 3);
    }

    #[test]
    fn wide_characters_are_two_columns() {
        // CJK ideograph
        assert_eq!(display_width("漢"), 2);
    }

    #[test]
    fn variation_selector_is_zero_width() {
        assert_eq!(display_width("\u{fe0f}"), 0);
    }
}
