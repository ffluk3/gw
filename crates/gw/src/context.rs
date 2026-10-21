use std::path::PathBuf;

use gw_core::Layout;

use crate::cli::Cli;
use crate::config::resolve_layout;
use crate::error::Error;
use crate::git::{CliGit, GitBackend};
use crate::settings::Settings;

/// Everything a command needs, wired once.
pub struct Context {
    pub layout: Layout,
    pub git: CliGit,
    pub settings: Settings,
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

        let settings = Settings::load(&layout.root, &cli.jira_project, cli.jira_url.as_deref())?;

        Ok(Self {
            git,
            layout,
            settings,
            json: cli.json,
        })
    }

    /// Whether output should be machine-readable.
    ///
    /// `--json` always wins; otherwise a non-TTY stdout assumes a script.
    pub fn is_json(&self) -> bool {
        self.json
    }
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
