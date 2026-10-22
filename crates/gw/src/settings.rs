//! Reporting helpers for `gw repos`.
//!
//! `gw repos` answers a different question from every other command: not "what
//! convention applies here" but "what does this repository declare". It reads
//! the layout file alone, with no global layer and no inheritance.

/// Ticket provider names a layout's own convention references.
///
/// Used by `gw repos`, which reports each repository's own configuration
/// rather than the convention that would apply to it.
pub fn declared_ticket_providers(
    root: &std::path::Path,
) -> Result<Vec<String>, crate::error::Error> {
    let loaded = crate::convention_file::load_layout_only(root)?;
    let mut seen = std::collections::BTreeSet::new();
    Ok(loaded
        .convention
        .ticket_rules
        .iter()
        .filter(|rule| seen.insert(rule.provider.clone()))
        .map(|rule| rule.provider.to_string())
        .collect())
}

/// Ticket project filters a layout's own convention declares.
pub fn declared_projects(root: &std::path::Path) -> Result<Vec<String>, crate::error::Error> {
    let loaded = crate::convention_file::load_layout_only(root)?;
    let mut seen = std::collections::BTreeSet::new();
    Ok(loaded
        .convention
        .ticket_rules
        .iter()
        .flat_map(|rule| match &rule.selector {
            gw_core::TicketSelector::Embedded { allow_projects, .. } => allow_projects.clone(),
        })
        .filter(|project| seen.insert(project.to_ascii_uppercase()))
        .collect())
}

#[cfg(test)]
mod coverage_tests {
    use super::*;

    fn layout_with(config: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        if !config.is_empty() {
            std::fs::write(dir.path().join(crate::convention_file::LAYOUT_FILE), config).unwrap();
        }
        dir
    }

    #[test]
    fn a_layout_without_configuration_declares_the_built_in_provider() {
        let dir = layout_with("");
        assert_eq!(
            declared_ticket_providers(dir.path()).unwrap(),
            vec!["jira".to_string()]
        );
        // The built-in rule has no project filter.
        assert!(declared_projects(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn declared_providers_are_deduplicated_and_ordered() {
        let dir = layout_with(
            r#"
[[convention.ticket_rules]]
provider = "linear"
selector = "embedded"
pattern = 'eng-(?P<id>[0-9]+)'
allow_projects = ["eng"]

[[convention.ticket_rules]]
provider = "linear"
selector = "embedded"
pattern = 'ops-(?P<id>[0-9]+)'
allow_projects = ["ops", "ENG"]

[[convention.ticket_rules]]
provider = "jira"
selector = "embedded"
pattern = '[A-Z]+-[0-9]+'
"#,
        );
        assert_eq!(
            declared_ticket_providers(dir.path()).unwrap(),
            vec!["linear".to_string(), "jira".to_string()]
        );
        // Projects compare case-insensitively and keep their first spelling,
        // so the second `ENG` is a duplicate of `eng`.
        assert_eq!(
            declared_projects(dir.path()).unwrap(),
            vec!["eng".to_string(), "ops".to_string()]
        );
    }
}
