use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::InitArgs;
use crate::error::Error;
use crate::output::Outcome;

/// Create the bare-repo layout from a remote.
///
/// This is the one command that does not require an existing layout, so it does
/// not go through `Context`. The repository, branch, and destination are all
/// explicit instead of hardcoded.
pub fn run(args: InitArgs) -> Result<Outcome, Error> {
    let root = match args.dir {
        Some(dir) => dir,
        None => default_root(&args.remote, &crate::repos::repos_root())?,
    };
    let root = absolute(&root)?;

    let bare = root.join(".bare");
    let worktrees = root.join("worktrees");

    if bare.join("HEAD").is_file() {
        return Err(Error::Usage(format!(
            "{} is already a gw layout (.bare exists).\n\
             Use it directly:\n\
             \n\
             \x20 gw --repo {} status",
            root.display(),
            root.display()
        )));
    }

    if root.exists() && !root.is_dir() {
        return Err(Error::Usage(format!(
            "{} exists and is not a directory",
            root.display()
        )));
    }

    println!("🏗  Creating worktree layout");
    println!("   remote    : {}", args.remote);
    println!("   root      : {}", root.display());
    println!("   bare repo : {}", bare.display());
    println!("   worktrees : {}", worktrees.display());

    if args.dry_run {
        println!();
        println!("Dry run — nothing written. Re-run without --dry-run to create it.");
        return Ok(Outcome::ok());
    }

    std::fs::create_dir_all(&root)
        .map_err(|e| Error::Other(format!("cannot create {}: {e}", root.display())))?;

    // Clone bare. `--no-checkout` is implied by --bare; this puts every branch
    // in refs/heads, which is what makes `worktree add <branch>` work without
    // a tracking branch dance.
    git(
        &root,
        &["clone", "--bare", &args.remote, bare.to_str().unwrap()],
    )?;

    // Match the existing convention: explicit remote-tracking refspec, so
    // `origin/<branch>` exists and fetch --prune behaves.
    git(
        &bare,
        &[
            "config",
            "remote.origin.fetch",
            "+refs/heads/*:refs/remotes/origin/*",
        ],
    )?;
    git(&bare, &["fetch", "origin", "--prune", "--quiet"])?;

    let branch = match &args.branch {
        Some(b) => b.clone(),
        None => detect_default_branch(&bare)?,
    };
    gw_core::BranchName::parse(&branch)
        .map_err(|e| Error::Usage(format!("invalid branch name '{branch}': {e}")))?;

    std::fs::create_dir_all(&worktrees)
        .map_err(|e| Error::Other(format!("cannot create {}: {e}", worktrees.display())))?;

    let checkout = worktrees.join(&branch);
    git(
        &bare,
        &["worktree", "add", checkout.to_str().unwrap(), &branch],
    )?;
    // The bash original did this separately because the branch already existed
    // in refs/heads after a bare clone; keep the same end state.
    let _ = git(
        &checkout,
        &[
            "branch",
            "--set-upstream-to",
            &format!("origin/{branch}"),
            &branch,
        ],
    );

    println!();
    println!("✅ Layout ready");
    println!();
    println!("Start working:");
    println!("   cd {}", checkout.display());
    println!();
    println!("From anywhere inside the layout, gw finds it automatically:");
    println!("   gw status");
    println!("   gw add <branch>");
    println!("   gw clean --dry-run");
    Ok(Outcome::ok())
}

/// `owner/name` → `<repos>/owner/name`, `name` → `<repos>/name`.
///
/// The owner is only taken from a network remote. A local path such as
/// `/srv/git/thing.git` contributes just its final component, so its parent
/// directory never becomes a bogus owner.
fn default_root(remote: &str, repos_root: &Path) -> Result<PathBuf, Error> {
    let (owner, name) = remote_parts(remote).ok_or_else(|| {
        Error::Usage(format!(
            "cannot derive a directory name from '{remote}'; pass --dir"
        ))
    })?;
    Ok(match owner {
        Some(owner) => repos_root.join(owner).join(name),
        None => repos_root.join(name),
    })
}

/// Split a remote into `(owner, name)`, with the owner present only when the
/// remote is a network URL or scp-style address.
fn remote_parts(remote: &str) -> Option<(Option<String>, String)> {
    let trimmed = remote.trim().trim_end_matches('/').trim_end_matches(".git");
    if trimmed.is_empty() {
        return None;
    }

    let (path, network) = if let Some((_scheme, rest)) = trimmed.split_once("://") {
        let path = rest.split_once('/').map_or("", |(_, path)| path);
        (path, true)
    } else if let Some((host, path)) = trimmed.split_once(':') {
        // scp-like `git@host:owner/name`; a local path has no ':' before a '/'.
        if host.contains('/') {
            (trimmed, false)
        } else {
            (path, true)
        }
    } else {
        (trimmed, false)
    };

    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let name = parts.last()?.to_string();
    let owner = if network && parts.len() >= 2 {
        Some(parts[parts.len() - 2].to_string())
    } else {
        None
    };
    Some((owner, name))
}

fn absolute(path: &Path) -> Result<PathBuf, Error> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    Ok(std::env::current_dir()?.join(path))
}

/// Ask the clone what its default branch is, rather than assuming `main` or
/// `develop`.
fn detect_default_branch(bare: &Path) -> Result<String, Error> {
    let out = Command::new("git")
        .args(["-C", &bare.to_string_lossy()])
        .args([
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ])
        .output()
        .map_err(|e| Error::Other(e.to_string()))?;
    if out.status.success() {
        let name = String::from_utf8_lossy(&out.stdout);
        if let Some(name) = name.trim().strip_prefix("origin/") {
            if !name.is_empty() {
                return Ok(name.to_string());
            }
        }
    }
    Err(Error::Usage(
        "could not determine the remote's default branch; pass --branch".into(),
    ))
}

fn git(cwd: &Path, args: &[&str]) -> Result<(), Error> {
    let out = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .map_err(|e| Error::Git {
            context: "failed to run git".into(),
            stderr: e.to_string(),
        })?;
    if !out.status.success() {
        return Err(Error::Git {
            context: format!("git {}", args.join(" ")),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from("/repos")
    }

    #[test]
    fn default_root_from_ssh_url() {
        assert_eq!(
            default_root("git@github.com:owner/thing.git", &root()).unwrap(),
            PathBuf::from("/repos/owner/thing")
        );
    }

    #[test]
    fn default_root_from_https_url() {
        assert_eq!(
            default_root("https://github.com/owner/thing.git", &root()).unwrap(),
            PathBuf::from("/repos/owner/thing")
        );
        assert_eq!(
            default_root("https://github.com/owner/thing/", &root()).unwrap(),
            PathBuf::from("/repos/owner/thing")
        );
    }

    #[test]
    fn default_root_from_local_path() {
        assert_eq!(
            default_root("/srv/git/thing.git", &root()).unwrap(),
            PathBuf::from("/repos/thing")
        );
    }

    #[test]
    fn default_root_rejects_empty() {
        assert!(default_root("/", &root()).is_err());
    }
}
