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

#[cfg(test)]
mod coverage_tests {
    use super::*;

    #[test]
    fn a_higher_layer_replaces_only_the_fields_it_sets() {
        let id = ProviderId::parse("jira").unwrap();
        let mut lower = ProviderConfig {
            id: id.clone(),
            kind: "jira".into(),
            base_url: Some("https://lower".into()),
            done_status_category: Some("done".into()),
            command: None,
        };
        lower.merge(ProviderConfig {
            id,
            kind: "command".into(),
            base_url: None,
            done_status_category: None,
            command: Some(vec!["tool".into()]),
        });
        assert_eq!(lower.kind, "command");
        assert_eq!(lower.base_url.as_deref(), Some("https://lower"));
        assert_eq!(lower.done_status_category.as_deref(), Some("done"));
        assert_eq!(lower.command.unwrap(), vec!["tool".to_string()]);
    }

    #[test]
    fn a_file_without_a_convention_table_is_inert() {
        let mut loaded = Loaded {
            convention: Convention::built_in(),
            ..Loaded::default()
        };
        apply(&mut loaded, File { convention: None }, Origin::Layout).unwrap();
        assert_eq!(
            loaded.convention.ticket_rules.len(),
            1,
            "the built-in rule survived"
        );
        assert_eq!(loaded.ticket_rules_origin, Origin::BuiltIn);
    }

    #[test]
    fn provider_tables_merge_across_files() {
        let dir = tempfile::tempdir().unwrap();
        let global = dir.path().join("global.toml");
        std::fs::write(
            &global,
            "[convention.providers.jira]\nbase_url = \"https://global\"\n",
        )
        .unwrap();
        let layout = dir.path().join("layout");
        std::fs::create_dir_all(&layout).unwrap();
        std::fs::write(
            layout.join(LAYOUT_FILE),
            "[convention.providers.jira]\ndone_status_category = \"shipped\"\n",
        )
        .unwrap();

        let _guard = EnvGuard::new(&global);
        let loaded = load(&layout).unwrap();
        let jira = loaded
            .providers
            .iter()
            .find(|p| p.id.as_str() == "jira")
            .expect("jira was declared");
        assert_eq!(jira.base_url.as_deref(), Some("https://global"));
        assert_eq!(jira.done_status_category.as_deref(), Some("shipped"));
        assert_eq!(loaded.sources.len(), 2);
    }

    #[test]
    fn an_unreadable_file_is_a_usage_error() {
        let dir = tempfile::tempdir().unwrap();
        // A directory where a file is expected is readable-but-not-a-file.
        let path = dir.path().join("gw.toml");
        std::fs::create_dir_all(&path).unwrap();
        assert!(matches!(File::load(&path), Err(Error::Usage(_))));
    }

    #[test]
    fn a_missing_file_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(File::load(&dir.path().join("absent.toml"))
            .unwrap()
            .is_none());
    }

    #[test]
    fn parser_hints_name_the_replacement_for_legacy_keys() {
        let with_hint = [
            "jira = 1\n",
            "[convention.providers.jira]\nproject = \"X\"\n",
            "[convention]\nbase_url = 1\n",
            "[convention]\nprovider = \"jira\"\n",
        ];
        for source in with_hint {
            let error = toml::from_str::<File>(source)
                .err()
                .unwrap_or_else(|| panic!("accepted {source:?}"));
            assert!(!hint(&error).is_empty(), "no hint for {error}");
        }

        // A shape error that no hint covers stays bare.
        let error = toml::from_str::<File>("x = 1\n").unwrap_err();
        assert!(hint(&error).is_empty());
    }

    #[test]
    fn a_legacy_config_gets_a_targeted_hint() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gw.toml");
        std::fs::write(&path, "[jira]\nbase_url = \"x\"\n").unwrap();
        let error = File::load(&path).unwrap_err();
        assert!(
            error.to_string().contains("[convention.providers.jira]"),
            "{error}"
        );
    }

    #[test]
    fn a_provider_name_is_validated() {
        // Parsing accepts any key; validation happens when the table is
        // applied, so the file is otherwise valid.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gw.toml");
        std::fs::write(
            &path,
            "[[convention.ticket_rules]]\nprovider = \"jira\"\nselector = \"embedded\"\npattern = \"x\"\n\n[convention.providers.\"a:b\"]\nkind = \"command\"\n",
        )
        .unwrap();
        let mut loaded = Loaded {
            convention: Convention::built_in(),
            ..Loaded::default()
        };
        let file = File::load(&path).unwrap().unwrap();
        let error = apply(&mut loaded, file, Origin::Layout).unwrap_err();
        assert!(
            error.to_string().contains("invalid provider name"),
            "{error}"
        );
    }

    #[test]
    fn an_array_command_is_kept_verbatim() {
        let table: ProviderTable =
            toml::from_str("kind = \"command\"\ncommand = [\"tool\", \"--json\"]\n").unwrap();
        let config = table.into_config(ProviderId::parse("linear").unwrap());
        assert_eq!(config.command.unwrap(), vec!["tool", "--json"]);
        // A provider without a kind borrows its id.
        let bare: ProviderTable = toml::from_str("").unwrap();
        assert_eq!(
            bare.into_config(ProviderId::parse("linear").unwrap()).kind,
            "linear"
        );
    }

    #[test]
    fn strength_values_convert() {
        assert_eq!(Strength::from(StrengthValue::Identity), Strength::Identity);
        assert_eq!(Strength::from(StrengthValue::Mention), Strength::Mention);
    }

    #[test]
    fn rule_conversion_rejects_incomplete_rules() {
        let no_pattern: TicketRuleTable =
            toml::from_str("provider = \"jira\"\nselector = \"embedded\"\n").unwrap();
        assert!(TicketRule::try_from(no_pattern).is_err());

        let bad_provider: TicketRuleTable =
            toml::from_str("provider = \"a:b\"\nselector = \"embedded\"\npattern = \"x\"\n")
                .unwrap();
        assert!(TicketRule::try_from(bad_provider).is_err());

        let bad_change_provider: ChangeRuleTable =
            toml::from_str("provider = \"a:b\"\nselector = \"head_branch\"\n").unwrap();
        assert!(ChangeRule::try_from(bad_change_provider).is_err());

        let unknown_selector: ChangeRuleTable =
            toml::from_str("provider = \"github\"\nselector = \"nope\"\n").unwrap();
        assert!(ChangeRule::try_from(unknown_selector).is_err());
    }

    #[test]
    fn every_change_selector_name_is_accepted() {
        for name in ["head_branch", "contains_commit", "mention_ticket"] {
            let table: ChangeRuleTable =
                toml::from_str(&format!("provider = \"github\"\nselector = \"{name}\"\n")).unwrap();
            assert!(ChangeRule::try_from(table).is_ok(), "{name}");
        }
    }

    #[test]
    fn strength_is_read_from_a_rule() {
        let table: TicketRuleTable = toml::from_str(
            "provider = \"jira\"\nselector = \"embedded\"\npattern = \"x\"\nstrength = \"mention\"\n",
        )
        .unwrap();
        assert_eq!(
            TicketRule::try_from(table).unwrap().strength,
            Strength::Mention
        );
    }

    #[test]
    fn layout_only_loading_ignores_the_global_layer() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(LAYOUT_FILE),
            "[[convention.ticket_rules]]\nprovider = \"linear\"\nselector = \"embedded\"\npattern = \"x\"\n",
        )
        .unwrap();
        let loaded = load_layout_only(dir.path()).unwrap();
        assert_eq!(
            loaded.convention.ticket_rules[0].provider.as_str(),
            "linear"
        );
        assert_eq!(loaded.sources.len(), 1);
    }

    /// Point `GW_CONFIG` at one file for the duration of a test.
    struct EnvGuard<'a> {
        previous: Option<std::ffi::OsString>,
        _lock: std::sync::MutexGuard<'a, ()>,
    }

    impl<'a> EnvGuard<'a> {
        fn new(path: &Path) -> Self {
            static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
            let lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let previous = std::env::var_os(CONFIG_ENV);
            std::env::set_var(CONFIG_ENV, path);
            Self {
                previous,
                _lock: lock,
            }
        }
    }

    impl Drop for EnvGuard<'_> {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(value) => std::env::set_var(CONFIG_ENV, value),
                None => std::env::remove_var(CONFIG_ENV),
            }
        }
    }
}
