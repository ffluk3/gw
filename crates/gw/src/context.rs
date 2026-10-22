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

        let loaded = crate::convention_file::load(&layout.root)?;
        let registry = Registry::build(&loaded, skip_providers(cli, &loaded));
        let convention = loaded.convention.clone();

        Ok(Self {
            git,
            layout,
            convention,
            loaded,
            registry,
            json: cli.json,
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
        }
    }

    /// Whether output should be machine-readable.
    ///
    /// `--json` always wins; otherwise a non-TTY stdout assumes a script.
    pub fn is_json(&self) -> bool {
        self.json
    }
}

/// Providers to leave out of this run, rejecting invalid slugs.
///
/// The flag and `GW_SKIP_PROVIDERS` come first, then `[providers] skip` from
/// the files. The two sets are unioned rather than replaced: skipping is
/// additive, so a flag can silence one more provider without a team's shared
/// file having to repeat it, and a file cannot un-skip what the invocation
/// asked to skip.
fn skip_providers(cli: &Cli, loaded: &crate::convention_file::Loaded) -> Vec<ProviderId> {
    cli.skip_provider
        .iter()
        .map(String::as_str)
        .chain(loaded.provider_selection.skip.iter().map(String::as_str))
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
    fn file_skips_are_unioned_with_flag_skips() {
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
        assert_eq!(skipped.len(), 2);
        assert_eq!(skipped[0].as_str(), "github");
        assert_eq!(skipped[1].as_str(), "jira");
    }
}
