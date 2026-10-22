//! Which providers are active, and why the inactive ones are not.
//!
//! The registry is built once from resolved settings. Commands ask it for a
//! capability and never construct a provider themselves.

use std::collections::BTreeSet;

use gw_core::ProviderId;

use crate::providers::{ChangeProvider, TicketProvider, Unavailable};
use crate::settings::Settings;

/// The providers available for one run.
pub struct Registry {
    tickets: Vec<Box<dyn TicketProvider>>,
    changes: Vec<Box<dyn ChangeProvider>>,
    unavailable: Vec<Unavailable>,
    skipped: BTreeSet<ProviderId>,
}

impl Registry {
    /// Build the active set from settings and the environment.
    ///
    /// Providers activate only when they can actually work: a tracker with no
    /// base URL or credentials is inactive, not failing. Every inactive
    /// provider records why, so `gw convention show` can explain it.
    #[must_use]
    pub fn build(settings: &Settings) -> Self {
        let mut registry = Self {
            tickets: Vec::new(),
            changes: Vec::new(),
            unavailable: Vec::new(),
            skipped: settings.skip_providers.iter().cloned().collect(),
        };

        registry.add_jira(settings);
        registry.add_github(settings);

        registry
    }

    #[must_use]
    pub fn ticket_provider(&self, id: &ProviderId) -> Option<&dyn TicketProvider> {
        self.tickets
            .iter()
            .find(|p| p.id() == id)
            .map(AsRef::as_ref)
    }

    #[must_use]
    pub fn change_provider(&self, id: &ProviderId) -> Option<&dyn ChangeProvider> {
        self.changes
            .iter()
            .find(|p| p.id() == id)
            .map(AsRef::as_ref)
    }

    /// Why a provider is not active, when it was explicitly excluded rather
    /// than simply absent from the configuration.
    #[must_use]
    pub fn inactive_reason(&self, id: &ProviderId) -> Option<&str> {
        self.unavailable
            .iter()
            .find(|u| &u.id == id)
            .map(|u| u.reason.as_str())
    }

    fn add_jira(&mut self, settings: &Settings) {
        let id = ProviderId::parse("jira").expect("literal is a valid provider id");
        if self.skipped.contains(&id) {
            return;
        }
        match crate::providers::jira::JiraRest::from_settings(settings) {
            Some(jira) => self.tickets.push(Box::new(jira)),
            None => self.unavailable.push(Unavailable {
                id,
                reason: "set a base URL in .gw.toml and GW_JIRA_USER / GW_JIRA_API_TOKEN".into(),
            }),
        }
    }

    fn add_github(&mut self, settings: &Settings) {
        let id = ProviderId::parse("github").expect("literal is a valid provider id");
        if self.skipped.contains(&id) {
            return;
        }
        if settings.skip_github {
            self.unavailable.push(Unavailable {
                id,
                reason: "--skip-github".into(),
            });
            return;
        }
        self.changes
            .push(Box::new(crate::providers::github::GhCli::new()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jira_without_configuration_is_inactive_and_says_why() {
        let registry = Registry::build(&Settings::default());
        let jira = ProviderId::parse("jira").unwrap();
        assert!(registry.ticket_provider(&jira).is_none());
        assert!(registry
            .inactive_reason(&jira)
            .is_some_and(|reason| reason.contains("base URL")));
    }

    #[test]
    fn skip_github_removes_the_provider_without_claiming_it_is_broken() {
        let settings = Settings {
            skip_github: true,
            ..Settings::default()
        };
        let github = ProviderId::parse("github").unwrap();
        let registry = Registry::build(&settings);
        assert!(registry.change_provider(&github).is_none());
        assert_eq!(registry.inactive_reason(&github), Some("--skip-github"));
    }
}
