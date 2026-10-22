use std::fmt;

use serde::Serialize;

/// A validation failure at the boundary where raw git output becomes a typed
/// value. Anything that fails here never reaches the classifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelError(pub &'static str);

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for ModelError {}

/// A commit object id. Kept as a string rather than a fixed-width byte array
/// because SHA-256 repositories produce 64 hex characters.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Sha(String);

impl Sha {
    pub fn parse(raw: &str) -> Result<Self, ModelError> {
        let trimmed = raw.trim();
        if trimmed.len() < 4 || trimmed.len() > 64 {
            return Err(ModelError("commit id has an implausible length"));
        }
        if !trimmed.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ModelError("commit id is not hexadecimal"));
        }
        Ok(Sha(trimmed.to_ascii_lowercase()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn short(&self, width: usize) -> &str {
        &self.0[..width.min(self.0.len())]
    }
}

impl fmt::Display for Sha {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A branch name that is safe to use as a git ref argument, a path component,
/// and a GraphQL variable value.
///
/// The character set is stricter than git's so that downstream consumers never
/// need to escape: quotes and backslashes are rejected even though git permits
/// them in ref names.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct BranchName(String);

impl BranchName {
    pub fn parse(raw: &str) -> Result<Self, ModelError> {
        let candidate = raw.trim();
        if candidate.is_empty() {
            return Err(ModelError("branch name is empty"));
        }
        if candidate.starts_with('-') {
            return Err(ModelError("branch name starts with '-'"));
        }
        if candidate.contains("..") || candidate.contains("//") {
            return Err(ModelError("branch name contains an invalid ref sequence"));
        }
        // Git reserves `.lock` case-insensitively, so reject `x.LOCK` too.
        // `lower` is already lowercase, so the comparison is exact.
        #[allow(clippy::case_sensitive_file_extension_comparisons)]
        let has_lock_suffix = candidate.to_ascii_lowercase().ends_with(".lock");
        if candidate.ends_with('/') || has_lock_suffix || candidate.ends_with('.') {
            return Err(ModelError("branch name has an invalid suffix"));
        }
        let forbidden = |c: char| {
            c.is_control()
                || matches!(
                    c,
                    ' ' | '~'
                        | '^'
                        | ':'
                        | '?'
                        | '*'
                        | '['
                        | '\\'
                        | '"'
                        | '\''
                        | '{'
                        | '}'
                        | '('
                        | ')'
                        | '@'
                )
        };
        if candidate.chars().any(forbidden) {
            return Err(ModelError("branch name contains a reserved character"));
        }
        Ok(BranchName(candidate.to_string()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Extracts the first JIRA-shaped key (`[A-Z][A-Z0-9]+-[0-9]+`) from the
    /// branch name. Handles prefixed branches such as `feature/PROJ-123-thing`.
    #[must_use]
    pub fn ticket_key(&self) -> Option<TicketId> {
        self.find_ticket(|_| true)
    }

    /// Extracts the first key whose project prefix is in `projects`.
    ///
    /// An empty list means "any project", which is the same as
    /// [`Self::ticket_key`]. Matching is case-insensitive because JIRA renders
    /// project keys in upper case but users type them in either case.
    #[must_use]
    pub fn ticket_key_for(&self, projects: &[String]) -> Option<TicketId> {
        if projects.is_empty() {
            return self.ticket_key();
        }
        self.find_ticket(|key| {
            let prefix = key.split_once('-').map_or(key, |(prefix, _)| prefix);
            projects.iter().any(|p| p.eq_ignore_ascii_case(prefix))
        })
    }

    /// The shared scanner. `accept` decides whether a well-shaped candidate is
    /// the key we want; a rejected candidate is skipped without rescanning its
    /// interior, so `XPROJ-1` can never satisfy a filter for `PROJ`.
    fn find_ticket(&self, accept: impl Fn(&str) -> bool) -> Option<TicketId> {
        let bytes = self.0.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if !bytes[i].is_ascii_uppercase() {
                i += 1;
                continue;
            }
            let start = i;
            let mut j = i + 1;
            while j < bytes.len() && (bytes[j].is_ascii_uppercase() || bytes[j].is_ascii_digit()) {
                j += 1;
            }
            // Require at least two characters before the dash (`[A-Z][A-Z0-9]+`).
            if j > start + 1 && j < bytes.len() && bytes[j] == b'-' {
                let digit_start = j + 1;
                let mut k = digit_start;
                while k < bytes.len() && bytes[k].is_ascii_digit() {
                    k += 1;
                }
                if k > digit_start {
                    let candidate = &self.0[start..k];
                    if accept(candidate) {
                        return Some(TicketId(candidate.to_string()));
                    }
                    i = k.max(i + 1);
                    continue;
                }
            }
            i = j.max(i + 1);
        }
        None
    }
}

impl fmt::Display for BranchName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A ticket identifier in whatever shape the convention produces, such as a
/// JIRA key `PROJ-47383` or a Linear identifier `ENG-42`.
///
/// The type deliberately does not validate the shape: a convention decides
/// what an identifier looks like, and a provider decides what it means. It only
/// guarantees that whatever reached it came from a matched rule.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct TicketId(String);

impl TicketId {
    pub fn parse(raw: &str) -> Result<Self, ModelError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(ModelError("ticket id is empty"));
        }
        if trimmed.len() > 128 {
            return Err(ModelError("ticket id is implausibly long"));
        }
        if trimmed.chars().any(|c| c.is_control() || c.is_whitespace()) {
            return Err(ModelError(
                "ticket id contains whitespace or control characters",
            ));
        }
        Ok(Self(trimmed.to_string()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The provider-scoped lookup key: the value itself.
    #[must_use]
    pub fn lookup_key(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TicketId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha_accepts_sha1_and_sha256() {
        assert_eq!(Sha::parse("9df03e38ae0").unwrap().as_str(), "9df03e38ae0");
        assert!(Sha::parse(&"a".repeat(64)).is_ok());
        assert!(Sha::parse("").is_err());
        assert!(Sha::parse("zzzz").is_err());
    }

    #[test]
    fn sha_is_case_insensitive() {
        assert_eq!(Sha::parse("ABCD1234").unwrap().as_str(), "abcd1234");
    }

    #[test]
    fn branch_rejects_graphql_metacharacters() {
        assert!(BranchName::parse("feature/a\"b").is_err());
        assert!(BranchName::parse("feature/a\\b").is_err());
        assert!(BranchName::parse("-danger").is_err());
        assert!(BranchName::parse("a..b").is_err());
        assert!(BranchName::parse("").is_err());
    }

    #[test]
    fn branch_accepts_real_shapes() {
        for name in [
            "PROJ-47383",
            "feature/nodejs-20-migration",
            "agents/right-now-when-i-run-asdf-ensure-versions",
            "family/platform/1.408",
            "copilot/shrill-rook",
        ] {
            assert!(BranchName::parse(name).is_ok(), "rejected {name}");
        }
    }

    #[test]
    fn ticket_key_extraction() {
        let cases = [
            ("PROJ-47383", Some("PROJ-47383")),
            ("feature/PROJ-123-thing", Some("PROJ-123")),
            ("agents/right-now-when", None),
            ("family/platform/1.408", None),
            ("ABC123-9-x", Some("ABC123-9")),
            ("A-1", None),
        ];
        for (input, expected) in cases {
            let got = BranchName::parse(input).unwrap().ticket_key();
            assert_eq!(
                got.as_ref().map(super::TicketId::as_str),
                expected,
                "input={input}"
            );
        }
    }

    #[test]
    fn ticket_key_for_filters_by_project() {
        let branch = BranchName::parse("feature/OTHER-7-PROJ-123").unwrap();
        let proj = vec!["PROJ".to_string()];
        assert_eq!(
            branch.ticket_key_for(&proj).as_ref().map(TicketId::as_str),
            Some("PROJ-123")
        );
        // Empty means unrestricted, preserving the no-config behavior.
        assert_eq!(
            branch.ticket_key_for(&[]).as_ref().map(TicketId::as_str),
            Some("OTHER-7")
        );
    }

    #[test]
    fn ticket_key_for_is_case_insensitive_and_multi_project() {
        let branch = BranchName::parse("PROJ-5").unwrap();
        assert!(branch.ticket_key_for(&["proj".to_string()]).is_some());
        assert!(branch
            .ticket_key_for(&["OPS".to_string(), "PROJ".to_string()])
            .is_some());
        assert!(branch.ticket_key_for(&["OPS".to_string()]).is_none());
    }

    #[test]
    fn rejected_candidate_interior_is_not_rescanned() {
        // `XPROJ-1` must not satisfy a filter for `PROJ`.
        let branch = BranchName::parse("XPROJ-1").unwrap();
        assert!(branch.ticket_key_for(&["PROJ".to_string()]).is_none());
    }

    #[test]
    fn ticket_key_does_not_match_lowercase() {
        assert!(BranchName::parse("feature/proj-123")
            .unwrap()
            .ticket_key()
            .is_none());
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::*;

    #[test]
    fn sha_short_is_bounded_by_the_value_and_the_width() {
        let sha = Sha::parse("9df03e38ae0").unwrap();
        assert_eq!(sha.short(4), "9df0");
        assert_eq!(sha.short(100), "9df03e38ae0");
    }

    #[test]
    fn sha_display_is_the_value() {
        assert_eq!(Sha::parse("ABCD").unwrap().to_string(), "abcd");
    }

    #[test]
    fn sha_rejects_bad_shapes() {
        assert!(Sha::parse("abc").is_err(), "too short");
        assert!(Sha::parse(&"a".repeat(65)).is_err(), "too long");
        assert!(Sha::parse("zzzz").is_err(), "not hexadecimal");
    }

    #[test]
    fn branch_name_rejects_bad_shapes() {
        for bad in [
            "",
            "-x",
            "a..b",
            "a//b",
            "trailing/",
            "x.lock",
            "x.LOCK",
            "trailing.",
            "has space",
            "a~b",
            "a^b",
            "a:b",
            "a?b",
            "a*b",
            "a[b",
            "a\\b",
            "a\"b",
            "a'b",
            "a{b",
            "a}b",
            "a(b",
            "a)b",
            "a@b",
            "a\u{7}b",
        ] {
            assert!(BranchName::parse(bad).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn branch_name_accepts_slashes_and_dots() {
        assert_eq!(
            BranchName::parse(" feature/x.y ").unwrap().as_str(),
            "feature/x.y"
        );
        assert_eq!(BranchName::parse("main").unwrap().as_str(), "main");
    }

    #[test]
    fn branch_name_display_is_the_value() {
        assert_eq!(BranchName::parse("PROJ-1").unwrap().to_string(), "PROJ-1");
    }

    #[test]
    fn ticket_ids_reject_bad_shapes() {
        assert!(TicketId::parse("").is_err());
        assert!(TicketId::parse("  ").is_err());
        assert!(TicketId::parse(&"x".repeat(129)).is_err());
        assert!(TicketId::parse("has space").is_err());
        assert!(TicketId::parse("has\u{7}bell").is_err());
    }

    #[test]
    fn ticket_id_keeps_its_value_and_lookup_key() {
        let id = TicketId::parse(" PROJ-1 ").unwrap();
        assert_eq!(id.as_str(), "PROJ-1");
        assert_eq!(id.lookup_key(), "PROJ-1");
        assert_eq!(id.to_string(), "PROJ-1");
    }

    #[test]
    fn a_lone_prefix_never_becomes_a_ticket() {
        // No digits after the dash, and a prefix too short to satisfy the rule.
        assert!(BranchName::parse("PROJ-").unwrap().ticket_key().is_none());
        assert!(BranchName::parse("X-1").unwrap().ticket_key().is_none());
    }
}
