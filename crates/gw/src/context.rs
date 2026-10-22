use std::path::PathBuf;

use gw_core::{Convention, Layout, ProviderId};

use crate::cli::Cli;
use crate::config::resolve_layout;
use crate::error::Error;
use crate::git::{CliGit, GitBackend};
use crate::providers::Registry;

/// Everything a command needs, wired once.
pub struct Context {
    pub layout: Layout,
    pub git: CliGit,
    /// The convention in force: what a branch name means.
    pub convention: Convention,
    /// The full loaded configuration, kept for `gw convention show`.
    pub loaded: crate::convention_file::Loaded,
    /// The providers available for this run.
    registry: Registry,
    pub json: bool,
    /// Whether decorative emoji are suppressed in human output.
    no_color: bool,
}

impl Context {
    pub fn load(cli: &Cli) -> Result<Self, Error> {
        let cwd: PathBuf = std::env::current_dir()?;

        // Discovery needs a git handle, but that handle needs the bare path.
        // Probe with an unanchored handle first, then rebuild anchored.
        let probe = CliGit::new(cwd.clone());
        let explicit = match cli.repo.as_deref() {
            Some(value) => Some(crate::repos::resolve_repo_arg(value, &cwd)?),
            None => None,
        };
        let mut layout = resolve_layout(explicit.as_deref(), &cwd, &probe as &dyn GitBackend)?;

        let git = CliGit::new(layout.bare.clone());

        // Trust the repository over the guess: if the existing worktrees live
        // somewhere other than the conventional directory, adopt that path so
        // gw never creates a second layout beside the real one.
        if let Ok(entries) = git.worktree_list() {
            if let Some(parent) = infer_worktrees_dir(&entries, &layout) {
                layout = layout.with_worktrees(parent);
            }
        }

        // The default-branch layer is derived here because `Context` owns the
        // git handle. `convention_file` is handed a path, never a backend.
        let default_branch = git.default_branch()?;
        let layer = default_branch_layer(&layout, default_branch.as_ref());
        let loaded = crate::convention_file::load(&layout.root, layer.as_deref())?;
        let registry = Registry::build(&loaded, skip_providers(cli, &loaded)?);
        let convention = loaded.convention.clone();

        Ok(Self {
            git,
            layout,
            convention,
            loaded,
            registry,
            json: cli.json,
            no_color: cli.color_disabled(),
        })
    }

    /// The providers available for this run.
    #[must_use]
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Build a context from already-resolved parts.
    ///
    /// Test-only: the real constructor owns discovery and file loading, which
    /// a unit test of a renderer should not have to reproduce.
    #[cfg(test)]
    pub(crate) fn for_test(layout: Layout, loaded: crate::convention_file::Loaded) -> Self {
        let git = CliGit::new(layout.bare.clone());
        let registry = Registry::build(&loaded, Vec::new());
        let convention = loaded.convention.clone();
        Self {
            git,
            layout,
            convention,
            loaded,
            registry,
            json: false,
            no_color: false,
        }
    }

    /// Whether output should be machine-readable.
    ///
    /// `--json` always wins; otherwise a non-TTY stdout assumes a script.
    pub fn is_json(&self) -> bool {
        self.json
    }

    /// A decorative glyph, or nothing when decoration is disabled.
    ///
    /// That is `--no-color`, or `NO_COLOR` present and non-empty.
    ///
    /// The trailing space belongs to the glyph so plain output has no gap left
    /// behind. Table markers (`●`, `—`) are content, not decoration, and never
    /// pass through here.
    pub fn decor(&self, glyph: &'static str) -> &'static str {
        crate::output::decor(!self.no_color, glyph)
    }
}

/// The checkout directory whose `.gw.toml` forms the default-branch layer.
///
/// `None` when `refs/remotes/origin/HEAD` did not resolve. The branch is never
/// guessed from `HEAD` or a branch name: an unset ref means no layer, not a
/// default guess.
fn default_branch_layer(layout: &Layout, branch: Option<&gw_core::BranchName>) -> Option<PathBuf> {
    branch.map(|branch| layout.worktree_path(branch))
}

/// Providers to leave out of this run, rejecting invalid slugs.
///
/// `--skip-provider`/`GW_SKIP_PROVIDERS` replaces `[providers] skip` from the
/// files when at least one entry is non-empty after trimming. If the flag is
/// absent, or every entry is empty, the file list applies.
///
/// An invalid non-empty id from the flag or the environment is a usage error.
/// Dropping it silently would clear the file's skip list while quietly failing
/// to skip the provider the user named — less skipping than either input asked
/// for. "Invalid" covers both a syntactically bad slug and a well-formed slug
/// that names no provider, because a typo such as `jria` is exactly the case
/// that used to clear the file list. The file list is validated where it is
/// parsed; here an unparseable entry is still ignored so a bad file cannot be
/// masked by the flag path.
///
/// Replacement is the user's explicit per-invocation choice, not a safety
/// guarantee. A provider that a layout skipped because it is not authoritative
/// there can answer once it is re-enabled, and that answer can change a
/// verdict. Skipping only withholds observations; it does not make them safer.
fn skip_providers(
    cli: &Cli,
    loaded: &crate::convention_file::Loaded,
) -> Result<Vec<ProviderId>, Error> {
    let flag: Vec<&str> = cli
        .skip_provider
        .iter()
        .map(|raw| raw.trim())
        .filter(|raw| !raw.is_empty())
        .collect();
    if flag.is_empty() {
        return Ok(loaded
            .provider_selection
            .skip
            .iter()
            .map(|raw| raw.trim())
            .filter_map(|raw| ProviderId::parse(raw).ok())
            .collect());
    }
    let known = known_provider_ids(loaded);
    flag.into_iter()
        .map(|raw| {
            let id = ProviderId::parse(raw).map_err(|e| {
                Error::Usage(format!(
                    "invalid provider id '{raw}' in --skip-provider or \
                     GW_SKIP_PROVIDERS: {e}; provider ids are 1-64 characters \
                     with no whitespace, control characters, or ':', '/', or '\\'"
                ))
            })?;
            if !known.contains(&id) {
                let names: Vec<&str> = known.iter().map(ProviderId::as_str).collect();
                return Err(Error::Usage(format!(
                    "invalid provider id '{raw}' in --skip-provider or \
                     GW_SKIP_PROVIDERS: no provider with that id; known providers \
                     are {}",
                    names.join(", ")
                )));
            }
            Ok(id)
        })
        .collect()
}

/// Every provider id a run could select: the built-ins plus every provider the
/// files declare. Used to refuse a skip id that names nothing.
fn known_provider_ids(
    loaded: &crate::convention_file::Loaded,
) -> std::collections::BTreeSet<ProviderId> {
    let mut ids: std::collections::BTreeSet<ProviderId> = loaded
        .providers
        .iter()
        .map(|config| config.id.clone())
        .collect();
    for raw in ["jira", "github"] {
        ids.insert(ProviderId::parse(raw).expect("literal is valid"));
    }
    ids
}

/// Infer the worktrees directory from the worktrees git actually reports.
///
/// Only a directory that appears as the common parent of at least one linked
/// worktree is accepted, so a stray path can never widen the managed area.
fn infer_worktrees_dir(
    entries: &[crate::git::WorktreeEntry],
    layout: &Layout,
) -> Option<std::path::PathBuf> {
    let mut counts: std::collections::HashMap<std::path::PathBuf, usize> =
        std::collections::HashMap::new();
    for entry in entries {
        if entry.bare || gw_core::Layout::is_harness_worktree(&entry.path) {
            continue;
        }
        let Some(parent) = entry.path.parent() else {
            continue;
        };
        // Only consider direct children of the repo root.
        if parent.parent() != Some(layout.root.as_path()) {
            continue;
        }
        *counts.entry(parent.to_path_buf()).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(path, _)| path)
}

#[cfg(test)]
mod coverage_tests {
    use super::*;
    use crate::cli::Command;
    use crate::git::WorktreeEntry;

    fn entry(path: &str, bare: bool) -> WorktreeEntry {
        WorktreeEntry {
            path: std::path::PathBuf::from(path),
            head: None,
            branch: None,
            bare,
            detached: false,
            prunable: false,
        }
    }

    #[test]
    fn the_worktrees_dir_is_inferred_from_direct_children() {
        let layout = Layout::new(std::path::PathBuf::from("/repo"));
        let entries = [
            entry("/repo/worktrees/a", false),
            entry("/repo/worktrees/b", false),
            entry("/repo/worktrees/c", false),
            entry("/repo/other/d", false),
            entry("/repo/.bare/.claude/worktrees/agent", false),
            entry("/repo/wt-e", false),
            entry("/repo/.bare", true),
            entry("/", false),
        ];
        assert_eq!(
            infer_worktrees_dir(&entries, &layout),
            Some(std::path::PathBuf::from("/repo/worktrees"))
        );
    }

    #[test]
    fn no_direct_children_means_no_guess() {
        let layout = Layout::new(std::path::PathBuf::from("/repo"));
        let entries = [
            entry("/repo/.bare", true),
            entry("/repo/other/deep/x", false),
            entry("/", false),
        ];
        assert!(infer_worktrees_dir(&entries, &layout).is_none());
        assert!(infer_worktrees_dir(&[], &layout).is_none());
    }

    #[test]
    fn skip_providers_trims_valid_slugs() {
        let cli = Cli {
            repo: None,
            json: false,
            no_color: false,
            skip_provider: vec!["github".into(), " jira ".into()],
            command: Command::Status(crate::cli::StatusArgs {}),
        };
        let skipped =
            skip_providers(&cli, &crate::convention_file::Loaded::default()).expect("valid ids");
        assert_eq!(skipped.len(), 2);
        assert_eq!(skipped[0].as_str(), "github");
        assert_eq!(skipped[1].as_str(), "jira");
    }

    #[test]
    fn an_invalid_flag_slug_is_a_usage_error() {
        // A layout skip list must not be cleared by a typo: the call fails
        // before any effective selection is built. `jria` is a well-formed
        // slug, so this also proves the id is checked against real providers.
        let loaded = crate::convention_file::Loaded {
            provider_selection: crate::convention_file::ProviderSelection {
                skip: vec!["github".into()],
                ..Default::default()
            },
            ..Default::default()
        };
        for (raw, expected) in [("jria", "known providers"), ("a:b", "1-64")] {
            let cli = Cli {
                repo: None,
                json: false,
                no_color: false,
                skip_provider: vec![raw.into()],
                command: Command::Status(crate::cli::StatusArgs {}),
            };
            let err = skip_providers(&cli, &loaded).expect_err("invalid id must be refused");
            let message = err.to_string();
            assert!(message.contains(raw), "{message}");
            assert!(message.contains("GW_SKIP_PROVIDERS"), "{message}");
            assert!(message.contains(expected), "{message}");
        }
    }

    #[test]
    fn a_declared_provider_may_be_skipped() {
        let cli = Cli {
            repo: None,
            json: false,
            no_color: false,
            skip_provider: vec!["linear".into()],
            command: Command::Status(crate::cli::StatusArgs {}),
        };
        let loaded = crate::convention_file::Loaded {
            providers: vec![crate::convention_file::ProviderConfig {
                id: ProviderId::parse("linear").unwrap(),
                kind: Some("command".into()),
                base_url: None,
                done_status_category: None,
                command: None,
            }],
            ..Default::default()
        };
        let skipped = skip_providers(&cli, &loaded).expect("a declared id is known");
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].as_str(), "linear");
    }

    #[test]
    fn flag_skips_replace_file_skips() {
        let cli = Cli {
            repo: None,
            json: false,
            no_color: false,
            skip_provider: vec!["github".into()],
            command: Command::Status(crate::cli::StatusArgs {}),
        };
        let loaded = crate::convention_file::Loaded {
            provider_selection: crate::convention_file::ProviderSelection {
                skip: vec!["jira".into()],
                ..Default::default()
            },
            ..Default::default()
        };
        let skipped = skip_providers(&cli, &loaded).expect("valid ids");
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].as_str(), "github");
    }

    #[test]
    fn an_explicitly_empty_flag_list_inherits_the_file_list() {
        // `--skip-provider ""` reaches clap as one empty entry. It must not
        // count as a replacement that clears the file list.
        let cli = Cli {
            repo: None,
            json: false,
            no_color: false,
            skip_provider: vec!["".into(), "   ".into()],
            command: Command::Status(crate::cli::StatusArgs {}),
        };
        let loaded = crate::convention_file::Loaded {
            provider_selection: crate::convention_file::ProviderSelection {
                skip: vec!["jira".into()],
                ..Default::default()
            },
            ..Default::default()
        };
        let skipped = skip_providers(&cli, &loaded).expect("valid ids");
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].as_str(), "jira");
    }

    #[test]
    fn absent_flag_skips_inherit_the_file_list() {
        let cli = Cli {
            repo: None,
            json: false,
            no_color: false,
            skip_provider: vec![],
            command: Command::Status(crate::cli::StatusArgs {}),
        };
        let loaded = crate::convention_file::Loaded {
            provider_selection: crate::convention_file::ProviderSelection {
                skip: vec!["jira".into()],
                ..Default::default()
            },
            ..Default::default()
        };
        let skipped = skip_providers(&cli, &loaded).expect("valid ids");
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].as_str(), "jira");
    }

    #[test]
    fn the_default_branch_layer_preserves_slashes_and_skips_none() {
        let layout = Layout::new(std::path::PathBuf::from("/repo"));
        let develop = gw_core::BranchName::parse("develop").unwrap();
        let feature = gw_core::BranchName::parse("feature/x").unwrap();
        assert_eq!(
            default_branch_layer(&layout, Some(&develop)),
            Some(std::path::PathBuf::from("/repo/worktrees/develop"))
        );
        assert_eq!(
            default_branch_layer(&layout, Some(&feature)),
            Some(std::path::PathBuf::from("/repo/worktrees/feature/x"))
        );
        assert_eq!(default_branch_layer(&layout, None), None);
    }
}
