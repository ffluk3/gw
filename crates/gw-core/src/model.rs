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
    /// branch name. Handles prefixed branches such as `feature/AX-123-thing`.
    #[must_use]
    pub fn ticket_key(&self) -> Option<TicketKey> {
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
                    return Some(TicketKey(self.0[start..k].to_string()));
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

/// A JIRA issue key such as `AX-47383`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct TicketKey(String);

impl TicketKey {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TicketKey {
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
            "AX-47383",
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
            ("AX-47383", Some("AX-47383")),
            ("feature/AX-123-thing", Some("AX-123")),
            ("agents/right-now-when", None),
            ("family/platform/1.408", None),
            ("ABC123-9-x", Some("ABC123-9")),
            ("A-1", None),
        ];
        for (input, expected) in cases {
            let got = BranchName::parse(input).unwrap().ticket_key();
            assert_eq!(
                got.as_ref().map(super::TicketKey::as_str),
                expected,
                "input={input}"
            );
        }
    }

    #[test]
    fn ticket_key_does_not_match_lowercase() {
        assert!(BranchName::parse("feature/ax-123")
            .unwrap()
            .ticket_key()
            .is_none());
    }
}
