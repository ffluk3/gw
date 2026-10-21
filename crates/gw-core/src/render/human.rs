use crate::decision::Reason;

/// Format a duration in the same units the shell implementation used:
/// minutes, hours, days, weeks, months — always rounded down and suffixed
/// with "ago".
pub fn short_age(age_secs: i64) -> String {
    let age = age_secs.max(0);
    if age < 3_600 {
        format!("{}m ago", age / 60)
    } else if age < 86_400 {
        format!("{}h ago", age / 3_600)
    } else if age < 604_800 {
        format!("{}d ago", age / 86_400)
    } else if age < 2_592_000 {
        format!("{}w ago", age / 604_800)
    } else {
        format!("{}mo ago", age / 2_592_000)
    }
}

/// A single-line human description of why a decision was reached.
pub fn describe_reason(reason: &Reason) -> String {
    match reason {
        Reason::Protected => "protected branch".into(),
        Reason::HarnessWorktree => "agent harness worktree".into(),
        Reason::DetachedHead => "detached HEAD".into(),
        Reason::Prunable => "prunable worktree".into(),
        Reason::HasUnpublishedCommits => "has unpublished commits".into(),
        Reason::HasUncommittedChanges => "has uncommitted changes".into(),
        Reason::StatusUnknown => "status could not be read".into(),
        Reason::RecentlyActive { age_secs } => format!("active {}", short_age(*age_secs)),
        Reason::Stale { age_secs } => format!("stale ({})", short_age(*age_secs)),
        Reason::MergedPr => "PR merged".into(),
        Reason::TicketClosed { resolution } => format!("ticket closed ({resolution})"),
        Reason::NoMergedPr => "no merged PR found".into(),
        Reason::NoMergedPrTicketClosed { resolution } => {
            format!("no merged PR; ticket closed ({resolution})")
        }
    }
}

/// The glyph shown in the status column.
pub fn glyph(reason: &Reason) -> &'static str {
    match reason {
        Reason::Prunable
        | Reason::MergedPr
        | Reason::Stale { .. }
        | Reason::TicketClosed { .. } => "🗑️",
        Reason::HasUnpublishedCommits | Reason::HasUncommittedChanges | Reason::StatusUnknown => {
            "⚠️"
        }
        Reason::HarnessWorktree | Reason::DetachedHead | Reason::NoMergedPr => "⏭",
        Reason::Protected | Reason::RecentlyActive { .. } => "🔒",
        Reason::NoMergedPrTicketClosed { .. } => "🎫",
    }
}

/// The restore hint printed after a branch deletion.
pub fn restore_hint(branch: &str, sha: &str) -> String {
    format!("git branch {branch} {sha}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ages_match_previous_output() {
        assert_eq!(short_age(59), "0m ago");
        assert_eq!(short_age(3_600), "1h ago");
        assert_eq!(short_age(86_400 * 3), "3d ago");
        assert_eq!(short_age(604_800 * 3), "3w ago");
        assert_eq!(short_age(2_592_000 * 2), "2mo ago");
    }

    #[test]
    fn negative_ages_are_clamped() {
        assert_eq!(short_age(-500), "0m ago");
    }

    #[test]
    fn restore_hint_is_usable() {
        assert_eq!(restore_hint("AX-1", "abc123"), "git branch AX-1 abc123");
    }
}
