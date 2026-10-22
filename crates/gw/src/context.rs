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
        let registry = Registry::build(&loaded, skip_providers(cli, &loaded));
        let convention = loaded.convention.clone();

        Ok(Self {
            git,
            layout,
            convention,
            loaded,
            registry,
            json: cli.json,
            no_color: cli.no_color,
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

    /// A decorative glyph, or nothing when `--no-color`/`NO_COLOR` is set.
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
/// A non-empty `--skip-provider`/`GW_SKIP_PROVIDERS` replaces `[providers]
/// skip` from the files; an absent or empty flag list inherits the file list.
/// Replacement is fail-closed: skipping suppresses queries, so re-enabling a
/// provider can only add observations, and an observation that fails is
/// `Unknown`, which blocks destruction.
fn skip_providers(cli: &Cli, loaded: &crate::convention_file::Loaded) -> Vec<ProviderId> {
    let selected = if cli.skip_provider.is_empty() {
        &loaded.provider_selection.skip
    } else {
        &cli.skip_provider
    };
    selected
        .iter()
        .filter_map(|raw| ProviderId::parse(raw.trim()).ok())
        .collect()
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
    fn skip_providers_ignores_invalid_slugs() {
        let cli = Cli {
            repo: None,
            json: false,
            no_color: false,
            skip_provider: vec!["github".into(), "a:b".into(), " jira ".into()],
            command: Command::Status(crate::cli::StatusArgs {}),
        };
        let skipped = skip_providers(&cli, &crate::convention_file::Loaded::default());
        assert_eq!(skipped.len(), 2);
        assert_eq!(skipped[0].as_str(), "github");
        assert_eq!(skipped[1].as_str(), "jira");
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
        let skipped = skip_providers(&cli, &loaded);
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].as_str(), "github");
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
        let skipped = skip_providers(&cli, &loaded);
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
