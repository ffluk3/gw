use std::path::PathBuf;

use crate::model::BranchName;

/// How `gw add` should obtain a worktree for a branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddStrategy {
    /// A worktree already checks out this branch; reuse it.
    UseExisting { path: PathBuf },
    /// Both a local and a remote ref exist: fast-forward local, then check out.
    FastForwardThenUse,
    /// Only a remote ref exists: create a local branch tracking it.
    TrackRemote,
    /// Neither exists: create a new branch at HEAD.
    CreateNew,
}

/// Decide how to materialize a worktree.
///
/// Replaces the four-arm ladder of the earlier shell implementation, and unlike
/// that version there is no `|| true` swallowing a failed fast-forward: the
/// strategy is a decision, and the adapter reports failure to apply it.
#[must_use]
pub fn add_strategy(existing: Option<PathBuf>, has_local: bool, has_remote: bool) -> AddStrategy {
    if let Some(path) = existing {
        return AddStrategy::UseExisting { path };
    }
    // A local branch means a checkout is possible either way. Whether a remote
    // ref also exists only affects whether `origin/<branch>` is available as a
    // fetch point, which the adapter handles when it applies the strategy.
    if has_local {
        return AddStrategy::FastForwardThenUse;
    }
    if has_remote {
        return AddStrategy::TrackRemote;
    }
    AddStrategy::CreateNew
}

/// Validate a branch name is usable as a worktree directory.
pub fn validate_new_branch(branch: &BranchName) -> Result<(), &'static str> {
    if branch.as_str().contains('\\') {
        return Err("branch name cannot contain a backslash");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_worktree_wins() {
        let s = add_strategy(Some(PathBuf::from("/x")), true, true);
        assert_eq!(
            s,
            AddStrategy::UseExisting {
                path: PathBuf::from("/x")
            }
        );
    }

    #[test]
    fn local_only_still_fast_forwards() {
        assert_eq!(
            add_strategy(None, true, false),
            AddStrategy::FastForwardThenUse
        );
    }

    #[test]
    fn remote_only_tracks() {
        assert_eq!(add_strategy(None, false, true), AddStrategy::TrackRemote);
    }

    #[test]
    fn neither_creates() {
        assert_eq!(add_strategy(None, false, false), AddStrategy::CreateNew);
    }
}
