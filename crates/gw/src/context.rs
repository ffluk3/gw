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
        let registry = Registry::build(&loaded, skip_providers(cli));
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

    /// Whether output should be machine-readable.
    ///
    /// `--json` always wins; otherwise a non-TTY stdout assumes a script.
    pub fn is_json(&self) -> bool {
        self.json
    }
}

/// Providers named by `--skip-provider`, rejecting invalid slugs.
fn skip_providers(cli: &Cli) -> Vec<ProviderId> {
    cli.skip_provider
        .iter()
        .filter_map(|raw| ProviderId::parse(raw).ok())
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
