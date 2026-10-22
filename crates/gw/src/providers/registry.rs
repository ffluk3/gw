//! Which providers are active, and why the inactive ones are not.
//!
//! The registry is built once from resolved settings. Commands ask it for a
//! capability and never construct a provider themselves.

use std::collections::BTreeSet;

use gw_core::ProviderId;

use crate::providers::command::{Capability, CommandProvider};
use crate::providers::{ChangeProvider, TicketProvider, Unavailable};

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
    pub fn build(loaded: &crate::convention_file::Loaded, skipped: Vec<ProviderId>) -> Self {
        let mut registry = Self {
            tickets: Vec::new(),
            changes: Vec::new(),
            unavailable: Vec::new(),
            skipped: skipped.into_iter().collect(),
        };

        registry.add_jira(loaded);
        registry.add_github();
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
                reason: "set [convention.providers.jira] base_url and the \
                         JIRA credentials in the environment"
                    .to_string(),
            }),
        }
    }

    fn add_github(&mut self) {
        let id = ProviderId::parse("github").expect("literal is a valid provider id");
        if self.skipped.contains(&id) {
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
        let registry = Registry::build(&Default::default(), Vec::new());
        let jira = ProviderId::parse("jira").unwrap();
        assert!(registry.ticket_provider(&jira).is_none());
        assert!(
            registry
                .inactive_reason(&jira)
                .is_some_and(|reason| reason.contains("base_url")),
            "the reason should name the setting that is missing"
        );
    }

    #[test]
    fn a_skipped_provider_is_absent_and_says_why() {
        let github = ProviderId::parse("github").unwrap();
        let registry = Registry::build(&Default::default(), vec![github.clone()]);
        assert!(registry.change_provider(&github).is_none());
        assert!(registry.inactive_reason(&github).is_none());
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::*;
    use crate::convention_file::{Loaded, ProviderConfig};

    fn loaded_with(provider: ProviderConfig, rule_provider: &str) -> Loaded {
        let id = ProviderId::parse(rule_provider).unwrap();
        Loaded {
            convention: gw_core::Convention {
                ticket_rules: vec![gw_core::TicketRule {
                    provider: id.clone(),
                    selector: gw_core::TicketSelector::Embedded {
                        pattern: "x-(?P<id>[0-9]+)".into(),
                        allow_projects: vec![],
                    },
                    strength: gw_core::Strength::Identity,
                }],
                change_rules: vec![gw_core::ChangeRule {
                    provider: id,
                    selector: gw_core::ChangeSelector::HeadBranch,
                    strength: gw_core::Strength::Identity,
                }],
            },
            providers: vec![provider],
            ..Loaded::default()
        }
    }

    fn command_provider(id: &str, command: Option<Vec<String>>) -> ProviderConfig {
        ProviderConfig {
            id: ProviderId::parse(id).unwrap(),
            kind: "command".into(),
            base_url: None,
            done_status_category: None,
            command,
        }
    }

    #[test]
    fn a_command_provider_backs_whichever_rules_name_it() {
        let loaded = loaded_with(command_provider("host", Some(vec!["tool".into()])), "host");
        let registry = Registry::build(&loaded, Vec::new());
        let id = ProviderId::parse("host").unwrap();
        assert!(registry.ticket_provider(&id).is_some());
        assert!(registry.change_provider(&id).is_some());
        assert!(registry.inactive_reason(&id).is_none());
    }

    #[test]
    fn a_command_without_argv_is_inactive_and_says_why() {
        let loaded = loaded_with(command_provider("host", Some(vec![])), "host");
        let registry = Registry::build(&loaded, Vec::new());
        let id = ProviderId::parse("host").unwrap();
        assert!(registry.ticket_provider(&id).is_none());
        assert!(registry
            .inactive_reason(&id)
            .is_some_and(|reason| reason.contains("command")));
    }

    #[test]
    fn a_command_named_by_no_rule_is_inactive_and_says_why() {
        let loaded = loaded_with(
            command_provider("orphan", Some(vec!["tool".into()])),
            "host",
        );
        let registry = Registry::build(&loaded, Vec::new());
        let id = ProviderId::parse("orphan").unwrap();
        assert!(registry.ticket_provider(&id).is_none());
        assert!(registry
            .inactive_reason(&id)
            .is_some_and(|reason| reason.contains("no convention rule")));
    }

    #[test]
    fn a_skipped_command_provider_is_absent() {
        let loaded = loaded_with(command_provider("host", Some(vec!["tool".into()])), "host");
        let id = ProviderId::parse("host").unwrap();
        let registry = Registry::build(&loaded, vec![id.clone()]);
        assert!(registry.ticket_provider(&id).is_none());
        assert!(registry.change_provider(&id).is_none());
    }

    #[test]
    fn a_non_command_kind_is_not_added_as_a_command_provider() {
        let mut config = command_provider("host", Some(vec!["tool".into()]));
        config.kind = "jira".into();
        let loaded = loaded_with(config, "host");
        let registry = Registry::build(&loaded, Vec::new());
        let id = ProviderId::parse("host").unwrap();
        assert!(registry.ticket_provider(&id).is_none());
    }

    #[test]
    fn github_is_active_unless_skipped() {
        let github = ProviderId::parse("github").unwrap();
        let active = Registry::build(&Default::default(), Vec::new());
        assert!(active.change_provider(&github).is_some());
    }

    #[test]
    fn a_configured_jira_adapter_requires_credentials() {
        // Without the environment variables, even a base URL leaves it
        // inactive: an adapter that cannot authenticate must not be active.
        let _lock = crate::providers::test_support::credential_lock();
        std::env::remove_var("GW_JIRA_USER");
        std::env::remove_var("GW_JIRA_API_TOKEN");
        let loaded = Loaded {
            providers: vec![ProviderConfig {
                id: ProviderId::parse("jira").unwrap(),
                kind: "jira".into(),
                base_url: Some("https://example.atlassian.net".into()),
                done_status_category: None,
                command: None,
            }],
            ..Loaded::default()
        };
        let registry = Registry::build(&loaded, Vec::new());
        assert!(registry
            .ticket_provider(&ProviderId::parse("jira").unwrap())
            .is_none());
    }
}
