use gw_core::ProviderId;

use crate::convention_file::{Loaded, Origin};

/// Per-layout settings, resolved from configuration files and the command line.
///
/// Only values that are safe to commit belong here. Credentials stay in the
/// environment (see `providers::jira`), never in a file the layout may share.
///
/// The values are derived from [`Loaded`] rather than re-parsed: one file must
/// have exactly one schema, or the two parsers disagree about what is valid.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    /// Providers the caller asked to skip for this run.
    pub skip_providers: Vec<ProviderId>,
    /// `--skip-github`, which is the pre-provider spelling of the same idea.
    pub skip_github: bool,
}

impl Settings {
    /// Derive run settings from the merged files plus command-line overrides.
    #[must_use]
    pub fn from_loaded(
        loaded: &Loaded,
        skip_providers: Vec<ProviderId>,
        skip_github: bool,
    ) -> Self {
        let _ = loaded;
        Self {
            skip_providers,
            skip_github,
        }
    }
}

/// Read only the JIRA projects declared by a layout, ignoring the global layer.
///
/// Used by `gw repos`, which reports each repository's own configuration
/// rather than the convention that would apply to it.
pub fn declared_projects(root: &std::path::Path) -> Result<Vec<String>, crate::error::Error> {
    let loaded = crate::convention_file::load(root)?;
    // The layout file is the last source read; fall back to whatever the
    // effective convention narrowed to when it declared nothing.
    if loaded.ticket_rules_origin == Origin::Layout || !loaded.jira_project.is_empty() {
        return Ok(loaded.jira_project);
    }
    Ok(loaded
        .convention
        .ticket_rules
        .iter()
        .flat_map(|rule| match &rule.selector {
            gw_core::TicketSelector::Embedded { allow_projects, .. } => allow_projects.clone(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_are_recorded_verbatim() {
        let id = ProviderId::parse("github").unwrap();
        let settings = Settings::from_loaded(&Loaded::default(), vec![id.clone()], true);
        assert_eq!(settings.skip_providers, vec![id]);
        assert!(settings.skip_github);
    }
}
