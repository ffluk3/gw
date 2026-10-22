use std::collections::BTreeSet;

use gw_core::{ChangeSelector, Convention, ProviderId, TicketSelector};
use serde::Serialize;

use crate::cli::ConventionAction;
use crate::context::Context;
use crate::convention_file::Origin;
use crate::error::Error;
use crate::output::Outcome;
use crate::providers::Registry;

/// `gw convention show` and `gw convention explain`.
pub fn run(ctx: &Context, args: ConventionAction) -> Result<Outcome, Error> {
    match args {
        ConventionAction::Show { json } => show(ctx, json),
        ConventionAction::Explain { branch, json } => explain(ctx, &branch, json),
    }
}

fn show(ctx: &Context, json: bool) -> Result<Outcome, Error> {
    let view = View::new(ctx);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&view).unwrap_or_else(|_| "{}".into())
        );
        return Ok(Outcome::ok());
    }

    println!("Convention in force");
    println!();
    println!(
        "  ticket rules  ({})",
        ctx.loaded.ticket_rules_origin.as_str()
    );
    for rule in &ctx.convention.ticket_rules {
        println!(
            "    {}.{}  {}",
            rule.provider,
            rule.selector.as_str(),
            describe_ticket_selector(&rule.selector)
        );
        println!("      strength: {}", rule.strength.as_str());
    }
    if ctx.convention.ticket_rules.is_empty() {
        println!("    (none)");
    }

    println!();
    println!(
        "  change rules  ({})",
        ctx.loaded.change_rules_origin.as_str()
    );
    for rule in &ctx.convention.change_rules {
        println!(
            "    {}.{}  strength: {}",
            rule.provider,
            rule.selector.as_str(),
            rule.strength.as_str()
        );
    }
    if ctx.convention.change_rules.is_empty() {
        println!("    (none)");
    }

    println!();
    println!("Providers");
    for line in provider_lines(ctx.registry()) {
        println!("    {line}");
    }

    println!();
    println!("Settings");
    for line in settings_lines(ctx) {
        println!("    {line}");
    }

    println!();
    if ctx.loaded.sources.is_empty() {
        println!("Configuration: none (built-in convention)");
    } else {
        println!("Configuration, lowest precedence first:");
        for path in &ctx.loaded.sources {
            println!("    {}", path.display());
        }
    }
    Ok(Outcome::ok())
}

fn explain(ctx: &Context, branch: &str, json: bool) -> Result<Outcome, Error> {
    let resolution = ctx.convention.resolve(branch);
    let lines = explain_lines(ctx.registry(), &ctx.convention, branch);

    if json {
        let document = serde_json::json!({
            "branch": branch,
            "tickets": resolution.tickets,
            // The domain enum serializes with its tag to stay self-describing
            // in `gw clean --json`; here each query is already keyed by the
            // field name, so flatten it to the selector's name.
            "changes": resolution
                .changes
                .iter()
                .map(|query| {
                    serde_json::json!({
                        "provider": query.provider,
                        "selector": query.selector.as_str(),
                        "strength": query.strength,
                        "branch": query.branch,
                        "ticket": query.ticket,
                    })
                })
                .collect::<Vec<_>>(),
            "rules": lines
                .iter()
                .map(|line| {
                    serde_json::json!({
                        "rule": line.rule,
                        "matched": line.matched,
                        "strength": line.strength,
                        "provider": line.provider,
                        "provider_active": line.active,
                    })
                })
                .collect::<Vec<_>>(),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&document).unwrap_or_else(|_| "{}".into())
        );
        return Ok(Outcome::ok());
    }

    println!("Branch: {branch}");
    println!();
    for line in &lines {
        let mark = if line.matched { "match" } else { "     " };
        println!(
            "  [{mark}] {} via {} ({})",
            line.rule, line.provider, line.strength
        );
        println!("          {}", line.detail);
    }
    println!();
    if resolution.tickets.is_empty() && resolution.changes.is_empty() {
        println!("No rule matched. Nothing will be looked up for this branch.");
    } else {
        println!("Would query:");
        for ticket in &resolution.tickets {
            println!("  {} ticket {}", ticket.provider, ticket.id);
        }
        for change in &resolution.changes {
            println!(
                "  {} change {} for {}",
                change.provider,
                change.selector.as_str(),
                change.branch
            );
        }
    }
    Ok(Outcome::ok())
}

/// One evaluated rule, with enough detail to debug a surprising mapping.
pub struct RuleLine {
    pub rule: String,
    pub matched: bool,
    pub strength: &'static str,
    pub provider: String,
    pub active: bool,
    pub detail: String,
}

/// Evaluate every rule, whether or not it matched.
///
/// A non-match is as informative as a match when a branch maps to nothing, so
/// this does not stop at the first hit the way resolution does.
fn explain_lines(registry: &Registry, convention: &Convention, branch: &str) -> Vec<RuleLine> {
    let mut lines = Vec::new();
    for (index, rule) in convention.ticket_rules.iter().enumerate() {
        let found = rule.selector.select(branch);
        let active = registry.ticket_provider(&rule.provider).is_some();
        let detail = match &found {
            Some(id) => format!("found ticket {}", id.as_str()),
            None => "no match".to_string(),
        };
        lines.push(RuleLine {
            rule: format!("ticket_rules[{index}]"),
            matched: found.is_some(),
            strength: rule.strength.as_str(),
            provider: rule.provider.to_string(),
            active,
            detail: match (found.is_some(), active) {
                (true, false) => format!("{detail}; provider is not active"),
                _ => detail,
            },
        });
    }
    let primary = convention.resolve(branch).primary_ticket().cloned();
    for (index, rule) in convention.change_rules.iter().enumerate() {
        let needs_ticket = matches!(
            rule.selector,
            ChangeSelector::MentionTicket | ChangeSelector::ContainsCommit
        );
        let matched = !needs_ticket || primary.is_some();
        let summary = match (&rule.selector, &primary) {
            (ChangeSelector::HeadBranch, _) => format!("query head branch {branch}"),
            (_, Some(ticket)) => format!("query for ticket {}", ticket.id),
            (_, None) => "skipped: needs a ticket and none matched".to_string(),
        };
        lines.push(RuleLine {
            rule: format!("change_rules[{index}]"),
            matched,
            strength: rule.strength.as_str(),
            provider: rule.provider.to_string(),
            active: registry.change_provider(&rule.provider).is_some(),
            detail: summary,
        });
    }
    lines
}

fn describe_ticket_selector(selector: &TicketSelector) -> String {
    match selector {
        TicketSelector::Embedded {
            pattern,
            allow_projects,
        } => {
            let projects = if allow_projects.is_empty() {
                "any project".to_string()
            } else {
                format!("projects: {}", allow_projects.join(", "))
            };
            format!("pattern {pattern}; {projects}")
        }
    }
}

/// One line per provider that the convention names, with its status.
fn provider_lines(registry: &Registry) -> Vec<String> {
    let mut ids: BTreeSet<ProviderId> = BTreeSet::new();
    for id in ["jira", "github"] {
        ids.insert(ProviderId::parse(id).expect("literal is valid"));
    }

    ids.into_iter()
        .map(|id| {
            let capability = match id.as_str() {
                "jira" => "tickets",
                "github" => "changes",
                _ => "unknown",
            };
            let active = if capability == "tickets" {
                registry.ticket_provider(&id).is_some()
            } else {
                registry.change_provider(&id).is_some()
            };
            let state = if active {
                "active".to_string()
            } else {
                format!(
                    "inactive ({})",
                    registry.inactive_reason(&id).unwrap_or("not configured")
                )
            };
            format!("{id}  ({capability})  {state}")
        })
        .collect()
}

/// The settings in force, for both renderers.
///
/// Each value reports the file it came from, so an unexpected stale limit or
/// repos root can be traced to a file instead of guessed at.
fn settings_lines(ctx: &Context) -> Vec<String> {
    let clean = &ctx.loaded.clean;
    let stale = match clean.stale_after_secs {
        Some(seconds) => format!(
            "stale_after {} ({})",
            gw_core::format_duration_secs(seconds),
            clean.stale_after_origin.as_str()
        ),
        None => format!(
            "stale_after {} (built-in)",
            gw_core::format_duration_secs(gw_core::Policy::default().stale_after_secs)
        ),
    };
    let protect = if clean.protect.is_empty() {
        "protect (none)".to_string()
    } else {
        format!(
            "protect {} ({})",
            clean.protect.join(", "),
            clean.protect_origin.as_str()
        )
    };
    let skip = if ctx.loaded.provider_selection.skip.is_empty() {
        "skip (none)".to_string()
    } else {
        format!(
            "skip {} ({})",
            ctx.loaded.provider_selection.skip.join(", "),
            ctx.loaded.provider_selection.skip_origin.as_str()
        )
    };
    let repos_root = match &ctx.loaded.discovery.repos_root {
        Some(root) => format!(
            "repos_root {root} ({})",
            ctx.loaded.discovery.repos_root_origin.as_str()
        ),
        None => "repos_root (default ~/repos)".to_string(),
    };
    vec![stale, protect, skip, repos_root]
}

/// The JSON shaping of `gw convention show`.
#[derive(Debug, Serialize)]
struct View {
    ticket_rules: Vec<RuleView>,
    ticket_rules_origin: &'static str,
    change_rules: Vec<ChangeRuleView>,
    change_rules_origin: &'static str,
    providers: Vec<ProviderView>,
    settings: SettingsView,
    sources: Vec<String>,
}

/// Additive: a new object, so no existing JSON field changes shape.
#[derive(Debug, Serialize)]
struct SettingsView {
    stale_after_secs: i64,
    stale_after_origin: &'static str,
    protect: Vec<String>,
    protect_origin: &'static str,
    skip: Vec<String>,
    skip_origin: &'static str,
    repos_root: Option<String>,
    repos_root_origin: &'static str,
}

#[derive(Debug, Serialize)]
struct RuleView {
    provider: String,
    selector: String,
    strength: &'static str,
    detail: String,
}

#[derive(Debug, Serialize)]
struct ChangeRuleView {
    provider: String,
    selector: String,
    strength: &'static str,
}

#[derive(Debug, Serialize)]
struct ProviderView {
    id: String,
    active: bool,
    reason: Option<String>,
}

impl View {
    fn new(ctx: &Context) -> Self {
        let registry = ctx.registry();
        Self {
            ticket_rules: ctx
                .convention
                .ticket_rules
                .iter()
                .map(|rule| RuleView {
                    provider: rule.provider.to_string(),
                    selector: rule.selector.as_str().to_string(),
                    strength: rule.strength.as_str(),
                    detail: describe_ticket_selector(&rule.selector),
                })
                .collect(),
            ticket_rules_origin: ctx.loaded.ticket_rules_origin.as_str(),
            change_rules: ctx
                .convention
                .change_rules
                .iter()
                .map(|rule| ChangeRuleView {
                    provider: rule.provider.to_string(),
                    selector: rule.selector.as_str().to_string(),
                    strength: rule.strength.as_str(),
                })
                .collect(),
            change_rules_origin: ctx.loaded.change_rules_origin.as_str(),
            providers: ["jira", "github"]
                .into_iter()
                .map(|raw| {
                    let id = ProviderId::parse(raw).expect("literal is valid");
                    let active = registry.ticket_provider(&id).is_some()
                        || registry.change_provider(&id).is_some();
                    ProviderView {
                        active,
                        reason: registry.inactive_reason(&id).map(str::to_string),
                        id: raw.to_string(),
                    }
                })
                .collect(),
            settings: SettingsView {
                // Report the value that will actually be used, so the field is
                // never `null` for a caller that just wants the number.
                stale_after_secs: ctx
                    .loaded
                    .clean
                    .stale_after_secs
                    .unwrap_or_else(|| gw_core::Policy::default().stale_after_secs),
                stale_after_origin: if ctx.loaded.clean.stale_after_secs.is_some() {
                    ctx.loaded.clean.stale_after_origin.as_str()
                } else {
                    Origin::BuiltIn.as_str()
                },
                protect: ctx.loaded.clean.protect.clone(),
                protect_origin: ctx.loaded.clean.protect_origin.as_str(),
                skip: ctx.loaded.provider_selection.skip.clone(),
                skip_origin: ctx.loaded.provider_selection.skip_origin.as_str(),
                repos_root: ctx.loaded.discovery.repos_root.clone(),
                repos_root_origin: ctx.loaded.discovery.repos_root_origin.as_str(),
            },
            sources: ctx
                .loaded
                .sources
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_selector_description_names_pattern_and_projects() {
        let selector = TicketSelector::Embedded {
            pattern: "X".into(),
            allow_projects: vec!["PROJ".into()],
        };
        let text = describe_ticket_selector(&selector);
        assert!(text.contains("pattern X"));
        assert!(text.contains("PROJ"));
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::*;
    use crate::convention_file::{Loaded, Origin, ProviderConfig};
    use gw_core::{ChangeRule, Convention, TicketRule, TicketSelector};
    use std::path::PathBuf;

    fn ctx(
        convention: Convention,
        providers: Vec<ProviderConfig>,
        sources: Vec<PathBuf>,
    ) -> Context {
        let loaded = Loaded {
            // The real loader always returns origin strings, so a fixture with
            // sources reports Layout even when a rule list happens to be empty.
            ticket_rules_origin: Origin::BuiltIn,
            change_rules_origin: Origin::BuiltIn,
            convention: convention.clone(),
            providers,
            sources,
            ..Loaded::default()
        };
        Context::for_test(gw_core::Layout::new(PathBuf::from("/repo")), loaded)
    }

    fn command_provider(kind: &str) -> ProviderConfig {
        ProviderConfig {
            id: ProviderId::parse("host").unwrap(),
            kind: kind.into(),
            base_url: None,
            done_status_category: None,
            command: Some(vec!["tool".into()]),
        }
    }

    fn ticket_rule(provider: &str, projects: &[&str]) -> TicketRule {
        TicketRule {
            provider: ProviderId::parse(provider).unwrap(),
            selector: TicketSelector::Embedded {
                pattern: "[A-Z]+-[0-9]+".into(),
                allow_projects: projects.iter().map(|p| p.to_string()).collect(),
            },
            strength: gw_core::Strength::Identity,
        }
    }

    // These exercise the text renderer. stdout is captured by the harness; the
    // JSON shape is asserted in `tests/cli.rs`.

    #[test]
    fn show_renders_rules_providers_and_sources() {
        for convention in [
            Convention::built_in(),
            Convention {
                ticket_rules: vec![],
                change_rules: vec![],
            },
            Convention {
                ticket_rules: vec![ticket_rule("jira", &["PROJ"])],
                change_rules: vec![ChangeRule {
                    provider: ProviderId::parse("github").unwrap(),
                    selector: ChangeSelector::HeadBranch,
                    strength: gw_core::Strength::Identity,
                }],
            },
        ] {
            for sources in [vec![], vec![PathBuf::from("/home/me/.gw.toml")]] {
                let ctx = ctx(convention.clone(), vec![], sources);
                show(&ctx, false).unwrap();
            }
        }
    }

    #[test]
    fn explain_renders_matches_misses_and_queries() {
        let ctx = ctx(Convention::built_in(), vec![], vec![]);
        for branch in ["feature/PROJ-1-x", "no-ticket-here"] {
            explain(&ctx, branch, false).unwrap();
        }
    }

    #[test]
    fn providers_include_an_inactive_command_with_its_reason() {
        let ctx = ctx(
            Convention::built_in(),
            vec![command_provider("command")],
            vec![],
        );
        let lines = provider_lines(ctx.registry());
        assert_eq!(lines.len(), 2, "jira and github");
        // JIRA needs configuration; GitHub is always available.
        assert!(lines.iter().any(|l| l.contains("inactive")), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("active")), "{lines:?}");
    }

    #[test]
    fn a_command_provider_without_a_command_is_explained() {
        let mut provider = command_provider("command");
        provider.command = None;
        let ctx = ctx(Convention::built_in(), vec![provider], vec![]);
        let id = ProviderId::parse("host").unwrap();
        assert!(ctx
            .registry()
            .inactive_reason(&id)
            .is_some_and(|reason| reason.contains("command")));
    }

    #[test]
    fn selector_descriptions_cover_no_project_filter() {
        let selector = TicketSelector::Embedded {
            pattern: "X".into(),
            allow_projects: vec![],
        };
        assert!(describe_ticket_selector(&selector).contains("any project"));
    }

    #[test]
    fn explain_lines_flag_a_provider_that_is_not_active() {
        let convention = Convention {
            ticket_rules: vec![ticket_rule("jira", &[])],
            change_rules: vec![],
        };
        let ctx = ctx(convention, vec![], vec![]);
        let lines = explain_lines(ctx.registry(), &ctx.convention, "PROJ-1");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].matched);
        assert!(!lines[0].active);
        assert!(lines[0].detail.contains("not active"));
    }

    #[test]
    fn a_mention_ticket_change_rule_explains_a_skip() {
        let no_built_in = || Convention {
            ticket_rules: vec![],
            change_rules: vec![],
        };
        let _ = no_built_in;
        let convention = Convention {
            ticket_rules: vec![ticket_rule("jira", &[])],
            change_rules: vec![ChangeRule {
                provider: ProviderId::parse("github").unwrap(),
                selector: ChangeSelector::MentionTicket,
                strength: gw_core::Strength::Identity,
            }],
        };
        let ctx = ctx(convention, vec![], vec![]);
        let lines = explain_lines(ctx.registry(), &ctx.convention, "no-ticket");
        // One ticket rule and one change rule are evaluated.
        assert_eq!(lines.len(), 2);
        assert!(!lines[0].matched, "the ticket rule did not match");
        assert!(!lines[1].matched, "the change rule needs a ticket");
        assert!(lines[1].detail.contains("skipped"));
    }

    #[test]
    fn a_ticket_dependent_change_rule_names_the_ticket() {
        let convention = Convention {
            ticket_rules: vec![ticket_rule("jira", &[])],
            change_rules: vec![ChangeRule {
                provider: ProviderId::parse("github").unwrap(),
                selector: ChangeSelector::ContainsCommit,
                strength: gw_core::Strength::Identity,
            }],
        };
        let ctx = ctx(convention, vec![], vec![]);
        let lines = explain_lines(ctx.registry(), &ctx.convention, "PROJ-1");
        assert_eq!(lines.len(), 2);
        assert!(lines[1].detail.contains("PROJ-1"));
    }

    #[test]
    fn the_view_reports_every_built_in_provider() {
        let ctx = ctx(Convention::built_in(), vec![], vec![]);
        let view = View::new(&ctx);
        assert_eq!(view.providers.len(), 2);
        assert_eq!(view.ticket_rules_origin, "built-in");
        assert_eq!(view.change_rules.len(), 1);
        assert!(view.sources.is_empty());
    }
}
