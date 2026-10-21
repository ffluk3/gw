use std::path::{Path, PathBuf};
use std::process::Command;

use gw_core::{BranchName, Dirty, Sha};

use crate::error::Error;

use super::backend::{AddMode, BranchEntry, GitBackend, TrackStart};
use super::porcelain::parse_worktree_list;

/// The real backend: shells out to `git`.
///
/// Deliberately not libgit2. Worktree support in libgit2 is partial and drifts
/// from the CLI's behavior, and these commands were all validated against the
/// CLI's semantics.
pub struct CliGit {
    /// The bare repository directory. All commands run here unless they need a
    /// specific worktree.
    bare: PathBuf,
}

impl CliGit {
    pub fn new(bare: PathBuf) -> Self {
        Self { bare }
    }

    fn run(&self, args: &[&str]) -> Result<String, Error> {
        Self::run_in(&self.bare, args)
    }

    fn run_in(cwd: &Path, args: &[&str]) -> Result<String, Error> {
        let output = Command::new("git")
            .current_dir(cwd)
            .args(["-C", &cwd.to_string_lossy()])
            .args(args)
            .output()
            .map_err(|e| Error::Git {
                context: format!("failed to run git {}", args.join(" ")),
                stderr: e.to_string(),
            })?;

        if !output.status.success() {
            return Err(Error::Git {
                context: format!("git {}", args.join(" ")),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// Run a command and treat failure as an ordinary outcome.
    fn run_optional(cwd: &Path, args: &[&str]) -> Option<String> {
        let output = Command::new("git")
            .args(["-C", &cwd.to_string_lossy()])
            .args(args)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

impl GitBackend for CliGit {
    fn common_dir(&self, cwd: &Path) -> Result<PathBuf, Error> {
        let out = Self::run_in(
            cwd,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?;
        Ok(PathBuf::from(out.trim()))
    }

    fn worktree_list(&self) -> Result<Vec<super::backend::WorktreeEntry>, Error> {
        let out = self.run(&["worktree", "list", "--porcelain"])?;
        Ok(parse_worktree_list(&out))
    }

    fn status(&self, path: &Path) -> Result<Dirty, Error> {
        // A non-zero exit means the probe failed, not that the tree is clean.
        // Reporting `Unknown` makes the classifier fail closed.
        match Self::run_optional(path, &["status", "--porcelain"]) {
            Some(out) if out.trim().is_empty() => Ok(Dirty::Clean),
            Some(_) => Ok(Dirty::Dirty),
            None => Ok(Dirty::Unknown),
        }
    }

    fn rev_list_remotes(&self) -> Result<Vec<Sha>, Error> {
        let out = self.run(&["rev-list", "--remotes"])?;
        Ok(out
            .lines()
            .filter_map(|line| Sha::parse(line).ok())
            .collect())
    }

    fn for_each_branch(&self, now_epoch: i64) -> Result<Vec<BranchEntry>, Error> {
        let out = self.run(&[
            "for-each-ref",
            "--format=%(refname:short)%09%(worktreepath)%09%(objectname)%09%(committerdate:unix)",
            "refs/heads",
        ])?;

        let mut entries = Vec::new();
        for line in out.lines() {
            let mut fields = line.split('\t');
            let Some(name) = fields.next() else { continue };
            let Some(worktree) = fields.next() else {
                continue;
            };
            let Some(tip) = fields.next() else { continue };
            let Some(commit_epoch) = fields.next() else {
                continue;
            };

            let Ok(name) = BranchName::parse(name) else {
                continue;
            };
            let worktree = if worktree.is_empty() {
                None
            } else {
                Some(PathBuf::from(worktree))
            };
            let tip = Sha::parse(tip).ok();
            let commit_age_secs = commit_epoch
                .trim()
                .parse::<i64>()
                .ok()
                .map(|epoch| (now_epoch - epoch).max(0));

            entries.push(BranchEntry {
                name,
                worktree,
                tip,
                commit_age_secs,
            });
        }
        Ok(entries)
    }

    fn default_branch(&self) -> Result<Option<BranchName>, Error> {
        let out = Self::run_optional(
            &self.bare,
            &[
                "symbolic-ref",
                "--quiet",
                "--short",
                "refs/remotes/origin/HEAD",
            ],
        );
        Ok(out
            .and_then(|s| s.trim().strip_prefix("origin/").map(str::to_string))
            .and_then(|name| BranchName::parse(&name).ok()))
    }

    fn head_branch(&self) -> Result<Option<BranchName>, Error> {
        let out = Self::run_optional(&self.bare, &["symbolic-ref", "--quiet", "--short", "HEAD"]);
        Ok(out.and_then(|s| BranchName::parse(s.trim()).ok()))
    }

    fn ref_exists(&self, branch: &BranchName) -> Result<bool, Error> {
        let spec = format!("refs/heads/{}", branch.as_str());
        let out = Command::new("git")
            .args(["-C", &self.bare.to_string_lossy()])
            .args(["show-ref", "--verify", "--quiet", &spec])
            .status()
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(out.success())
    }

    fn remote_ref_exists(&self, branch: &BranchName) -> Result<bool, Error> {
        let spec = format!("refs/remotes/origin/{}", branch.as_str());
        let out = Command::new("git")
            .args(["-C", &self.bare.to_string_lossy()])
            .args(["show-ref", "--verify", "--quiet", &spec])
            .status()
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(out.success())
    }

    fn fetch_prune(&self) -> Result<(), Error> {
        self.run(&["fetch", "--prune", "--quiet"])?;
        Ok(())
    }

    fn fast_forward_to_remote(&self, branch: &BranchName) -> Result<(), Error> {
        // A branch checked out in another worktree cannot be moved; surface
        // that instead of swallowing it as the shell version did.
        self.run(&[
            "branch",
            "-f",
            branch.as_str(),
            &format!("origin/{}", branch.as_str()),
        ])?;
        Ok(())
    }

    fn worktree_add(&self, path: &Path, branch: &BranchName, mode: AddMode) -> Result<(), Error> {
        let path_str = path.to_string_lossy().into_owned();
        match mode {
            AddMode::Existing => {
                self.run(&["worktree", "add", &path_str, branch.as_str()])?;
            }
            AddMode::Track {
                start: TrackStart::Remote,
            } => {
                self.run(&[
                    "worktree",
                    "add",
                    "--track",
                    "-b",
                    branch.as_str(),
                    &path_str,
                    &format!("origin/{}", branch.as_str()),
                ])?;
            }
            AddMode::Track {
                start: TrackStart::Head,
            } => {
                self.run(&["worktree", "add", "-b", branch.as_str(), &path_str])?;
            }
        }
        Ok(())
    }

    fn worktree_remove(&self, path: &Path) -> Result<(), Error> {
        self.run(&["worktree", "remove", &path.to_string_lossy()])?;
        Ok(())
    }

    fn worktree_prune(&self) -> Result<(), Error> {
        self.run(&["worktree", "prune", "-v"])?;
        Ok(())
    }

    fn branch_delete(&self, branch: &BranchName) -> Result<(), Error> {
        self.run(&["branch", "-D", branch.as_str()])?;
        Ok(())
    }

    fn remote_url(&self) -> Result<Option<String>, Error> {
        Ok(
            Self::run_optional(&self.bare, &["remote", "get-url", "origin"])
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        )
    }

    fn last_commit_epoch(&self, path: &Path) -> Result<Option<i64>, Error> {
        Ok(Self::run_optional(path, &["log", "-1", "--format=%ct"])
            .and_then(|s| s.trim().parse::<i64>().ok()))
    }
}
