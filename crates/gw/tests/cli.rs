//! End-to-end tests against real git repositories.
//!
//! Real git rather than a mock: the behavior worth protecting is git's own
//! worktree semantics, and a mock would encode our assumptions instead.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::json;

struct Fixture {
    _tmp: tempfile::TempDir,
    root: PathBuf,
}

impl Fixture {
    /// Build the conventional layout: a bare repo at
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
        let mut cmd = gw_command(cwd);
        let out = cmd
            .args(args)
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

/// A `gw` process with every ambient `gw` setting removed.
///
/// Without this, a developer's own `GW_REPOS_ROOT`, `~/.gw.toml`, or JIRA
/// credentials would change what the tests observe. The global configuration
/// file is the dangerous one: it may set a convention that makes a fixture
/// branch resolve to a different ticket than the test asserts.
fn gw_command(cwd: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gw"));
    cmd.current_dir(cwd)
        .env_remove("GW_REPO")
        .env_remove("GW_REPOS_ROOT")
        .env_remove("GW_JIRA_PROJECT")
        .env_remove("GW_JIRA_BASE_URL")
        .env_remove("GW_JIRA_USER")
        .env_remove("GW_JIRA_API_TOKEN")
        .env_remove("GW_SKIP_PROVIDERS")
        .env_remove("GW_SKIP_GITHUB")
        .env_remove("GW_CONFIG")
        .env("GW_NO_GLOBAL_CONFIG", "1");
    cmd
}

fn gw_with_env(cwd: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut cmd = gw_command(cwd);
    cmd.args(args);
    for (key, value) in env {
        cmd.env(key, value);
    }
    let out = cmd.output().expect("run gw");
    Output {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
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
    fx.add_worktree("PROJ-1");

    let out = fx.gw(&["status", "--json"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).expect("valid json");
    let worktrees = json["worktrees"].as_array().expect("array");

    let branches: Vec<&str> = worktrees
        .iter()
        .map(|w| w["branch"].as_str().unwrap_or(""))
        .collect();
    assert!(branches.contains(&"develop"), "got {branches:?}");
    assert!(branches.contains(&"PROJ-1"), "got {branches:?}");
    assert!(
        !branches.iter().any(|b| b.is_empty()),
        "bare entry leaked into status: {branches:?}"
    );
}

#[test]
fn clean_dry_run_changes_nothing() {
    let fx = Fixture::new();
    let wt = fx.add_worktree("PROJ-2");

    let out = fx.gw(&["clean", "--dry-run", "--skip-github"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(wt.is_dir(), "dry run removed the worktree");
}

#[test]
fn clean_refuses_to_remove_a_dirty_worktree() {
    let fx = Fixture::new();
    let wt = fx.add_worktree("PROJ-3");
    fx.publish("PROJ-3");
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
    let wt = fx.add_worktree("PROJ-4");
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
    let wt = fx.add_worktree("PROJ-8");
    fx.publish("PROJ-8");

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
    fx.add_worktree("PROJ-5");

    let out = fx.gw(&["add", "PROJ-5", "--print-path", "--no-fetch"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let printed = out.stdout.trim();
    assert!(
        printed.ends_with("worktrees/PROJ-5"),
        "unexpected path: {printed}"
    );
}

#[test]
fn add_creates_a_new_branch_worktree() {
    let fx = Fixture::new();

    let out = fx.gw(&["add", "PROJ-6", "--print-path", "--no-fetch"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        fx.root.join("worktrees/PROJ-6").is_dir(),
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
    let wt = fx.add_worktree("PROJ-7");

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
    gw_with_env(cwd, args, &[])
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

    let out = gw_with_env(
        tmp.path(),
        &["init", src.to_str().unwrap()],
        &[("GW_REPOS_ROOT", tmp.path().to_str().unwrap())],
    );
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

// ---- repos root and name resolution ---------------------------------------

/// Expose a fixture layout under `<repos>/<slug>` without moving it.
fn link_layout(fx: &Fixture, repos: &Path, slug: &str) {
    let dest = repos.join(slug);
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&fx.root, &dest).unwrap();
}

#[test]
fn add_works_from_outside_a_repository_with_repo_path() {
    let fx = Fixture::new();
    let elsewhere = tempfile::tempdir().unwrap();

    let out = gw_with_env(
        elsewhere.path(),
        &[
            "--repo",
            fx.root.to_str().unwrap(),
            "add",
            "PROJ-9",
            "--print-path",
            "--no-fetch",
        ],
        &[],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(fx.root.join("worktrees/PROJ-9").is_dir());
}

#[test]
fn repo_slug_resolves_under_the_repos_root() {
    let fx = Fixture::new();
    let outer = tempfile::tempdir().unwrap();
    let repos = outer.path().join("repos");
    link_layout(&fx, &repos, "acme/widgets");

    let out = gw_with_env(
        outer.path(),
        &["--repo", "acme/widgets", "status", "--json"],
        &[("GW_REPOS_ROOT", repos.to_str().unwrap())],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert!(!json["worktrees"].as_array().unwrap().is_empty());
}

#[test]
fn repo_unique_name_resolves_under_the_repos_root() {
    let fx = Fixture::new();
    let outer = tempfile::tempdir().unwrap();
    let repos = outer.path().join("repos");
    link_layout(&fx, &repos, "acme/widgets");

    let out = gw_with_env(
        outer.path(),
        &["--repo", "widgets", "status", "--json"],
        &[("GW_REPOS_ROOT", repos.to_str().unwrap())],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
}

#[test]
fn repo_pointing_at_a_non_layout_is_a_usage_error() {
    let tmp = tempfile::tempdir().unwrap();
    let out = gw_with_env(
        tmp.path(),
        &["--repo", tmp.path().to_str().unwrap(), "status"],
        &[],
    );
    assert_eq!(out.code, 2, "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("not a gw layout"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn repos_lists_layouts_under_the_root() {
    let fx = Fixture::new();
    let outer = tempfile::tempdir().unwrap();
    let repos = outer.path().join("repos");
    link_layout(&fx, &repos, "acme/widgets");

    let out = gw_with_env(
        outer.path(),
        &["repos", "--json"],
        &[("GW_REPOS_ROOT", repos.to_str().unwrap())],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    let rows = json["repos"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["repo"], "acme/widgets");
    assert!(rows[0]["worktrees"].as_u64().unwrap() >= 1);
}

// ---- configurable JIRA project --------------------------------------------

fn ticket_for<'a>(json: &'a serde_json::Value, branch: &str) -> &'a serde_json::Value {
    json["worktrees"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["branch"] == branch)
        .unwrap_or_else(|| panic!("no row for {branch}"))
        .get("ticket")
        .unwrap()
}

#[test]
fn gw_toml_project_filters_ticket_keys() {
    let fx = Fixture::new();
    std::fs::write(fx.root.join(".gw.toml"), "[jira]\nproject = \"PROJ\"\n").unwrap();
    fx.add_worktree("PROJ-42");
    fx.add_worktree("OTHER-7");

    let out = fx.gw(&["status", "--json"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();

    assert_eq!(ticket_for(&json, "PROJ-42"), "PROJ-42");
    assert!(
        ticket_for(&json, "OTHER-7").is_null(),
        "a non-project key was treated as a ticket"
    );
}

#[test]
fn cli_project_overrides_gw_toml() {
    let fx = Fixture::new();
    std::fs::write(fx.root.join(".gw.toml"), "[jira]\nproject = \"PROJ\"\n").unwrap();
    fx.add_worktree("PROJ-42");
    fx.add_worktree("OTHER-7");

    let out = gw_with_env(
        &fx.bare(),
        &["status", "--json"],
        &[
            ("GW_REPO", fx.root.to_str().unwrap()),
            ("GW_JIRA_PROJECT", "OTHER"),
        ],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();

    assert!(ticket_for(&json, "PROJ-42").is_null());
    assert_eq!(ticket_for(&json, "OTHER-7"), "OTHER-7");
}

// ---- machine-readable output for agents -----------------------------------

/// stdout must hold exactly one JSON document, even when the command narrates.
#[test]
fn clean_json_writes_only_json_to_stdout() {
    let fx = Fixture::new();
    fx.add_worktree("PROJ-20");

    let out = fx.gw(&["clean", "--dry-run", "--skip-github", "--json"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout)
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {:?}", out.stdout));
    assert_eq!(json["applied"], serde_json::json!(false));
}

/// A script that asked for JSON can tell a plan from an execution.
#[test]
fn clean_json_reports_whether_the_plan_was_applied() {
    let fx = Fixture::new();
    let wt = fx.add_worktree("PROJ-21");
    fx.publish("PROJ-21");

    let planned = fx.gw(&["clean", "--skip-github", "--stale-days", "0", "--json"]);
    assert_eq!(planned.code, 0, "stderr: {}", planned.stderr);
    let json: serde_json::Value = serde_json::from_str(&planned.stdout).expect("valid json");
    assert_eq!(json["applied"], serde_json::json!(false));
    assert!(wt.is_dir(), "a JSON run without --yes removed a worktree");

    let applied = fx.gw(&[
        "clean",
        "--yes",
        "--skip-github",
        "--stale-days",
        "0",
        "--json",
    ]);
    assert_eq!(applied.code, 0, "stderr: {}", applied.stderr);
    let json: serde_json::Value = serde_json::from_str(&applied.stdout).expect("valid json");
    assert_eq!(json["applied"], serde_json::json!(true));
    assert!(!wt.exists(), "--yes did not apply the plan");
}

#[test]
fn add_json_reports_the_branch_and_path() {
    let fx = Fixture::new();

    let out = fx.gw(&["add", "PROJ-22", "--no-fetch", "--json"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).expect("valid json");
    assert_eq!(json["branch"], "PROJ-22");
    assert_eq!(json["created"], json!(true));
    assert!(json["path"]
        .as_str()
        .unwrap()
        .ends_with("worktrees/PROJ-22"));

    // Reusing an existing worktree is reported as not created.
    let again = fx.gw(&["add", "PROJ-22", "--no-fetch", "--json"]);
    assert_eq!(again.code, 0, "stderr: {}", again.stderr);
    let json: serde_json::Value = serde_json::from_str(&again.stdout).expect("valid json");
    assert_eq!(json["created"], json!(false));
}

/// `--print-path` is a stricter contract than JSON and wins over `--json`.
#[test]
fn add_print_path_beats_json() {
    let fx = Fixture::new();
    let out = fx.gw(&["add", "PROJ-23", "--no-fetch", "--print-path", "--json"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(out.stdout.trim().ends_with("worktrees/PROJ-23"));
}

/// The binary cannot change its parent shell's directory, so `--cd` must hand
/// the shell the bare path the wrapper `cd`s into. Anything else on stdout
/// would be pasted into `cd`.
#[test]
fn add_cd_prints_only_the_path() {
    let fx = Fixture::new();
    let out = fx.gw(&["add", "PROJ-24", "--no-fetch", "--cd", "--json"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    // One line, absolute, and nothing else. Compare the suffix because the
    // temp root may resolve through a symlink (`/var` on macOS).
    assert_eq!(out.stdout.lines().count(), 1, "output: {}", out.stdout);
    assert_eq!(out.stdout.trim(), out.stdout.trim_end_matches('\n'));
    assert!(
        out.stdout.trim().starts_with('/'),
        "not absolute: {}",
        out.stdout
    );
    assert!(out.stdout.trim().ends_with("worktrees/PROJ-24"));
}

/// `--no-cd` opts out of a `GW_CD=1` default or a shell alias, and the last
/// of the pair wins when both are given.
#[test]
fn add_no_cd_restores_normal_output() {
    let fx = Fixture::new();
    let out = fx.gw(&["add", "PROJ-25", "--no-fetch", "--cd", "--no-cd"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("Worktree ready at"),
        "unexpected output: {}",
        out.stdout
    );

    let out = fx.gw(&["add", "PROJ-25", "--no-fetch", "--no-cd", "--cd"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(out.stdout.trim().ends_with("worktrees/PROJ-25"));
}

#[test]
fn init_json_reports_the_created_layout() {
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
            "--json",
        ],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout)
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {:?}", out.stdout));
    assert_eq!(json["created"], json!(true));
    assert_eq!(json["branch"], "main");
    assert!(json["checkout"]
        .as_str()
        .unwrap()
        .ends_with("worktrees/main"));
}

#[test]
fn init_json_dry_run_reports_no_checkout() {
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
            "--dry-run",
            "--json",
        ],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).expect("valid json");
    assert_eq!(json["created"], json!(false));
    assert!(json["branch"].is_null());
}

/// Every command that documents `--json` must emit a parseable document with
/// nothing else on stdout. This is the guard against a future command or a new
/// progress line silently breaking the contract.
#[test]
fn every_documented_json_command_parses() {
    let fx = Fixture::new();
    fx.add_worktree("PROJ-30");

    let cases: [(&str, &[&str]); 4] = [
        ("status", &["status", "--json"]),
        ("repos", &["repos", "--json"]),
        ("clean", &["clean", "--json", "--skip-github"]),
        ("add", &["add", "PROJ-31", "--no-fetch", "--json"]),
    ];

    for (name, args) in cases {
        let out = fx.gw(args);
        assert_eq!(out.code, 0, "{name}: stderr: {}", out.stderr);
        serde_json::from_str::<serde_json::Value>(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "{name} did not write one JSON document to stdout ({e}): {:?}",
                out.stdout
            )
        });
    }
}

/// A deleted branch's restore command is the only way back. It must still be
/// reachable from a JSON run, just not on stdout.
#[test]
fn clean_json_moves_restore_hints_to_stderr() {
    let fx = Fixture::new();
    // Orphan branches are only deleted outside a worktree, so create a local
    // branch with no checkout.
    run(&fx.bare(), &["branch", "PROJ-32", "develop"]);
    // `publish` pushes from the bare repo, which makes the tip reachable and
    // therefore deletable.
    fx.publish("PROJ-32");

    let out = fx.gw(&[
        "clean",
        "--yes",
        "--delete-branches",
        "--skip-github",
        "--stale-days",
        "0",
        "--json",
    ]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value =
        serde_json::from_str(&out.stdout).expect("stdout is one document");
    assert!(
        json["counts"]["to_delete"].as_u64().unwrap() >= 1,
        "nothing was deleted, so the hint path was not exercised: {json}"
    );
    assert!(
        out.stderr.contains("git branch PROJ-32"),
        "restore hint lost in JSON mode: {}",
        out.stderr
    );
}

/// Exit 3 means partial destruction. The JSON body must still be emitted so
/// the caller can see what succeeded and what did not.
#[test]
fn clean_json_emits_a_document_on_partial_failure() {
    let fx = Fixture::new();
    let locked = fx.add_worktree("PROJ-33");
    let free = fx.add_worktree("PROJ-34");
    fx.publish("PROJ-33");
    fx.publish("PROJ-34");
    // A locked worktree cannot be removed without --force, while its sibling
    // is removable: one success, one failure.
    run(&fx.bare(), &["worktree", "lock", locked.to_str().unwrap()]);

    let out = fx.gw(&[
        "clean",
        "--yes",
        "--skip-github",
        "--stale-days",
        "0",
        "--json",
    ]);
    assert_eq!(
        out.code, 3,
        "expected a partial failure; stderr: {}",
        out.stderr
    );
    let json: serde_json::Value = serde_json::from_str(&out.stdout)
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {:?}", out.stdout));
    assert_eq!(json["applied"], json!(true));
    assert!(!free.is_dir(), "the removable worktree survived");
    assert!(locked.is_dir(), "the locked worktree was removed");
}

/// `--dry-run --yes` is still a dry run: the explicit refusal wins.
#[test]
fn clean_dry_run_beats_yes() {
    let fx = Fixture::new();
    let wt = fx.add_worktree("PROJ-35");
    fx.publish("PROJ-35");

    let out = fx.gw(&[
        "clean",
        "--dry-run",
        "--yes",
        "--skip-github",
        "--stale-days",
        "0",
        "--json",
    ]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).expect("valid json");
    assert_eq!(json["applied"], json!(false));
    assert!(wt.is_dir(), "dry run removed a worktree");
}

/// A plan with nothing to remove is still a valid document, and `applied`
/// stays false because nothing ran.
#[test]
fn clean_json_with_an_empty_plan_still_parses() {
    let fx = Fixture::new();
    let out = fx.gw(&["clean", "--skip-github", "--json"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).expect("valid json");
    assert_eq!(json["applied"], json!(false));
    assert!(json["entries"].is_array());
    assert!(json["counts"].is_object());
}

/// Warnings must not leak into stdout, however noisy the environment is.
#[test]
fn json_stdout_survives_lookup_failures() {
    let fx = Fixture::new();
    fx.add_worktree("PROJ-36");
    // Point at a JIRA base URL and credentials that cannot resolve, and let
    // the GitHub lookup run: both paths warn, neither may touch stdout.
    let out = gw_with_env(
        &fx.bare(),
        &["clean", "--json", "--stale-days", "0"],
        &[
            ("GW_REPO", fx.root.to_str().unwrap()),
            ("GW_JIRA_BASE_URL", "https://127.0.0.1:1"),
            ("GW_JIRA_PROJECT", "PROJ"),
            ("GW_JIRA_USER", "user@example.com"),
            ("GW_JIRA_API_TOKEN", "not-a-real-token"),
            ("GW_SKIP_GITHUB", "1"),
        ],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    serde_json::from_str::<serde_json::Value>(&out.stdout)
        .unwrap_or_else(|e| panic!("warning leaked into stdout ({e}): {:?}", out.stdout));
}

// ---- convention configuration ---------------------------------------------
//
// `GW_NO_GLOBAL_CONFIG` is set for every test, so the only global layer these
// tests can exercise is the one they point `GW_CONFIG` at.

#[test]
fn convention_show_reports_the_built_in_default() {
    let fx = Fixture::new();
    let out = fx.gw(&["convention", "show", "--json"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).expect("valid json");

    assert_eq!(json["ticket_rules"][0]["provider"], "jira");
    assert_eq!(json["ticket_rules"][0]["selector"], "embedded");
    assert_eq!(json["ticket_rules"][0]["strength"], "identity");
    assert_eq!(json["change_rules"][0]["selector"], "head_branch");
    assert_eq!(json["ticket_rules_origin"], "built-in");
    assert!(json["sources"].as_array().unwrap().is_empty());
}

#[test]
fn a_layout_convention_replaces_the_built_in_rules() {
    let fx = Fixture::new();
    std::fs::write(
        fx.root.join(".gw.toml"),
        r#"
[[convention.ticket_rules]]
provider = "linear"
selector = "embedded"
pattern = 'eng-(?P<id>[0-9]+)'

[[convention.change_rules]]
provider = "github"
selector = "head_branch"
"#,
    )
    .unwrap();
    fx.add_worktree("eng-42");

    let out = fx.gw(&["status", "--json"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(
        ticket_for(&json, "eng-42"),
        "42",
        "a custom pattern's capture group is the ticket id"
    );
}

#[test]
fn a_global_configuration_is_a_recipe_a_layout_can_specialize() {
    let fx = Fixture::new();
    let global = tempfile::tempdir().unwrap();
    let global_file = global.path().join("gw.toml");
    std::fs::write(
        &global_file,
        r#"
[[convention.ticket_rules]]
provider = "jira"
selector = "embedded"
pattern = '[A-Z][A-Z0-9]+-[0-9]+'
"#,
    )
    .unwrap();
    // The layout narrows only the project list.
    std::fs::write(fx.root.join(".gw.toml"), "[jira]\nproject = \"PROJ\"\n").unwrap();
    fx.add_worktree("PROJ-5");
    fx.add_worktree("OTHER-9");

    let out = gw_with_env(
        &fx.bare(),
        &["status", "--json"],
        &[
            ("GW_REPO", fx.root.to_str().unwrap()),
            ("GW_CONFIG", global_file.to_str().unwrap()),
        ],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(ticket_for(&json, "PROJ-5"), "PROJ-5");
    assert!(ticket_for(&json, "OTHER-9").is_null());
}

#[test]
fn convention_show_names_both_source_files() {
    let fx = Fixture::new();
    let global = tempfile::tempdir().unwrap();
    let global_file = global.path().join("gw.toml");
    std::fs::write(
        &global_file,
        "[[convention.change_rules]]\nprovider = \"github\"\nselector = \"head_branch\"\n",
    )
    .unwrap();
    std::fs::write(fx.root.join(".gw.toml"), "[jira]\nproject = \"PROJ\"\n").unwrap();

    let out = gw_with_env(
        &fx.bare(),
        &["convention", "show", "--json"],
        &[
            ("GW_REPO", fx.root.to_str().unwrap()),
            ("GW_CONFIG", global_file.to_str().unwrap()),
        ],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    let sources: Vec<&str> = json["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(sources.len(), 2, "both files were read: {sources:?}");
    assert!(sources[0].ends_with("gw.toml"));
    assert!(sources[1].ends_with(".gw.toml"));
    assert_eq!(json["change_rules_origin"], "global");
}

#[test]
fn an_unknown_selector_is_a_usage_error() {
    let fx = Fixture::new();
    std::fs::write(
        fx.root.join(".gw.toml"),
        "[[convention.ticket_rules]]\nprovider = \"jira\"\nselector = \"regex\"\n",
    )
    .unwrap();
    let out = fx.gw(&["status"]);
    assert_eq!(out.code, 2, "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("unknown ticket selector"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn an_invalid_pattern_is_a_usage_error() {
    let fx = Fixture::new();
    std::fs::write(
        fx.root.join(".gw.toml"),
        "[[convention.ticket_rules]]\nprovider = \"jira\"\nselector = \"embedded\"\npattern = \"([\"\n",
    )
    .unwrap();
    let out = fx.gw(&["status"]);
    assert_eq!(out.code, 2, "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("invalid ticket pattern"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn an_unknown_convention_key_is_a_usage_error() {
    let fx = Fixture::new();
    std::fs::write(fx.root.join(".gw.toml"), "[convention]\nserch = \"typo\"\n").unwrap();
    let out = fx.gw(&["status"]);
    assert_eq!(out.code, 2, "stderr: {}", out.stderr);
}

#[test]
fn explain_shows_which_rule_matched() {
    let fx = Fixture::new();
    let out = fx.gw(&["convention", "explain", "feature/PROJ-77-thing", "--json"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(json["tickets"][0]["id"], "PROJ-77");
    assert_eq!(json["rules"][0]["matched"], json!(true));
    assert_eq!(json["rules"][0]["rule"], "ticket_rules[0]");
}

/// A branch with no ticket still has a change query, because the built-in
/// change rule keys on the branch name rather than on a ticket. Explain must
/// say so instead of implying that nothing will be looked up.
#[test]
fn explain_separates_a_ticket_miss_from_a_change_match() {
    let fx = Fixture::new();
    let out = fx.gw(&["convention", "explain", "no-ticket-here", "--json"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(json["tickets"], json!([]));
    assert_eq!(json["rules"][0]["matched"], json!(false));
    assert_eq!(json["rules"][1]["rule"], "change_rules[0]");
    assert_eq!(json["rules"][1]["matched"], json!(true));
    assert_eq!(json["changes"][0]["selector"], "head_branch");
}

/// A change selector that needs a ticket does not fire without one.
#[test]
fn explain_skips_a_ticket_dependent_change_rule() {
    let fx = Fixture::new();
    std::fs::write(
        fx.root.join(".gw.toml"),
        "[[convention.change_rules]]\nprovider = \"github\"\nselector = \"mention_ticket\"\n",
    )
    .unwrap();
    let out = fx.gw(&["convention", "explain", "no-ticket-here", "--json"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(json["rules"][0]["matched"], json!(false));
    assert_eq!(json["changes"], json!([]));
}

// ---- out-of-process providers ---------------------------------------------
//
// These tests write a small executable that speaks the provider protocol, so
// the mapping under test is the real one: a branch name, a convention, a
// process boundary, and a decision.

/// Write an executable script that echoes a fixed protocol-1 response.
fn fake_provider(dir: &Path, name: &str, response: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(
        &path,
        format!("#!/bin/sh\ncat > /dev/null\ncat <<'JSON'\n{response}\nJSON\n"),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}

#[test]
fn a_command_provider_resolves_a_non_jira_ticket() {
    let fx = Fixture::new();
    let scripts = tempfile::tempdir().unwrap();
    let provider = fake_provider(
        scripts.path(),
        "linear",
        r#"{"protocol":1,"results":[{"id":"eng-42","state":"closed","resolution":"Done","summary":"Fix it"}]}"#,
    );
    std::fs::write(
        fx.root.join(".gw.toml"),
        format!(
            r#"
[convention.providers.linear]
kind = "command"
command = "{}"

[[convention.ticket_rules]]
provider = "linear"
selector = "embedded"
pattern = 'eng-(?P<id>[0-9]+)'

[[convention.change_rules]]
provider = "github"
selector = "head_branch"
"#,
            provider.display()
        ),
    )
    .unwrap();
    fx.add_worktree("eng-42");

    let out = fx.gw(&["convention", "explain", "eng-42", "--json"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(json["tickets"][0]["provider"], "linear");
    assert_eq!(json["tickets"][0]["id"], "42");
    assert_eq!(json["rules"][0]["provider_active"], json!(true));
}

/// The full loop: a command provider reports a merged change for a branch
/// whose name is unrelated to its ticket, and the worktree is removed.
#[test]
fn clean_removes_a_worktree_when_a_command_provider_reports_a_merge() {
    let fx = Fixture::new();
    let scripts = tempfile::tempdir().unwrap();
    let provider = fake_provider(
        scripts.path(),
        "host",
        r##"{"protocol":1,"results":[{"branch":"topic-ish-name","state":"merged","reference":"#7"}]}"##,
    );
    let wt = fx.add_worktree("topic-ish-name");
    fx.publish("topic-ish-name");
    std::fs::write(
        fx.root.join(".gw.toml"),
        format!(
            r#"
[convention.providers.host]
kind = "command"
command = "{}"

[[convention.change_rules]]
provider = "host"
selector = "head_branch"
"#,
            provider.display()
        ),
    )
    .unwrap();

    let out = fx.gw(&["clean", "--yes", "--json"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(
        json["applied"],
        json!(true),
        "stderr: {} json: {json}",
        out.stderr
    );
    assert!(
        !wt.exists(),
        "a merged change from a command provider did not remove the worktree: {json}"
    );
}

/// A provider that violates the protocol must not be read as "not merged".
#[test]
fn a_broken_command_provider_never_removes_a_worktree() {
    let fx = Fixture::new();
    let scripts = tempfile::tempdir().unwrap();
    let provider = scripts.path().join("broken");
    std::fs::write(&provider, "#!/bin/sh\necho not-json\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&provider, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let wt = fx.add_worktree("PROJ-broken");
    fx.publish("PROJ-broken");
    std::fs::write(
        fx.root.join(".gw.toml"),
        format!(
            r#"
[convention.providers.host]
kind = "command"
command = "{}"

[[convention.change_rules]]
provider = "host"
selector = "head_branch"
"#,
            provider.display()
        ),
    )
    .unwrap();

    let out = fx.gw(&["clean", "--yes", "--json", "--stale-days", "0"]);
    let json: serde_json::Value = serde_json::from_str(&out.stdout)
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {:?}", out.stdout));
    let removed = json["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["action"] == "remove" && entry["reason"] == "merged_pr");
    assert!(!removed, "a malformed response produced a merge: {json}");
    // The stale rule may still apply; what matters is that the merge rule did
    // not. The stale removal is legitimate, so the test does not assert that
    // the directory survived.
    let _ = wt;
}

/// Two providers disagree. Neither answer may justify removal.
#[test]
fn a_contested_merge_from_two_command_providers_removes_nothing() {
    let fx = Fixture::new();
    let scripts = tempfile::tempdir().unwrap();
    let merged = fake_provider(
        scripts.path(),
        "merged",
        r#"{"protocol":1,"results":[{"branch":"PROJ-9","state":"merged"}]}"#,
    );
    let broken = scripts.path().join("broken");
    std::fs::write(&broken, "#!/bin/sh\ncat > /dev/null\nexit 3\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&broken, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let wt = fx.add_worktree("PROJ-9");
    fx.publish("PROJ-9");
    std::fs::write(
        fx.root.join(".gw.toml"),
        format!(
            r#"
[convention.providers.merged]
kind = "command"
command = "{}"

[convention.providers.broken]
kind = "command"
command = "{}"

[[convention.change_rules]]
provider = "merged"
selector = "head_branch"

[[convention.change_rules]]
provider = "broken"
selector = "head_branch"
"#,
            merged.display(),
            broken.display()
        ),
    )
    .unwrap();

    let out = fx.gw(&["clean", "--yes", "--json"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    let removed = json["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["action"] == "remove");
    assert!(!removed, "a contested merge removed a worktree: {json}");
    assert!(wt.is_dir(), "the worktree directory is gone");
}

/// Two worktrees, one provider, one selector. Each must receive its own
/// answer. Before this was keyed by branch, the first answer was applied to
/// both, so a merge on one branch could authorize removing the other.
#[test]
fn each_worktree_receives_its_own_change_answer() {
    let fx = Fixture::new();
    let scripts = tempfile::tempdir().unwrap();
    let provider = fake_provider(
        scripts.path(),
        "host",
        r#"{"protocol":1,"results":[
            {"branch":"merged-one","state":"merged"},
            {"branch":"open-one","state":"open"}
        ]}"#,
    );
    let merged = fx.add_worktree("merged-one");
    let open = fx.add_worktree("open-one");
    fx.publish("merged-one");
    fx.publish("open-one");
    std::fs::write(
        fx.root.join(".gw.toml"),
        format!(
            r#"
[convention.providers.host]
kind = "command"
command = "{}"

[[convention.change_rules]]
provider = "host"
selector = "head_branch"
"#,
            provider.display()
        ),
    )
    .unwrap();

    let out = fx.gw(&["clean", "--yes", "--json"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(!merged.exists(), "the merged worktree was not removed");
    assert!(
        open.exists(),
        "the open worktree was removed: the first answer was applied to both"
    );
}
