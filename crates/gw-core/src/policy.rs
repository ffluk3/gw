use serde::Serialize;

/// Every threshold and opt-in that changes what `gw clean` is willing to
/// destroy. Constructed once at the CLI boundary and passed by reference into
/// the classifier, which never reads the environment itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Policy {
    /// A worktree with no file modified within this window is a staleness
    /// candidate.
    pub stale_after_secs: i64,
    /// Remove worktrees whose JIRA ticket is closed (`--closed`).
    pub clean_closed_tickets: bool,
    /// Delete orphan local branches that are fully pushed and stale.
    pub delete_branches: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            stale_after_secs: 7 * 86_400,
            clean_closed_tickets: false,
            delete_branches: true,
        }
    }
}
