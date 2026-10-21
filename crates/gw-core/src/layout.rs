use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::model::BranchName;

/// Where a bare repo and its worktrees live.
///
/// Mirrors the on-disk convention established by `worktree_setup.sh`:
///
/// ```text
/// <root>/
///   .bare/            # the bare repository
///   worktrees/<ref>/  # one linked worktree per branch
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Layout {
    pub root: PathBuf,
    pub bare: PathBuf,
    pub worktrees: PathBuf,
}

impl Layout {
    pub fn new(root: PathBuf) -> Self {
        let worktrees = detect_worktrees_dir(&root);
        let bare = root.join(".bare");
        Self {
            root,
            bare,
            worktrees,
        }
    }

    /// Same layout with an explicitly known worktrees directory.
    pub fn with_worktrees(self, worktrees: PathBuf) -> Self {
        Self { worktrees, ..self }
    }

    /// Derive a layout from the common git directory reported by git itself:
    /// `<root>/.bare` implies `<root>` is the repo root.
    pub fn from_common_dir(common: &Path) -> Option<Self> {
        if common.file_name()?.to_str()? != ".bare" {
            return None;
        }
        let root = common.parent()?.to_path_buf();
        Some(Self::new(root))
    }

    /// The checkout directory for a branch.
    ///
    /// Branch names keep their slashes, so `feature/x` maps to
    /// `worktrees/feature/x`. This preserves compatibility with existing
    /// checkouts created by the shell implementation.
    pub fn worktree_path(&self, branch: &BranchName) -> PathBuf {
        self.worktrees.join(branch.as_str())
    }

    /// Whether a path belongs to Claude's ephemeral agent worktrees, which the
    /// harness owns and gw must never touch.
    pub fn is_harness_worktree(path: &Path) -> bool {
        let text = path.to_string_lossy();
        text.contains("/.claude/worktrees/")
    }
}

/// Find where worktrees live.
///
/// `worktrees/` is the documented convention, but an existing checkout may use
/// the sibling `‎.bare.worktrees/` form. Prefer whichever already exists so gw
/// refuses to create a second, divergent layout.
fn detect_worktrees_dir(root: &Path) -> PathBuf {
    for candidate in ["worktrees", ".bare.worktrees"] {
        let path = root.join(candidate);
        if path.is_dir() {
            return path;
        }
    }
    root.join("worktrees")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worktree_path_preserves_slashes() {
        let layout = Layout::new(PathBuf::from("/repo"));
        let branch = BranchName::parse("feature/x").unwrap();
        assert_eq!(
            layout.worktree_path(&branch),
            PathBuf::from("/repo/worktrees/feature/x")
        );
    }

    #[test]
    fn from_common_dir_requires_bare_suffix() {
        let layout = Layout::from_common_dir(Path::new("/repo/.bare")).unwrap();
        assert_eq!(layout.root, PathBuf::from("/repo"));
        assert_eq!(layout.worktrees, PathBuf::from("/repo/worktrees"));

        assert!(Layout::from_common_dir(Path::new("/repo/.git")).is_none());
    }

    #[test]
    fn harness_detection() {
        assert!(Layout::is_harness_worktree(Path::new(
            "/repo/.bare/.claude/worktrees/agent-abc"
        )));
        assert!(!Layout::is_harness_worktree(Path::new(
            "/repo/worktrees/AX-1"
        )));
    }
}
