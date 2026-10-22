//! Which providers are active, and why the inactive ones are not.
//!
//! The registry is built once from resolved settings. Commands ask it for a
//! capability and never construct a provider themselves.

use std::collections::BTreeSet;

use gw_core::ProviderId;

use crate::providers::command::{Capability, CommandProvider};
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
    pub fn build(settings: &Settings, loaded: &crate::convention_file::Loaded) -> Self {
        let mut registry = Self {
            tickets: Vec::new(),
            changes: Vec::new(),
            unavailable: Vec::new(),
            skipped: settings.skip_providers.iter().cloned().collect(),
        };

        registry.add_jira(loaded);
        registry.add_github(settings);
        registry.add_commands(loaded);

        registry
    }

    /// Add every `kind = "command"` provider.
    ///
    /// A command provider is added to whichever capability its rules name, so
    /// one command can back tickets, changes, or both.
    fn add_commands(&mut self, loaded: &crate::convention_file::Loaded) {
        for config in &loaded.providers {
            if config.kind != "command" {
                continue;
            }
            if self.skipped.contains(&config.id) {
                continue;
            }
            let Some(argv) = config.command.clone().filter(|argv| !argv.is_empty()) else {
                self.unavailable.push(Unavailable {
                    id: config.id.clone(),
                    reason: "kind = \"command\" needs a command".into(),
                });
                continue;
            };

            let backs_tickets = loaded
                .convention
                .ticket_rules
                .iter()
                .any(|rule| rule.provider == config.id);
            let backs_changes = loaded
                .convention
                .change_rules
                .iter()
                .any(|rule| rule.provider == config.id);

            if backs_tickets {
                self.tickets.push(Box::new(CommandProvider::new(
                    config.id.clone(),
                    argv.clone(),
                    Capability::Ticket,
                )));
            }
            if backs_changes {
                self.changes.push(Box::new(CommandProvider::new(
                    config.id.clone(),
                    argv,
                    Capability::Change,
                )));
            }
            if !backs_tickets && !backs_changes {
                self.unavailable.push(Unavailable {
                    id: config.id.clone(),
                    reason: "no convention rule names this provider".into(),
                });
            }
        }
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

    fn add_jira(&mut self, loaded: &crate::convention_file::Loaded) {
        let id = ProviderId::parse("jira").expect("literal is a valid provider id");
        if self.skipped.contains(&id) {
            return;
        }
        match crate::providers::jira::JiraRest::from_config(loaded) {
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
        let registry = Registry::build(&Settings::default(), &Default::default());
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
        let registry = Registry::build(&settings, &Default::default());
        assert!(registry.change_provider(&github).is_none());
        assert_eq!(registry.inactive_reason(&github), Some("--skip-github"));
    }
}
