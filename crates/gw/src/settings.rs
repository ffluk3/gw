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
