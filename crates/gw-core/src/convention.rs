//! The convention: a pure, ordered mapping from git facts to provider queries.
//!
//! A *provider* talks to an external system. A *convention* decides what a
//! branch name means and therefore which systems `gw` should ask about it. The
//! two are separate so that changing a tracker or a code host is configuration,
//! not a code change.
//!
//! Nothing here performs I/O. [`Convention::resolve`] is a total function over
//! a branch name, so the mapping is testable without a repository and the
//! classifier never has to know which vendor answered.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::model::{ModelError, TicketId};

/// The set of valid provider identifiers.
const RESERVED_PROVIDER_CHARS: &[char] = &[':', '/', '\\', ' ', '\t', '\n'];

/// A provider name, used to scope both configuration and observations.
///
/// A slug rather than a free string: it appears in configuration keys, in the
/// `--skip-provider` flag, and in JSON, so it must be stable and unambiguous.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderId(String);

impl ProviderId {
    pub fn parse(raw: &str) -> Result<Self, ModelError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(ModelError("provider id is empty"));
        }
        if trimmed.len() > 64 {
            return Err(ModelError("provider id is implausibly long"));
        }
        if trimmed.chars().any(|c| c.is_control()) {
            return Err(ModelError("provider id contains a control character"));
        }
        if RESERVED_PROVIDER_CHARS.iter().any(|c| trimmed.contains(*c)) {
            return Err(ModelError("provider id contains a reserved character"));
        }
        Ok(Self(trimmed.to_ascii_lowercase()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// How much a matching rule proves.
///
/// Only [`Strength::Identity`] may justify destruction. A rule that merely
/// finds a ticket mentioned in text proves a topic, not ownership, and a
/// worktree removed on a topic match would be the exact failure mode the whole
/// fail-closed design exists to prevent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Strength {
    /// The rule identifies the work: the branch *is* the ticket, or the change
    /// *is* the branch.
    Identity,
    /// The rule associates a topic. Display and recommendation only.
    Mention,
}

impl Strength {
    #[must_use]
    pub fn is_identity(self) -> bool {
        matches!(self, Strength::Identity)
    }

    /// Whether a fact at this strength may authorize a destructive decision.
    #[must_use]
    pub fn permits_destruction(self) -> bool {
        self.is_identity()
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Strength::Identity => "identity",
            Strength::Mention => "mention",
        }
    }
}

/// How to find a ticket identifier for a branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "selector")]
pub enum TicketSelector {
    /// Scan the branch name with a regular expression and return the first
    /// capture group (or the whole match when the pattern has no group).
    ///
    /// This is the built-in default's selector: it reproduces the original
    /// hand-written scanner exactly.
    Embedded {
        pattern: String,
        /// Project prefixes to accept, compared case-insensitively against
        /// the leading `PREFIX-` of a candidate. Empty means any.
        #[serde(default)]
        allow_projects: Vec<String>,
    },
}

impl TicketSelector {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            TicketSelector::Embedded { .. } => "embedded",
        }
    }
}

/// How to find a change (pull request, merge request) for a branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "selector")]
pub enum ChangeSelector {
    /// The change's head branch has this exact name — the 1:1 mapping.
    HeadBranch,
    /// The host reports whether a commit is contained in a merged change.
    ///
    /// Not implemented by any provider yet; reserving the shape keeps the enum
    /// exhaustive so a provider must write down its consequence.
    ContainsCommit,
    /// A change whose text mentions the branch's ticket identifier.
    MentionTicket,
}

impl ChangeSelector {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            ChangeSelector::HeadBranch => "head_branch",
            ChangeSelector::ContainsCommit => "contains_commit",
            ChangeSelector::MentionTicket => "mention_ticket",
        }
    }
}

/// One ordered rule that maps a branch to a ticket lookup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketRule {
    pub provider: ProviderId,
    pub selector: TicketSelector,
    #[serde(default = "default_strength")]
    pub strength: Strength,
}

fn default_strength() -> Strength {
    Strength::Identity
}

/// One ordered rule that maps a branch to a change lookup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeRule {
    pub provider: ProviderId,
    pub selector: ChangeSelector,
    #[serde(default = "default_strength")]
    pub strength: Strength,
}

/// A resolved ticket for a branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TicketRef {
    pub provider: ProviderId,
    pub id: TicketId,
    pub strength: Strength,
    /// Which selector produced this reference, for `gw convention explain`.
    pub selector: String,
}

/// A change lookup to perform for a branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangeQuery {
    pub provider: ProviderId,
    pub selector: ChangeSelector,
    pub strength: Strength,
    pub branch: String,
    /// The ticket identifier to search for, for selectors that need one.
    pub ticket: Option<TicketId>,
}

impl ChangeQuery {
    /// A stable key used to correlate a query with its observation.
    #[must_use]
    pub fn key(&self) -> String {
        format!(
            "{}:{}:{}",
            self.provider.as_str(),
            self.selector.as_str(),
            self.branch
        )
    }
}

/// Everything a convention derived from one branch name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Resolution {
    pub tickets: Vec<TicketRef>,
    pub changes: Vec<ChangeQuery>,
}

impl Resolution {
    /// The first identity-strength ticket reference, which is what the status
    /// column and the default display use.
    #[must_use]
    pub fn primary_ticket(&self) -> Option<&TicketRef> {
        self.tickets.iter().find(|t| t.strength.is_identity())
    }

    /// Providers that need a ticket lookup, in declaration order, deduplicated.
    #[must_use]
    pub fn ticket_providers(&self) -> Vec<&ProviderId> {
        let mut seen = BTreeSet::new();
        self.tickets
            .iter()
            .filter(|t| seen.insert(t.provider.clone()))
            .map(|t| &t.provider)
            .collect()
    }
}

/// The convention in force: ordered rules, one list per capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Convention {
    pub ticket_rules: Vec<TicketRule>,
    pub change_rules: Vec<ChangeRule>,
}

impl Convention {
    /// The built-in convention: a JIRA-shaped key embedded in the branch name,
    /// and a GitHub PR whose head branch is that same branch name.
    ///
    /// This is the behavior earlier versions compiled in. It stays the default
    /// so configuration always has a sane starting point to specialize.
    #[must_use]
    pub fn built_in() -> Self {
        Self {
            ticket_rules: vec![TicketRule {
                provider: ProviderId("jira".to_string()),
                selector: TicketSelector::Embedded {
                    pattern: DEFAULT_TICKET_PATTERN.to_string(),
                    allow_projects: Vec::new(),
                },
                strength: Strength::Identity,
            }],
            change_rules: vec![ChangeRule {
                provider: ProviderId("github".to_string()),
                selector: ChangeSelector::HeadBranch,
                strength: Strength::Identity,
            }],
        }
    }

    #[must_use]
    pub fn is_built_in(&self) -> bool {
        *self == Self::built_in()
    }

    /// Apply the convention to one branch name.
    ///
    /// Rules are evaluated in order. Ticket rules stop at the first match in
    /// the same way the original scanner took the first valid candidate, so a
    /// branch mentioning several keys resolves to one. Change rules all
    /// contribute, because a branch can legitimately have more than one change
    /// query against a host.
    #[must_use]
    pub fn resolve(&self, branch: &str) -> Resolution {
        let mut resolution = Resolution::default();
        let mut seen_tickets: BTreeSet<(ProviderId, TicketId)> = BTreeSet::new();

        for rule in &self.ticket_rules {
            let Some(id) = rule.selector.select(branch) else {
                continue;
            };
            if !seen_tickets.insert((rule.provider.clone(), id.clone())) {
                continue;
            }
            resolution.tickets.push(TicketRef {
                provider: rule.provider.clone(),
                id,
                strength: rule.strength,
                selector: rule.selector.as_str().to_string(),
            });
        }

        let primary = resolution.primary_ticket().cloned();
        for rule in &self.change_rules {
            // A selector that needs a ticket cannot fire without one.
            if matches!(
                rule.selector,
                ChangeSelector::MentionTicket | ChangeSelector::ContainsCommit
            ) && primary.is_none()
            {
                continue;
            }
            resolution.changes.push(ChangeQuery {
                provider: rule.provider.clone(),
                selector: rule.selector.clone(),
                strength: rule.strength,
                branch: branch.to_string(),
                ticket: primary.as_ref().map(|t| t.id.clone()),
            });
        }

        resolution
    }
}

impl Default for Convention {
    fn default() -> Self {
        Self::built_in()
    }
}

/// The default ticket pattern: `[A-Z][A-Z0-9]+-[0-9]+`.
///
/// It matches the hand-written scanner's shape exactly, including its refusal
/// to match a lowercase prefix.
pub const DEFAULT_TICKET_PATTERN: &str = r"[A-Z][A-Z0-9]+-[0-9]+";

impl TicketSelector {
    /// The first identifier this selector finds in `branch`.
    #[must_use]
    pub fn select(&self, branch: &str) -> Option<TicketId> {
        match self {
            TicketSelector::Embedded {
                pattern,
                allow_projects,
            } => embedded_match(pattern, allow_projects, branch),
        }
    }
}

/// Compile a pattern without matching anything.
///
/// Configuration uses this to reject an uncompilable pattern at load time.
pub fn validate_pattern(pattern: &str) -> Result<(), String> {
    regex::Regex::new(pattern)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Run an `Embedded` selector.
///
/// Compiled on each call rather than cached: the convention resolves a few
/// dozen branches per run and a `regex::RegexSet` cache would add interior
/// mutability and a lock to a pure module for no measurable gain.
fn embedded_match(pattern: &str, allow_projects: &[String], branch: &str) -> Option<TicketId> {
    let regex = regex::Regex::new(pattern).ok()?;
    let mut search_from = 0;
    while search_from <= branch.len() {
        let captures = regex.captures_at(branch, search_from)?;
        let whole = captures.get(0)?;
        let candidate = captures
            .name("id")
            .or_else(|| captures.get(1))
            .map_or_else(|| whole.as_str(), |m| m.as_str());
        if prefix_allowed(candidate, allow_projects) {
            return TicketId::parse(candidate).ok();
        }
        // Skip past the rejected candidate instead of rescanning its interior,
        // so `XPROJ-1` can never satisfy a filter for `PROJ`.
        search_from = whole.end();
        if search_from >= branch.len() {
            return None;
        }
    }
    None
}

fn prefix_allowed(candidate: &str, allow_projects: &[String]) -> bool {
    if allow_projects.is_empty() {
        return true;
    }
    let prefix = candidate.split_once('-').map_or(candidate, |(p, _)| p);
    allow_projects
        .iter()
        .any(|p| p.eq_ignore_ascii_case(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn embedded(pattern: &str, allow: &[&str]) -> TicketSelector {
        TicketSelector::Embedded {
            pattern: pattern.to_string(),
            allow_projects: allow.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn provider_ids_are_slugs() {
        assert_eq!(ProviderId::parse(" JIRA ").unwrap().as_str(), "jira");
        assert!(ProviderId::parse("").is_err());
        assert!(ProviderId::parse("a:b").is_err());
        assert!(ProviderId::parse("a/b").is_err());
    }

    #[test]
    fn only_identity_permits_destruction() {
        assert!(Strength::Identity.permits_destruction());
        assert!(!Strength::Mention.permits_destruction());
    }

    #[test]
    fn default_convention_reproduces_the_original_scanner() {
        let convention = Convention::built_in();
        let cases = [
            ("PROJ-47383", Some("PROJ-47383")),
            ("feature/PROJ-123-thing", Some("PROJ-123")),
            ("agents/right-now-when", None),
            ("family/platform/1.408", None),
            ("ABC123-9-x", Some("ABC123-9")),
            ("A-1", None),
            ("feature/proj-123", None),
        ];
        for (branch, expected) in cases {
            let got = convention.resolve(branch);
            assert_eq!(
                got.primary_ticket().map(|t| t.id.as_str()),
                expected,
                "branch={branch}"
            );
        }
    }

    #[test]
    fn default_convention_queries_the_head_branch() {
        let resolution = Convention::built_in().resolve("PROJ-1");
        assert_eq!(resolution.changes.len(), 1);
        assert_eq!(resolution.changes[0].selector, ChangeSelector::HeadBranch);
        assert_eq!(resolution.changes[0].provider.as_str(), "github");
        assert_eq!(resolution.changes[0].branch, "PROJ-1");
    }

    #[test]
    fn project_filter_rejects_without_rescanning() {
        let selector = embedded(DEFAULT_TICKET_PATTERN, &["PROJ"]);
        assert_eq!(selector.select("XPROJ-1"), None);
        assert_eq!(
            selector
                .select("feature/OTHER-7-PROJ-123")
                .map(|t| t.lookup_key().to_string()),
            Some("PROJ-123".to_string())
        );
    }

    #[test]
    fn project_filter_is_case_insensitive() {
        let selector = embedded(DEFAULT_TICKET_PATTERN, &["proj"]);
        assert_eq!(
            selector
                .select("PROJ-5")
                .map(|t| t.lookup_key().to_string()),
            Some("PROJ-5".to_string())
        );
    }

    #[test]
    fn custom_pattern_can_name_its_capture_group() {
        let selector = embedded(r"eng-(?P<id>[0-9]+)", &[]);
        assert_eq!(
            selector
                .select("feature/eng-42-fix")
                .map(|t| t.lookup_key().to_string()),
            Some("42".to_string())
        );
    }

    #[test]
    fn an_invalid_pattern_matches_nothing() {
        let selector = embedded("([", &[]);
        assert_eq!(selector.select("PROJ-1"), None);
    }

    #[test]
    fn first_matching_rule_wins() {
        let convention = Convention {
            ticket_rules: vec![
                TicketRule {
                    provider: ProviderId::parse("linear").unwrap(),
                    selector: embedded(r"eng-(?P<id>[0-9]+)", &[]),
                    strength: Strength::Identity,
                },
                TicketRule {
                    provider: ProviderId::parse("jira").unwrap(),
                    selector: embedded(DEFAULT_TICKET_PATTERN, &[]),
                    strength: Strength::Identity,
                },
            ],
            change_rules: vec![],
        };
        let resolution = convention.resolve("eng-42");
        assert_eq!(resolution.tickets.len(), 1);
        assert_eq!(resolution.tickets[0].provider.as_str(), "linear");
        assert_eq!(resolution.changes.len(), 0);
    }

    #[test]
    fn mention_rules_do_not_become_the_primary_ticket() {
        let convention = Convention {
            ticket_rules: vec![TicketRule {
                provider: ProviderId::parse("jira").unwrap(),
                selector: embedded(DEFAULT_TICKET_PATTERN, &[]),
                strength: Strength::Mention,
            }],
            change_rules: vec![ChangeRule {
                provider: ProviderId::parse("github").unwrap(),
                selector: ChangeSelector::MentionTicket,
                strength: Strength::Mention,
            }],
        };
        let resolution = convention.resolve("feature/PROJ-1");
        assert!(resolution.primary_ticket().is_none());
        assert!(
            resolution.changes.is_empty(),
            "a mention-only ticket cannot seed a change query"
        );
    }

    #[test]
    fn a_selector_needing_a_ticket_skips_without_one() {
        let convention = Convention {
            ticket_rules: vec![],
            change_rules: vec![ChangeRule {
                provider: ProviderId::parse("github").unwrap(),
                selector: ChangeSelector::MentionTicket,
                strength: Strength::Identity,
            }],
        };
        assert!(convention.resolve("no-ticket-here").changes.is_empty());
    }

    #[test]
    fn ticket_providers_are_deduplicated_in_order() {
        let convention = Convention {
            ticket_rules: vec![
                TicketRule {
                    provider: ProviderId::parse("jira").unwrap(),
                    selector: embedded(DEFAULT_TICKET_PATTERN, &[]),
                    strength: Strength::Identity,
                },
                TicketRule {
                    provider: ProviderId::parse("jira").unwrap(),
                    selector: embedded(r"alt-(?P<id>[0-9]+)", &[]),
                    strength: Strength::Identity,
                },
            ],
            change_rules: vec![],
        };
        let resolution = convention.resolve("PROJ-1-alt-2");
        let providers = resolution.ticket_providers();
        assert_eq!(providers.len(), 1);
    }
}
