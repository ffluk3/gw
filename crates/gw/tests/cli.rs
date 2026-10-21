//! End-to-end tests against real git repositories.
//!
//! Real git rather than a mock: the behavior worth protecting is git's own
//! worktree semantics, and a mock would encode our assumptions instead.

use std::path::{Path, PathBuf};
use std::process::Command;

struct Fixture {
    _tmp: tempfile::TempDir,
    root: PathBuf,
}

impl Fixture {
    /// Build the layout `worktree_setup.sh` creates: a bare repo at
    /// `<root>/.bare` cloned from a separate origin, with a `develop` worktree
    /// at `<root>/worktrees/develop`.
    ///
    /// Origin must be a distinct repository. Pointing it at the bare repo
    /// itself would make `git fetch` mirror every local branch into
    /// `refs/remotes/*`, so nothing would ever look unpublished.
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().to_path_buf();
        let remote = root.join("remote.git");
        let bare = root.join(".bare");

        run(
            &root,
            &["init", "--bare", "-b", "develop", remote.to_str().unwrap()],
        );

        // Seed a commit on develop so worktrees have a base.
        let seed = root.join("seed");
        run(&root, &["init", "-b", "develop", seed.to_str().unwrap()]);
        run(&seed, &["config", "user.email", "test@example.com"]);
        run(&seed, &["config", "user.name", "Test"]);
        std::fs::write(seed.join("README.md"), "seed\n").unwrap();
        run(&seed, &["add", "."]);
        run(&seed, &["commit", "-m", "seed"]);
        run(&seed, &["push", remote.to_str().unwrap(), "develop"]);
        std::fs::remove_dir_all(&seed).unwrap();

        run(
            &root,
            &[
                "clone",
                "--bare",
                remote.to_str().unwrap(),
                bare.to_str().unwrap(),
            ],
        );
        run(
            &bare,
            &[
                "config",
                "remote.origin.fetch",
                "+refs/heads/*:refs/remotes/origin/*",
            ],
        );
        run(&bare, &["config", "user.email", "test@example.com"]);
        run(&bare, &["config", "user.name", "Test"]);
        run(&bare, &["fetch", "origin"]);

        std::fs::create_dir_all(root.join("worktrees")).unwrap();
        run(
            &bare,
            &[
                "worktree",
                "add",
                root.join("worktrees/develop").to_str().unwrap(),
                "develop",
            ],
        );

        Self { _tmp: tmp, root }
    }

    /// Publish a branch to origin so it counts as reachable.
    fn publish(&self, branch: &str) {
        run(
            &self.bare(),
            &["push", "origin", &format!("{branch}:{branch}")],
        );
        run(&self.bare(), &["fetch", "origin"]);
    }

    fn bare(&self) -> PathBuf {
        self.root.join(".bare")
    }

    fn add_worktree(&self, branch: &str) -> PathBuf {
        let path = self.root.join("worktrees").join(branch);
        run(
            &self.bare(),
            &["worktree", "add", "-b", branch, path.to_str().unwrap()],
        );
        path
    }

    fn gw(&self, args: &[&str]) -> Output {
        self.gw_in(&self.bare(), args)
    }

    fn gw_in(&self, cwd: &Path, args: &[&str]) -> Output {
        let out = Command::new(env!("CARGO_BIN_EXE_gw"))
            .current_dir(cwd)
            .args(args)
            .env_remove("GW_JIRA_BASE_URL")
            .env_remove("GW_JIRA_USER")
            .env_remove("GW_JIRA_API_TOKEN")
            .env("GW_REPO", self.root.to_str().unwrap())
            .output()
            .expect("run gw");
        Output {
            code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }
}

struct Output {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run(cwd: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn status_lists_worktrees_and_skips_the_bare_entry() {
    let fx = Fixture::new();
    fx.add_worktree("AX-1");

    let out = fx.gw(&["status", "--json"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).expect("valid json");
    let worktrees = json["worktrees"].as_array().expect("array");

    let branches: Vec<&str> = worktrees
        .iter()
        .map(|w| w["branch"].as_str().unwrap_or(""))
        .collect();
    assert!(branches.contains(&"develop"), "got {branches:?}");
    assert!(branches.contains(&"AX-1"), "got {branches:?}");
    assert!(
        !branches.iter().any(|b| b.is_empty()),
        "bare entry leaked into status: {branches:?}"
    );
}

#[test]
fn clean_dry_run_changes_nothing() {
    let fx = Fixture::new();
    let wt = fx.add_worktree("AX-2");

    let out = fx.gw(&["clean", "--dry-run", "--skip-github"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(wt.is_dir(), "dry run removed the worktree");
}

#[test]
fn clean_refuses_to_remove_a_dirty_worktree() {
    let fx = Fixture::new();
    let wt = fx.add_worktree("AX-3");
    fx.publish("AX-3");
    std::fs::write(wt.join("scratch.txt"), "uncommitted\n").unwrap();

    let out = fx.gw(&["clean", "--yes", "--skip-github", "--stale-days", "0"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(wt.is_dir(), "removed a worktree with uncommitted changes");
    assert!(
        out.stdout.contains("uncommitted"),
        "dirty worktree not reported: {}",
        out.stdout
    );
}

#[test]
fn clean_refuses_to_remove_unpublished_commits() {
    let fx = Fixture::new();
    let wt = fx.add_worktree("AX-4");
    // Deliberately not published: the tip exists only locally.
    std::fs::write(wt.join("work.txt"), "a commit\n").unwrap();
    run(&wt, &["add", "."]);
    run(&wt, &["commit", "-m", "unpublished work"]);

    let out = fx.gw(&["clean", "--yes", "--skip-github", "--stale-days", "0"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(wt.is_dir(), "removed a worktree with unpublished commits");
    assert!(
        out.stdout.contains("unpublished"),
        "unpublished branch not reported: {}",
        out.stdout
    );
}

#[test]
fn clean_removes_a_clean_published_worktree() {
    let fx = Fixture::new();
    let wt = fx.add_worktree("AX-8");
    fx.publish("AX-8");

    let out = fx.gw(&["clean", "--yes", "--skip-github", "--stale-days", "0"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(!wt.exists(), "clean worktree was not removed");
}

#[test]
fn clean_never_touches_the_default_branch_worktree() {
    let fx = Fixture::new();
    let develop = fx.root.join("worktrees/develop");

    let out = fx.gw(&["clean", "--yes", "--skip-github", "--stale-days", "0"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(develop.is_dir(), "removed the default branch worktree");
}

#[test]
fn add_reuses_an_existing_worktree() {
    let fx = Fixture::new();
    fx.add_worktree("AX-5");

    let out = fx.gw(&["add", "AX-5", "--print-path", "--no-fetch"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let printed = out.stdout.trim();
    assert!(
        printed.ends_with("worktrees/AX-5"),
        "unexpected path: {printed}"
    );
}

#[test]
fn add_creates_a_new_branch_worktree() {
    let fx = Fixture::new();

    let out = fx.gw(&["add", "AX-6", "--print-path", "--no-fetch"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        fx.root.join("worktrees/AX-6").is_dir(),
        "worktree was not created"
    );
}

#[test]
fn add_rejects_an_invalid_branch_name() {
    let fx = Fixture::new();
    let out = fx.gw(&["add", "bad\"name", "--no-fetch"]);
    assert_eq!(out.code, 2, "expected a usage error, got {}", out.code);
}

#[test]
fn layout_discovery_works_from_inside_a_worktree() {
    let fx = Fixture::new();
    let wt = fx.add_worktree("AX-7");

    // No GW_REPO: discovery must find the bare repo from the worktree itself.
    let out = Command::new(env!("CARGO_BIN_EXE_gw"))
        .current_dir(&wt)
        .args(["status", "--json"])
        .env_remove("GW_REPO")
        .output()
        .expect("run gw");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("valid json from discovered layout");
    assert!(!json["worktrees"].as_array().unwrap().is_empty());
}

#[test]
fn missing_repository_is_a_clear_usage_error() {
    let tmp = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_gw"))
        .current_dir(tmp.path())
        .args(["status"])
        .env_remove("GW_REPO")
        .output()
        .expect("run gw");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("GW_REPO"),
        "error should name the escape hatch: {stderr}"
    );
}

#[test]
fn harness_worktrees_are_never_removed() {
    let fx = Fixture::new();
    let harness = fx.bare().join(".claude/worktrees/agent-test");
    std::fs::create_dir_all(harness.parent().unwrap()).unwrap();
    run(
        &fx.bare(),
        &[
            "worktree",
            "add",
            "-b",
            "worktree-agent-test",
            harness.to_str().unwrap(),
        ],
    );

    let out = fx.gw(&["clean", "--yes", "--skip-github", "--stale-days", "0"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(harness.is_dir(), "removed a harness worktree");
}

// ---- working-directory contract -------------------------------------------

#[test]
fn works_from_the_layout_root() {
    let fx = Fixture::new();
    // The layout root is not itself a git repository.
    let out = Command::new(env!("CARGO_BIN_EXE_gw"))
        .current_dir(&fx.root)
        .args(["status", "--json"])
        .env_remove("GW_REPO")
        .output()
        .expect("run gw");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn works_from_the_bare_repository() {
    let fx = Fixture::new();
    let out = Command::new(env!("CARGO_BIN_EXE_gw"))
        .current_dir(fx.bare())
        .args(["status", "--json"])
        .env_remove("GW_REPO")
        .output()
        .expect("run gw");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn works_from_a_subdirectory_of_the_layout_root() {
    let fx = Fixture::new();
    let nested = fx.root.join("scratch");
    std::fs::create_dir_all(&nested).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_gw"))
        .current_dir(&nested)
        .args(["status", "--json"])
        .env_remove("GW_REPO")
        .output()
        .expect("run gw");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_plain_clone_is_reported_as_the_wrong_layout() {
    let tmp = tempfile::tempdir().unwrap();
    let plain = tmp.path().join("plain");
    run(tmp.path(), &["init", "-b", "main", plain.to_str().unwrap()]);

    let out = Command::new(env!("CARGO_BIN_EXE_gw"))
        .current_dir(&plain)
        .args(["status"])
        .env_remove("GW_REPO")
        .output()
        .expect("run gw");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("different layout"),
        "should explain the mismatch: {stderr}"
    );
    assert!(
        stderr.contains("gw init"),
        "should name the way to create a layout: {stderr}"
    );
}

// ---- gw init ---------------------------------------------------------------

/// A source repository to clone from.
fn source_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("source");
    run(dir, &["init", "-b", "main", repo.to_str().unwrap()]);
    run(&repo, &["config", "user.email", "test@example.com"]);
    run(&repo, &["config", "user.name", "Test"]);
    std::fs::write(repo.join("README.md"), "hello\n").unwrap();
    run(&repo, &["add", "."]);
    run(&repo, &["commit", "-m", "initial"]);
    repo
}

fn gw_raw(cwd: &Path, args: &[&str]) -> Output {
    let out = Command::new(env!("CARGO_BIN_EXE_gw"))
        .current_dir(cwd)
        .args(args)
        .env_remove("GW_REPO")
        .output()
        .expect("run gw");
    Output {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

#[test]
fn init_creates_a_working_layout() {
    let tmp = tempfile::tempdir().unwrap();
    let src = source_repo(tmp.path());
    let root = tmp.path().join("project");

    let out = gw_raw(
        tmp.path(),
        &[
            "init",
            src.to_str().unwrap(),
            "--dir",
            root.to_str().unwrap(),
        ],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);

    assert!(root.join(".bare/HEAD").is_file(), "no bare repo");
    assert!(
        root.join("worktrees/main/README.md").is_file(),
        "no default-branch checkout"
    );

    // The created layout must be usable immediately, from inside it.
    let status = gw_raw(&root.join("worktrees/main"), &["status", "--json"]);
    assert_eq!(status.code, 0, "stderr: {}", status.stderr);
    let json: serde_json::Value = serde_json::from_str(&status.stdout).unwrap();
    assert!(!json["worktrees"].as_array().unwrap().is_empty());
}

#[test]
fn init_derives_the_directory_from_the_remote() {
    let tmp = tempfile::tempdir().unwrap();
    let src = source_repo(tmp.path());

    let out = gw_raw(tmp.path(), &["init", src.to_str().unwrap()]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(tmp.path().join("source/.bare/HEAD").is_file());
}

#[test]
fn init_honors_an_explicit_branch() {
    let tmp = tempfile::tempdir().unwrap();
    let src = source_repo(tmp.path());
    // A second branch that is not the default.
    run(&src, &["push", src.to_str().unwrap(), "main:feature/x"]);
    let root = tmp.path().join("proj");

    let out = gw_raw(
        tmp.path(),
        &[
            "init",
            src.to_str().unwrap(),
            "--dir",
            root.to_str().unwrap(),
            "--branch",
            "feature/x",
        ],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        root.join("worktrees/feature/x/README.md").is_file(),
        "slash branch checkout missing"
    );
}

#[test]
fn init_dry_run_writes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let src = source_repo(tmp.path());
    let root = tmp.path().join("proj");

    let out = gw_raw(
        tmp.path(),
        &[
            "init",
            src.to_str().unwrap(),
            "--dir",
            root.to_str().unwrap(),
            "--dry-run",
        ],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(!root.exists(), "dry run created the layout");
}

#[test]
fn init_refuses_to_clobber_an_existing_layout() {
    let tmp = tempfile::tempdir().unwrap();
    let src = source_repo(tmp.path());
    let root = tmp.path().join("proj");

    let first = gw_raw(
        tmp.path(),
        &[
            "init",
            src.to_str().unwrap(),
            "--dir",
            root.to_str().unwrap(),
        ],
    );
    assert_eq!(first.code, 0, "stderr: {}", first.stderr);

    let second = gw_raw(
        tmp.path(),
        &[
            "init",
            src.to_str().unwrap(),
            "--dir",
            root.to_str().unwrap(),
        ],
    );
    assert_eq!(second.code, 2, "expected a usage error");
    assert!(
        second.stderr.contains("already a gw layout"),
        "stderr: {}",
        second.stderr
    );
}
