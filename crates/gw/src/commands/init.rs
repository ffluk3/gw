use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::InitArgs;
use crate::error::Error;
use crate::output::Outcome;

/// Create the bare-repo layout from a remote.
///
/// This is the one command that does not require an existing layout, so it does
/// not go through `Context`. It is the replacement for `worktree_setup.sh`, with
/// the repository, branch, and destination all explicit instead of hardcoded.
pub fn run(args: InitArgs) -> Result<Outcome, Error> {
    let root = match args.dir {
        Some(dir) => dir,
        None => default_root(&args.remote)?,
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

/// `owner/name` or `name` → `./name`, stripping `.git`.
fn default_root(remote: &str) -> Result<PathBuf, Error> {
    let trimmed = remote.trim().trim_end_matches('/').trim_end_matches(".git");
    let name = trimmed
        .rsplit(['/', ':'])
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            Error::Usage(format!(
                "cannot derive a directory name from '{remote}'; pass --dir"
            ))
        })?;
    Ok(PathBuf::from(name))
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

    #[test]
    fn default_root_from_ssh_url() {
        assert_eq!(
            default_root("git@github.com:owner/thing.git").unwrap(),
            PathBuf::from("thing")
        );
    }

    #[test]
    fn default_root_from_https_url() {
        assert_eq!(
            default_root("https://github.com/owner/thing.git").unwrap(),
            PathBuf::from("thing")
        );
        assert_eq!(
            default_root("https://github.com/owner/thing/").unwrap(),
            PathBuf::from("thing")
        );
    }

    #[test]
    fn default_root_from_local_path() {
        assert_eq!(
            default_root("/srv/git/thing.git").unwrap(),
            PathBuf::from("thing")
        );
    }

    #[test]
    fn default_root_rejects_empty() {
        assert!(default_root("/").is_err());
    }
}
