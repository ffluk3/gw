use std::path::{Path, PathBuf};

use gw_core::{BranchName, Dirty, Sha};

use crate::error::Error;

/// One entry from `git worktree list --porcelain`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeEntry {
    pub path: PathBuf,
    pub head: Option<Sha>,
    pub branch: Option<BranchName>,
    pub bare: bool,
    pub detached: bool,
    pub prunable: bool,
}

impl WorktreeEntry {
    pub fn kind(&self) -> gw_core::WorktreeKind {
        if self.bare {
            return gw_core::WorktreeKind::Main;
        }
        if gw_core::Layout::is_harness_worktree(&self.path) {
            return gw_core::WorktreeKind::Harness;
        }
        if self.prunable {
            return gw_core::WorktreeKind::Prunable;
        }
        gw_core::WorktreeKind::Linked
    }
}

/// One entry from `git for-each-ref refs/heads`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchEntry {
    pub name: BranchName,
    pub worktree: Option<PathBuf>,
    pub tip: Option<Sha>,
    /// Age of the branch tip commit in seconds, as of `now`.
    pub commit_age_secs: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddMode {
    /// Check out an existing branch or start from HEAD.
    Existing,
    /// Create a local branch at the given start point.
    Track { start: TrackStart },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackStart {
    /// `origin/<branch>`
    Remote,
    /// Current HEAD of the bare repo.
    Head,
}

/// Typed git operations.
///
/// A typed trait rather than a generic `run(args)` buys two things: fakes
/// become trivial, and every failure mode has a name in the type system.
pub trait GitBackend {
    fn common_dir(&self, cwd: &Path) -> Result<PathBuf, Error>;
    fn worktree_list(&self) -> Result<Vec<WorktreeEntry>, Error>;
    fn status(&self, path: &Path) -> Result<Dirty, Error>;
    fn rev_list_remotes(&self) -> Result<Vec<Sha>, Error>;
    fn for_each_branch(&self, now_epoch: i64) -> Result<Vec<BranchEntry>, Error>;
    fn default_branch(&self) -> Result<Option<BranchName>, Error>;
    /// The branch checked out by the bare repo, which cannot be deleted.
    fn head_branch(&self) -> Result<Option<BranchName>, Error>;
    fn ref_exists(&self, branch: &BranchName) -> Result<bool, Error>;
    fn remote_ref_exists(&self, branch: &BranchName) -> Result<bool, Error>;
    fn fetch_prune(&self) -> Result<(), Error>;
    fn fast_forward_to_remote(&self, branch: &BranchName) -> Result<(), Error>;
    fn worktree_add(&self, path: &Path, branch: &BranchName, mode: AddMode) -> Result<(), Error>;
    fn worktree_remove(&self, path: &Path) -> Result<(), Error>;
    fn worktree_prune(&self) -> Result<(), Error>;
    fn branch_delete(&self, branch: &BranchName) -> Result<(), Error>;
    fn remote_url(&self) -> Result<Option<String>, Error>;
    fn last_commit_epoch(&self, path: &Path) -> Result<Option<i64>, Error>;
}
