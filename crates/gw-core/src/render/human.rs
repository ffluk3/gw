use crate::decision::Reason;

/// Format a duration in the same units the shell implementation used:
/// minutes, hours, days, weeks, months — always rounded down and suffixed
/// with "ago".
#[must_use]
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
#[must_use]
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
    }
}

/// The glyph shown in the status column.
#[must_use]
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
    }
}

/// The restore hint printed after a branch deletion.
#[must_use]
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
        assert_eq!(restore_hint("PROJ-1", "abc123"), "git branch PROJ-1 abc123");
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::*;

    #[test]
    fn every_reason_has_a_description_and_a_glyph() {
        let reasons = [
            Reason::Protected,
            Reason::HarnessWorktree,
            Reason::DetachedHead,
            Reason::Prunable,
            Reason::HasUnpublishedCommits,
            Reason::HasUncommittedChanges,
            Reason::StatusUnknown,
            Reason::RecentlyActive { age_secs: 120 },
            Reason::Stale { age_secs: 86_400 },
            Reason::MergedPr,
            Reason::TicketClosed {
                resolution: "Fixed".into(),
            },
            Reason::NoMergedPr,
        ];
        for reason in &reasons {
            assert!(!describe_reason(reason).is_empty(), "{reason:?}");
            assert!(!glyph(reason).is_empty(), "{reason:?}");
        }
    }

    #[test]
    fn descriptions_use_the_documented_wording() {
        assert_eq!(describe_reason(&Reason::Protected), "protected branch");
        assert_eq!(describe_reason(&Reason::NoMergedPr), "no merged PR found");
        assert_eq!(
            describe_reason(&Reason::TicketClosed {
                resolution: "Done".into()
            }),
            "ticket closed (Done)"
        );
        assert_eq!(
            describe_reason(&Reason::Stale {
                age_secs: 3 * 86_400
            }),
            "stale (3d ago)"
        );
    }
}
