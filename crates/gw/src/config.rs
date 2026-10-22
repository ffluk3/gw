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
        if !crate::repos::is_layout(&root) {
            return Err(Error::Usage(no_layout_at(&root)));
        }
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
///
/// The result is absolute and, when it exists, symlink-resolved. Worktree
/// paths are passed back to git, so a relative `--repo` would otherwise be
/// re-interpreted against the bare repository instead of the caller's cwd.
fn normalize_root(root: &Path) -> Result<PathBuf, Error> {
    let root = if root.ends_with(".bare") {
        root.parent()
            .ok_or_else(|| Error::Usage(format!("{} has no parent directory", root.display())))?
    } else {
        root
    };
    let absolute = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()?.join(root)
    };
    Ok(std::fs::canonicalize(&absolute).unwrap_or(absolute))
}

fn no_layout_at(root: &Path) -> String {
    let mut message = format!("{} is not a gw layout (no .bare/HEAD).", root.display());
    if root.is_dir() {
        let layouts = crate::repos::layouts_under(root);
        if !layouts.is_empty() {
            message.push_str("\n\nLayouts under this directory:");
            for slug in layouts.iter().take(20) {
                message.push_str(&format!("\n  {slug}"));
            }
            message.push_str("\n\nPick one with --repo <name>.");
        }
    }
    message
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
    use super::coverage_tests::FakeGit;
    use super::*;

    #[test]
    fn slug_from_ssh_remote() {
        assert_eq!(
            repo_slug("git@github.com:acme/widgets.git").as_deref(),
            Some("acme/widgets")
        );
    }

    #[test]
    fn slug_from_https_remote() {
        assert_eq!(
            repo_slug("https://github.com/acme/widgets.git").as_deref(),
            Some("acme/widgets")
        );
        assert_eq!(
            repo_slug("https://github.com/acme/widgets").as_deref(),
            Some("acme/widgets")
        );
    }

    #[test]
    fn slug_rejects_non_github() {
        assert!(repo_slug("https://gitlab.com/owner/name.git").is_none());
    }

    fn layout_root() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let bare = dir.path().join(".bare");
        std::fs::create_dir_all(&bare).unwrap();
        std::fs::write(bare.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        dir
    }

    #[test]
    fn explicit_bare_path_uses_parent() {
        let dir = layout_root();
        let root = std::fs::canonicalize(dir.path()).unwrap();

        let layout = resolve_layout(
            Some(&root.join(".bare")),
            Path::new("/x"),
            &FakeGit::default(),
        )
        .unwrap();
        assert_eq!(layout.root, root);
    }

    #[test]
    fn explicit_root_is_used_directly() {
        let dir = layout_root();
        let root = std::fs::canonicalize(dir.path()).unwrap();

        let layout = resolve_layout(Some(&root), Path::new("/x"), &FakeGit::default()).unwrap();
        assert_eq!(layout.bare, root.join(".bare"));
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::*;

    /// A backend with no repository facts. Every method is unused by
    /// `resolve_layout` except `common_dir`, so those are `unreachable!()`
    /// rather than `unimplemented!()`: they are never part of the path under
    /// test and should not count against coverage.
    #[derive(Default)]
    pub(super) struct FakeGit {
        common: Option<PathBuf>,
    }

    impl FakeGit {
        pub(super) fn with_common_dir(path: PathBuf) -> Self {
            Self { common: Some(path) }
        }
    }

    impl GitBackend for FakeGit {
        fn common_dir(&self, _: &Path) -> Result<PathBuf, Error> {
            match &self.common {
                Some(path) => Ok(path.clone()),
                None => Err(Error::Other("unused".into())),
            }
        }
        fn worktree_list(&self) -> Result<Vec<crate::git::WorktreeEntry>, Error> {
            unreachable!("resolve_layout does not list worktrees")
        }
        fn status(&self, _: &Path) -> Result<gw_core::Dirty, Error> {
            unreachable!("resolve_layout does not read status")
        }
        fn rev_list_remotes(&self) -> Result<Vec<gw_core::Sha>, Error> {
            unreachable!("resolve_layout does not list remotes")
        }
        fn for_each_branch(&self, _: i64) -> Result<Vec<crate::git::backend::BranchEntry>, Error> {
            unreachable!("resolve_layout does not list branches")
        }
        fn default_branch(&self) -> Result<Option<gw_core::BranchName>, Error> {
            unreachable!("resolve_layout does not read the default branch")
        }
        fn head_branch(&self) -> Result<Option<gw_core::BranchName>, Error> {
            unreachable!("resolve_layout does not read HEAD")
        }
        fn ref_exists(&self, _: &gw_core::BranchName) -> Result<bool, Error> {
            unreachable!("resolve_layout does not read refs")
        }
        fn remote_ref_exists(&self, _: &gw_core::BranchName) -> Result<bool, Error> {
            unreachable!("resolve_layout does not read refs")
        }
        fn fetch_prune(&self) -> Result<(), Error> {
            unreachable!("resolve_layout does not fetch")
        }
        fn fast_forward_to_remote(&self, _: &gw_core::BranchName) -> Result<(), Error> {
            unreachable!("resolve_layout does not mutate branches")
        }
        fn worktree_add(
            &self,
            _: &Path,
            _: &gw_core::BranchName,
            _: crate::git::AddMode,
        ) -> Result<(), Error> {
            unreachable!("resolve_layout does not add worktrees")
        }
        fn worktree_remove(&self, _: &Path) -> Result<(), Error> {
            unreachable!("resolve_layout does not remove worktrees")
        }
        fn worktree_prune(&self) -> Result<(), Error> {
            unreachable!("resolve_layout does not prune worktrees")
        }
        fn branch_delete(&self, _: &gw_core::BranchName) -> Result<(), Error> {
            unreachable!("resolve_layout does not delete branches")
        }
        fn remote_url(&self) -> Result<Option<String>, Error> {
            unreachable!("resolve_layout does not read the remote URL")
        }
        fn last_commit_epoch(&self, _: &Path) -> Result<Option<i64>, Error> {
            unreachable!("resolve_layout does not read commit dates")
        }
    }

    fn layout_root() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let bare = dir.path().join(".bare");
        std::fs::create_dir_all(&bare).unwrap();
        std::fs::write(bare.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        dir
    }

    #[test]
    fn a_root_without_a_parent_is_a_usage_error() {
        assert!(matches!(
            resolve_layout(
                Some(Path::new(".bare")),
                Path::new("/x"),
                &FakeGit::default()
            ),
            Err(Error::Usage(_))
        ));
    }

    #[test]
    fn standing_in_the_layout_root_is_recognized() {
        let dir = layout_root();
        let layout = resolve_layout(None, dir.path(), &FakeGit::default()).unwrap();
        assert_eq!(layout.root, dir.path());
    }

    #[test]
    fn a_non_layout_directory_names_its_layouts() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("acme/widgets");
        std::fs::create_dir_all(nested.join(".bare")).unwrap();
        std::fs::write(nested.join(".bare/HEAD"), "ref: refs/heads/main\n").unwrap();
        let elsewhere = dir.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();

        let error = resolve_layout(Some(dir.path()), &elsewhere, &FakeGit::default()).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("is not a gw layout"), "{message}");
        assert!(message.contains("acme/widgets"), "{message}");
        assert!(message.contains("--repo <name>"), "{message}");
    }

    #[test]
    fn a_directory_that_is_not_a_repository_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let error = resolve_layout(None, dir.path(), &FakeGit::default()).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("no git repository here"), "{message}");
        assert!(message.contains("gw init"), "{message}");
    }

    #[test]
    fn a_plain_clone_is_rejected_with_a_way_forward() {
        let clone = tempfile::tempdir().unwrap();
        let common = clone.path().join(".git");
        std::fs::create_dir_all(&common).unwrap();
        let error =
            resolve_layout(None, clone.path(), &FakeGit::with_common_dir(common)).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("different layout"), "{message}");
        assert!(message.contains("gw --repo"), "{message}");
    }

    #[test]
    fn an_ancestor_layout_root_is_found_from_a_subdirectory() {
        let dir = layout_root();
        // The subdirectory is not itself a repo, and `common_dir` fails; the
        // ancestor containing `.bare/HEAD` is the answer.
        let sub = dir.path().join("notes");
        std::fs::create_dir_all(&sub).unwrap();
        let layout = resolve_layout(None, &sub, &FakeGit::default()).unwrap();
        assert_eq!(layout.root, dir.path());
    }

    #[test]
    fn display_path_is_relative_when_inside_the_root() {
        assert_eq!(
            display_path(Path::new("/repo/worktrees/x"), Path::new("/repo")),
            "worktrees/x"
        );
        assert_eq!(
            display_path(Path::new("/elsewhere/x"), Path::new("/repo")),
            "/elsewhere/x"
        );
    }
}
