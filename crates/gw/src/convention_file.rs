//! Loading the convention from configuration files.
//!
//! Two files can carry configuration: `~/.gw.toml` (the user's recipes) and
//! `<root>/.gw.toml` (one layout's specialization). They merge field by field,
//! so a recipe can declare a provider once and each layout can narrow it.
//!
//! This module only parses and merges. It does not decide which providers
//! exist and it does not touch the network.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gw_core::{
    ChangeRule, ChangeSelector, Convention, ProviderId, Strength, TicketRule, TicketSelector,
};
use serde::Deserialize;

use crate::error::Error;

/// Name of the per-layout file, at the layout root.
pub const LAYOUT_FILE: &str = ".gw.toml";
/// Name of the user-global file, at `$HOME`.
pub const GLOBAL_FILE: &str = ".gw.toml";
/// Environment variable that points at one explicit settings file instead.
pub const CONFIG_ENV: &str = "GW_CONFIG";
/// Environment variable that suppresses the global file, for tests and for
/// runs that must see only one layout's configuration.
pub const NO_GLOBAL_ENV: &str = "GW_NO_GLOBAL_CONFIG";

/// Where a value came from, for `gw convention show`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// The built-in default, with no file involved.
    #[default]
    BuiltIn,
    /// The user-global file, or `GW_CONFIG`.
    Global,
    /// `<root>/.gw.toml`.
    Layout,
}

impl Origin {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Origin::BuiltIn => "built-in",
            Origin::Global => "global",
            Origin::Layout => "layout",
        }
    }
}

/// A provider declaration from configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderConfig {
    pub id: ProviderId,
    pub kind: String,
    pub base_url: Option<String>,
    pub done_status_category: Option<String>,
    /// For `kind = "command"`: the executable and its fixed arguments.
    pub command: Option<Vec<String>>,
}

impl ProviderConfig {
    /// Merge a higher-precedence declaration over this one, field by field.
    fn merge(&mut self, higher: Self) {
        self.kind = higher.kind;
        if higher.base_url.is_some() {
            self.base_url = higher.base_url;
        }
        if higher.done_status_category.is_some() {
            self.done_status_category = higher.done_status_category;
        }
        if higher.command.is_some() {
            self.command = higher.command;
        }
    }
}

/// Everything the configuration files produced.
#[derive(Debug, Clone, Default)]
pub struct Loaded {
    pub convention: Convention,
    pub ticket_rules_origin: Origin,
    pub change_rules_origin: Origin,
    pub providers: Vec<ProviderConfig>,
    /// Files that were read, in merge order.
    pub sources: Vec<PathBuf>,
}

/// Load one file, with no global layer and no inheritance.
///
/// Used by `gw repos`, which reports what each layout declares rather than
/// what would apply to it.
pub fn load_layout_only(layout_root: &Path) -> Result<Loaded, Error> {
    let mut loaded = Loaded {
        convention: Convention::built_in(),
        ..Loaded::default()
    };
    let path = layout_root.join(LAYOUT_FILE);
    if let Some(file) = File::load(&path)? {
        loaded.sources.push(path);
        apply(&mut loaded, file, Origin::Layout)?;
    }
    Ok(loaded)
}

/// Load and merge the global and layout files.
///
/// Precedence, highest first: the layout file, the global file (or `GW_CONFIG`),
/// then the built-in convention. Absent fields inherit; arrays are replaced as
/// a unit, so `ticket_rules = []` clears an inherited list.
pub fn load(layout_root: &Path) -> Result<Loaded, Error> {
    let mut loaded = Loaded {
        convention: Convention::built_in(),
        ..Loaded::default()
    };

    let layers = [
        (explicit_or_global_path(), Origin::Global),
        (Some(layout_root.join(LAYOUT_FILE)), Origin::Layout),
    ];

    for (path, origin) in layers {
        let Some(path) = path else { continue };
        let Some(file) = File::load(&path)? else {
            continue;
        };
        loaded.sources.push(path);
        apply(&mut loaded, file, origin)?;
    }

    Ok(loaded)
}

fn apply(loaded: &mut Loaded, file: File, origin: Origin) -> Result<(), Error> {
    let Some(convention) = file.convention else {
        return Ok(());
    };

    if let Some(rules) = convention.ticket_rules {
        loaded.convention.ticket_rules = rules
            .into_iter()
            .map(TicketRule::try_from)
            .collect::<Result<_, _>>()?;
        loaded.ticket_rules_origin = origin;
    }
    if let Some(rules) = convention.change_rules {
        loaded.convention.change_rules = rules
            .into_iter()
            .map(ChangeRule::try_from)
            .collect::<Result<_, _>>()?;
        loaded.change_rules_origin = origin;
    }

    for (raw_id, table) in convention.providers {
        let id = ProviderId::parse(&raw_id)
            .map_err(|e| Error::Usage(format!("invalid provider name '{raw_id}': {e}")))?;
        let config = table.into_config(id);
        match loaded.providers.iter_mut().find(|p| p.id == config.id) {
            Some(existing) => existing.merge(config),
            None => loaded.providers.push(config),
        }
    }

    Ok(())
}

/// `GW_CONFIG` when set, otherwise `$HOME/.gw.toml` unless suppressed.
fn explicit_or_global_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(CONFIG_ENV) {
        return Some(PathBuf::from(path));
    }
    if std::env::var_os(NO_GLOBAL_ENV).is_some() {
        return None;
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(GLOBAL_FILE))
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    convention: Option<ConventionTable>,
}

impl File {
    fn load(path: &Path) -> Result<Option<Self>, Error> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(Error::Usage(format!("cannot read {}: {e}", path.display()))),
        };
        toml::from_str(&text).map(Some).map_err(|e| {
            Error::Usage(format!(
                "invalid {}:\n{}{}",
                path.display(),
                indent(&e.to_string()),
                hint(&e)
            ))
        })
    }
}

/// Indent each line of a parser error under the file name, so a multi-line
/// caret diagram stays aligned.
fn indent(text: &str) -> String {
    text.lines()
        .map(|line| format!("  {line}\n"))
        .collect::<String>()
}

/// A next step for the two configuration mistakes a user actually makes.
///
/// The parser says `unknown field \`jira\`, expected \`convention\``, which is
/// accurate and useless to someone holding a config file from an earlier
/// version. Name the replacement instead.
fn hint(error: &toml::de::Error) -> &'static str {
    let text = error.to_string();
    if text.contains("unknown field `jira`") {
        return "\n  JIRA is configured under [convention.providers.jira], with its\n  \
                project filter on a [[convention.ticket_rules]] entry.\n  \
                See docs/how-to/configure-jira.md.\n";
    }
    if text.contains("unknown field `project`") || text.contains("unknown field `base_url`") {
        return "\n  Set these under [convention.providers.jira] and the ticket rule.\n";
    }
    if text.contains("unknown field `provider`") {
        return "\n  A rule needs provider and selector. See docs/reference/configuration.md.\n";
    }
    ""
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConventionTable {
    #[serde(default)]
    providers: BTreeMap<String, ProviderTable>,
    /// `Option` distinguishes "absent, inherit" from "present but empty, clear".
    #[serde(default)]
    ticket_rules: Option<Vec<TicketRuleTable>>,
    #[serde(default)]
    change_rules: Option<Vec<ChangeRuleTable>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderTable {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    done_status_category: Option<String>,
    #[serde(default)]
    command: Option<CommandValue>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum CommandValue {
    One(String),
    Many(Vec<String>),
}

impl ProviderTable {
    fn into_config(self, id: ProviderId) -> ProviderConfig {
        let kind = self.kind.unwrap_or_else(|| id.as_str().to_string());
        ProviderConfig {
            id,
            kind,
            base_url: self.base_url,
            done_status_category: self.done_status_category,
            command: self.command.map(|c| match c {
                CommandValue::One(value) => shell_words(&value),
                CommandValue::Many(values) => values,
            }),
        }
    }
}

/// Split a command string the way a shell would, without invoking one.
///
/// A path with spaces has to be quotable, and users write `command = "tool
/// --flag"` more often than an array. Single quotes, double quotes, and
/// backslash escapes are honored; nothing is expanded.
fn shell_words(value: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut has_word = false;

    for c in value.chars() {
        if escaped {
            current.push(c);
            escaped = false;
            has_word = true;
            continue;
        }
        match (quote, c) {
            (_, '\\') if quote != Some('\'') => {
                escaped = true;
                has_word = true;
            }
            (None, '\'' | '"') => {
                quote = Some(c);
                has_word = true;
            }
            (Some(q), c) if c == q => quote = None,
            (None, c) if c.is_whitespace() => {
                if has_word {
                    words.push(std::mem::take(&mut current));
                    has_word = false;
                }
            }
            _ => {
                current.push(c);
                has_word = true;
            }
        }
    }
    if has_word {
        words.push(current);
    }
    words
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TicketRuleTable {
    provider: String,
    /// Selector name, validated here rather than by `serde(tag = ...)`.
    ///
    /// A tagged enum has to be `flatten`ed, and `flatten` silently disables
    /// `deny_unknown_fields`. An explicit field keeps a typo in a rule name an
    /// error instead of a rule that quietly does nothing.
    selector: String,
    #[serde(default)]
    pattern: Option<String>,
    #[serde(default)]
    allow_projects: Vec<String>,
    #[serde(default)]
    strength: Option<StrengthValue>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChangeRuleTable {
    provider: String,
    selector: String,
    #[serde(default)]
    strength: Option<StrengthValue>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StrengthValue {
    Identity,
    Mention,
}

impl From<StrengthValue> for Strength {
    fn from(value: StrengthValue) -> Self {
        match value {
            StrengthValue::Identity => Strength::Identity,
            StrengthValue::Mention => Strength::Mention,
        }
    }
}

impl TryFrom<TicketRuleTable> for TicketRule {
    type Error = Error;

    fn try_from(table: TicketRuleTable) -> Result<Self, Self::Error> {
        let provider = ProviderId::parse(&table.provider)
            .map_err(|e| Error::Usage(format!("invalid provider '{}': {e}", table.provider)))?;
        let selector = match table.selector.as_str() {
            "embedded" => {
                let pattern = table.pattern.ok_or_else(|| {
                    Error::Usage(format!(
                        "ticket rule for provider '{}' uses selector \"embedded\" but has no pattern",
                        table.provider
                    ))
                })?;
                // Reject an uncompilable pattern at load time. A rule that
                // silently matches nothing is indistinguishable from "the
                // branch has no ticket", which would quietly disable cleanup.
                gw_core::convention::validate_pattern(&pattern).map_err(|e| {
                    Error::Usage(format!(
                        "invalid ticket pattern '{pattern}' for provider '{}': {e}",
                        table.provider
                    ))
                })?;
                TicketSelector::Embedded {
                    pattern,
                    allow_projects: table.allow_projects,
                }
            }
            other => {
                return Err(Error::Usage(format!(
                    "unknown ticket selector '{other}'. Supported selectors: embedded."
                )))
            }
        };
        Ok(TicketRule {
            provider,
            selector,
            strength: table.strength.map_or(Strength::Identity, Into::into),
        })
    }
}

impl TryFrom<ChangeRuleTable> for ChangeRule {
    type Error = Error;

    fn try_from(table: ChangeRuleTable) -> Result<Self, Self::Error> {
        let provider = ProviderId::parse(&table.provider)
            .map_err(|e| Error::Usage(format!("invalid provider '{}': {e}", table.provider)))?;
        let selector = match table.selector.as_str() {
            "head_branch" => ChangeSelector::HeadBranch,
            "contains_commit" => ChangeSelector::ContainsCommit,
            "mention_ticket" => ChangeSelector::MentionTicket,
            other => {
                return Err(Error::Usage(format!(
                    "unknown change selector '{other}'. Supported selectors: \
                     head_branch, contains_commit, mention_ticket."
                )))
            }
        };
        Ok(ChangeRule {
            provider,
            selector,
            strength: table.strength.map_or(Strength::Identity, Into::into),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_words_handles_quotes_and_spaces() {
        assert_eq!(shell_words("tool --flag"), vec!["tool", "--flag"]);
        assert_eq!(
            shell_words("'/path/with space/tool' -x"),
            vec!["/path/with space/tool", "-x"]
        );
        assert_eq!(shell_words("a\\ b"), vec!["a b"]);
        assert_eq!(shell_words(""), Vec::<String>::new());
    }

    #[test]
    fn an_invalid_pattern_fails_to_load() {
        let table: TicketRuleTable =
            toml::from_str("provider = \"jira\"\nselector = \"embedded\"\npattern = \"([\"\n")
                .unwrap();
        assert!(TicketRule::try_from(table).is_err());
    }

    #[test]
    fn an_unknown_key_is_rejected() {
        let parsed: Result<File, _> = toml::from_str("[convention]\nnope = 1\n");
        assert!(parsed.is_err());
    }

    #[test]
    fn absent_rules_inherit_and_empty_rules_clear() {
        let inherit: ConventionTable = toml::from_str("").unwrap();
        assert!(inherit.ticket_rules.is_none());
        let clear: ConventionTable = toml::from_str("ticket_rules = []\n").unwrap();
        assert_eq!(clear.ticket_rules.map(|r| r.len()), Some(0));
    }

    #[test]
    fn a_single_command_string_is_split_into_words() {
        let table: ProviderTable =
            toml::from_str("kind = \"command\"\ncommand = \"tool --json\"\n").unwrap();
        let config = table.into_config(ProviderId::parse("linear").unwrap());
        assert_eq!(config.kind, "command");
        assert_eq!(config.command.unwrap(), vec!["tool", "--json"]);
    }
}
