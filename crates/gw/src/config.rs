use std::path::{Path, PathBuf};

use gw_core::Layout;

use crate::error::Error;
use crate::git::GitBackend;

/// Resolve where the repository lives.
///
/// Precedence: `--repo` / `GW_REPO`, then discovery from the current directory.
///
/// Discovery accepts four starting points, in this order:
///
/// 1. any linked worktree (`<root>/worktrees/<branch>/`, including a
///    subdirectory of one),
/// 2. the bare repository itself (`<root>/.bare/`),
/// 3. the layout root (`<root>`), recognized by the presence of `<root>/.bare`,
/// 4. anything else under `<root>` that git can resolve.
///
/// Anywhere else is an error, and the message names the two ways out.
pub fn resolve_layout(
    explicit: Option<&Path>,
    cwd: &Path,
    git: &dyn GitBackend,
) -> Result<Layout, Error> {
    if let Some(root) = explicit {
        let root = normalize_root(root)?;
        return Ok(Layout::new(root));
    }

    // Case 3: standing in the layout root. This directory is not itself a git
    // repository, but `<root>/.bare` is, so recognize it explicitly rather
    // than making the user cd into a checkout first.
    if cwd.join(".bare").join("HEAD").is_file() {
        return Ok(Layout::new(cwd.to_path_buf()));
    }

    let Ok(common) = git.common_dir(cwd) else {
        // Cases 1-2 failed and there is no `.bare` here: either an
        // ancestor is a layout root, or this is not a repository at all.
        if let Some(root) = find_ancestor_root(cwd) {
            return Ok(Layout::new(root));
        }
        return Err(Error::Layout(not_a_repository(cwd)));
    };

    // Case 4 covers a subdirectory of the root whose `.bare` is a sibling.
    if let Some(root) = find_ancestor_root(cwd) {
        if Layout::from_common_dir(&common).is_none() {
            return Ok(Layout::new(root));
        }
    }

    Layout::from_common_dir(&common).ok_or_else(|| Error::Layout(wrong_layout(cwd, &common)))
}

/// Accept a root, a `.bare` directory, or either with a trailing slash.
fn normalize_root(root: &Path) -> Result<PathBuf, Error> {
    if root.ends_with(".bare") {
        return root
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| Error::Usage(format!("{} has no parent directory", root.display())));
    }
    Ok(root.to_path_buf())
}

/// Walk up looking for a directory that contains `.bare/HEAD`.
fn find_ancestor_root(cwd: &Path) -> Option<PathBuf> {
    cwd.ancestors()
        .find(|dir| dir.join(".bare").join("HEAD").is_file())
        .map(std::path::Path::to_path_buf)
}

fn not_a_repository(cwd: &Path) -> String {
    format!(
        "no git repository here.\n\
         \n\
         \x20 cwd:     {}\n\
         \x20 searched: this directory, its parents, and git's own discovery\n\
         \n\
         gw works inside a bare-repo layout:\n\
         \n\
         \x20 <root>/.bare/              the bare repository\n\
         \x20 <root>/worktrees/<branch>/ one checkout per branch\n\
         \n\
         Run gw from any of those, or point it at one:\n\
         \n\
         \x20 gw --repo <root> status\n\
         \x20 GW_REPO=<root> gw status\n\
         \n\
         To create this layout for a project:\n\
         \n\
         \x20 gw init <remote-url> --dir <root>",
        cwd.display()
    )
}

fn wrong_layout(cwd: &Path, common: &Path) -> String {
    format!(
        "this checkout uses a different layout.\n\
         \n\
         \x20 cwd:     {}\n\
         \x20 git dir: {}\n\
         \n\
         gw expects the bare repository at <root>/.bare, with the current\n\
         checkout at <root>/worktrees/<branch>. This looks like a normal\n\
         clone (a .git directory in the project itself).\n\
         \n\
         Point gw at an existing layout instead:\n\
         \n\
         \x20 gw --repo <root> status\n\
         \n\
         Or create one for this project:\n\
         \n\
         \x20 gw init <remote-url> --dir <root>",
        cwd.display(),
        common.display()
    )
}

/// Derive `owner/name` from a GitHub remote URL.
pub fn repo_slug(remote: &str) -> Option<String> {
    let trimmed = remote.trim().trim_end_matches(".git");
    // git@github.com:owner/name  or  https://github.com/owner/name
    let after_host = trimmed.rsplit_once("github.com").map(|(_, rest)| rest)?;
    let slug = after_host.trim_start_matches([':', '/']);
    let mut parts = slug.split('/').filter(|p| !p.is_empty());
    let owner = parts.next()?;
    let name = parts.next()?;
    Some(format!("{owner}/{name}"))
}

/// Path of a worktree relative to the repo root, for display.
pub fn display_path(path: &Path, root: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(rel) => rel.display().to_string(),
        Err(_) => path.display().to_string(),
    }
}

pub fn now_epoch() -> i64 {
    // Saturate rather than wrap: a clock past year 292 billion is not a
    // realistic case, and a negative epoch would invert every age comparison.
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn slug_from_ssh_remote() {
        assert_eq!(
            repo_slug("git@github.com:FlockFreight/platform.git").as_deref(),
            Some("FlockFreight/platform")
        );
    }

    #[test]
    fn slug_from_https_remote() {
        assert_eq!(
            repo_slug("https://github.com/FlockFreight/platform.git").as_deref(),
            Some("FlockFreight/platform")
        );
        assert_eq!(
            repo_slug("https://github.com/FlockFreight/platform").as_deref(),
            Some("FlockFreight/platform")
        );
    }

    #[test]
    fn slug_rejects_non_github() {
        assert!(repo_slug("https://gitlab.com/owner/name.git").is_none());
    }

    #[test]
    fn explicit_bare_path_uses_parent() {
        let layout =
            resolve_layout(Some(Path::new("/repo/.bare")), Path::new("/x"), &NoGit).unwrap();
        assert_eq!(layout.root, PathBuf::from("/repo"));
    }

    #[test]
    fn explicit_root_is_used_directly() {
        let layout = resolve_layout(Some(Path::new("/repo")), Path::new("/x"), &NoGit).unwrap();
        assert_eq!(layout.bare, PathBuf::from("/repo/.bare"));
    }

    struct NoGit;

    impl GitBackend for NoGit {
        fn common_dir(&self, _: &Path) -> Result<PathBuf, Error> {
            Err(Error::Other("unused".into()))
        }
        fn worktree_list(&self) -> Result<Vec<crate::git::WorktreeEntry>, Error> {
            unimplemented!()
        }
        fn status(&self, _: &Path) -> Result<gw_core::Dirty, Error> {
            unimplemented!()
        }
        fn rev_list_remotes(&self) -> Result<Vec<gw_core::Sha>, Error> {
            unimplemented!()
        }
        fn for_each_branch(&self, _: i64) -> Result<Vec<crate::git::backend::BranchEntry>, Error> {
            unimplemented!()
        }
        fn default_branch(&self) -> Result<Option<gw_core::BranchName>, Error> {
            unimplemented!()
        }
        fn head_branch(&self) -> Result<Option<gw_core::BranchName>, Error> {
            unimplemented!()
        }
        fn ref_exists(&self, _: &gw_core::BranchName) -> Result<bool, Error> {
            unimplemented!()
        }
        fn remote_ref_exists(&self, _: &gw_core::BranchName) -> Result<bool, Error> {
            unimplemented!()
        }
        fn fetch_prune(&self) -> Result<(), Error> {
            unimplemented!()
        }
        fn fast_forward_to_remote(&self, _: &gw_core::BranchName) -> Result<(), Error> {
            unimplemented!()
        }
        fn worktree_add(
            &self,
            _: &Path,
            _: &gw_core::BranchName,
            _: crate::git::AddMode,
        ) -> Result<(), Error> {
            unimplemented!()
        }
        fn worktree_remove(&self, _: &Path) -> Result<(), Error> {
            unimplemented!()
        }
        fn worktree_prune(&self) -> Result<(), Error> {
            unimplemented!()
        }
        fn branch_delete(&self, _: &gw_core::BranchName) -> Result<(), Error> {
            unimplemented!()
        }
        fn remote_url(&self) -> Result<Option<String>, Error> {
            unimplemented!()
        }
        fn last_commit_epoch(&self, _: &Path) -> Result<Option<i64>, Error> {
            unimplemented!()
        }
    }
}
