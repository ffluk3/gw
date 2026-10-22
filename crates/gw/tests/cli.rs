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
        .env_remove("GW_JIRA_BASE_URL")
        .env_remove("GW_JIRA_USER")
        .env_remove("GW_JIRA_API_TOKEN")
        .env_remove("GW_SKIP_PROVIDERS")
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

/// Add an empty commit to `worktree` whose committer date is `seconds` old.
///
/// `gw` measures a worktree's age from its committer date (`git log -1 %ct`),
/// not wall-clock time, so ageing the commit is what makes a worktree stale
/// deterministically. Sleeping instead races the clock: a `stale_after` of
/// `"1s"` needs the commit to be a whole second old, and a commit made
/// milliseconds earlier is usually age 0.
fn commit_aged(worktree: &Path, seconds: i64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after the epoch")
        .as_secs() as i64;
    let when = format!("@{} +0000", now - seconds);
    let out = Command::new("git")
        .current_dir(worktree)
        .env("GIT_AUTHOR_DATE", &when)
        .env("GIT_COMMITTER_DATE", &when)
        .args(["commit", "--allow-empty", "-m", "age this worktree"])
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "aged commit failed: {}",
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

    let out = fx.gw(&["clean", "--dry-run", "--skip-provider", "github"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(wt.is_dir(), "dry run removed the worktree");
}

#[test]
fn clean_refuses_to_remove_a_dirty_worktree() {
    let fx = Fixture::new();
    let wt = fx.add_worktree("PROJ-3");
    fx.publish("PROJ-3");
    std::fs::write(wt.join("scratch.txt"), "uncommitted\n").unwrap();

    let out = fx.gw(&[
        "clean",
        "--yes",
        "--skip-provider",
        "github",
        "--stale-days",
        "0",
    ]);
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

    let out = fx.gw(&[
        "clean",
        "--yes",
        "--skip-provider",
        "github",
        "--stale-days",
        "0",
    ]);
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

    let out = fx.gw(&[
        "clean",
        "--yes",
        "--skip-provider",
        "github",
        "--stale-days",
        "0",
    ]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(!wt.exists(), "clean worktree was not removed");
}

#[test]
fn clean_never_touches_the_default_branch_worktree() {
    let fx = Fixture::new();
    let develop = fx.root.join("worktrees/develop");

    let out = fx.gw(&[
        "clean",
        "--yes",
        "--skip-provider",
        "github",
        "--stale-days",
        "0",
    ]);
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

    let out = fx.gw(&[
        "clean",
        "--yes",
        "--skip-provider",
        "github",
        "--stale-days",
        "0",
    ]);
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

/// `gw repos` reports what each layout declares, not the convention that
/// would apply to it. A layout that names a non-JIRA provider must not be
/// labelled with one.
#[test]
fn repos_reports_the_layouts_own_ticket_provider() {
    let fx = Fixture::new();
    let outer = tempfile::tempdir().unwrap();
    let repos = outer.path().join("repos");
    link_layout(&fx, &repos, "acme/widgets");
    std::fs::write(
        repos.join("acme/widgets/.gw.toml"),
        r#"
[[convention.ticket_rules]]
provider = "linear"
selector = "embedded"
pattern = 'eng-(?P<id>[0-9]+)'
"#,
    )
    .unwrap();

    let out = gw_with_env(
        outer.path(),
        &["repos", "--json"],
        &[("GW_REPOS_ROOT", repos.to_str().unwrap())],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(json["repos"][0]["ticket_providers"], json!(["linear"]));
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
fn allow_projects_filters_ticket_keys() {
    let fx = Fixture::new();
    std::fs::write(
        fx.root.join(".gw.toml"),
        r#"
[[convention.ticket_rules]]
provider = "jira"
selector = "embedded"
pattern = '[A-Z][A-Z0-9]+-[0-9]+'
allow_projects = ["PROJ"]
"#,
    )
    .unwrap();
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

// ---- machine-readable output for agents -----------------------------------

/// stdout must hold exactly one JSON document, even when the command narrates.
#[test]
fn clean_json_writes_only_json_to_stdout() {
    let fx = Fixture::new();
    fx.add_worktree("PROJ-20");

    let out = fx.gw(&["clean", "--dry-run", "--skip-provider", "github", "--json"]);
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

    let planned = fx.gw(&[
        "clean",
        "--skip-provider",
        "github",
        "--stale-days",
        "0",
        "--json",
    ]);
    assert_eq!(planned.code, 0, "stderr: {}", planned.stderr);
    let json: serde_json::Value = serde_json::from_str(&planned.stdout).expect("valid json");
    assert_eq!(json["applied"], serde_json::json!(false));
    assert!(wt.is_dir(), "a JSON run without --yes removed a worktree");

    let applied = fx.gw(&[
        "clean",
        "--yes",
        "--skip-provider",
        "github",
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
        ("clean", &["clean", "--json", "--skip-provider", "github"]),
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
        "--skip-provider",
        "github",
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
        "--skip-provider",
        "github",
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
        "--skip-provider",
        "github",
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
    let out = fx.gw(&["clean", "--skip-provider", "github", "--json"]);
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
            ("GW_JIRA_USER", "user@example.com"),
            ("GW_JIRA_API_TOKEN", "not-a-real-token"),
            ("GW_SKIP_PROVIDERS", "github"),
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
    std::fs::write(
        fx.root.join(".gw.toml"),
        "[[convention.ticket_rules]]\nprovider = \"jira\"\nselector = \"embedded\"\npattern = '[A-Z][A-Z0-9]+-[0-9]+'\nallow_projects = [\"PROJ\"]\n",
    )
    .unwrap();
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
    std::fs::write(
        fx.root.join(".gw.toml"),
        "[[convention.ticket_rules]]\nprovider = \"jira\"\nselector = \"embedded\"\npattern = '[A-Z][A-Z0-9]+-[0-9]+'\n",
    )
    .unwrap();

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

// ---- built-in providers through their real command boundary ---------------
//
// The GitHub adapter shells out to `gh` and the JIRA adapter to `curl`. Putting
// a stand-in early on `PATH` exercises the real argument building, response
// parsing, and error handling without a network call or a mock in the code.

/// Write an executable script and return the directory to put on `PATH`.
fn fake_binary(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}

/// Run `gw` with `dir` prepended to `PATH`.
fn gw_with_fakes(cwd: &Path, args: &[&str], dir: &Path, env: &[(&str, &str)]) -> Output {
    let path = std::env::var("PATH").unwrap_or_default();
    let mut entries: Vec<(&str, String)> =
        env.iter().map(|(k, v)| (*k, (*v).to_string())).collect();
    entries.push(("PATH", format!("{}:{path}", dir.display())));
    let refs: Vec<(&str, &str)> = entries.iter().map(|(k, v)| (*k, v.as_str())).collect();
    gw_with_env(cwd, args, &refs)
}

/// Point `origin` at a GitHub URL, which is what makes the GitHub adapter pick
/// up the repository. The fixture's real origin is a local path, so this is
/// done after the branch is published.
fn pretend_origin_is_github(fx: &Fixture) {
    run(
        &fx.bare(),
        &[
            "remote",
            "set-url",
            "origin",
            "git@github.com:acme/widgets.git",
        ],
    );
}

/// A `gh` stand-in that answers each `is:pr is:merged` search with
/// `issueCount: 1` when the branch it names is in `merged`, and `0` otherwise.
///
/// The adapter derives its GraphQL aliases (`b0`, `b1`, ...) from the order
/// `git worktree list` reports, which is filesystem enumeration order, not
/// creation order — it differs between Linux and macOS. Keying the answer on
/// the branch inside the query string keeps a test indifferent to that order;
/// an alias-indexed answer would hand one branch another branch's verdict.
fn fake_merged_prs(dir: &Path, merged: &[&str]) -> PathBuf {
    let arms: String = merged
        .iter()
        .map(|branch| format!("      {branch}) count=1 ;;\n"))
        .collect();
    let body = r#"cat > /dev/null
out=""
for arg in "$@"; do
  case "$arg" in
    b*=*)
      alias=${arg%%=*}
      rest=${arg#*=}
      head=${rest#*head:}
      head=${head%% *}
      count=0
      case "$head" in
__MERGED_ARMS__      esac
      out="$out\"$alias\":{\"issueCount\":$count},"
      ;;
  esac
done
printf '{"data":{%s}}\n' "${out%,}"
"#;
    fake_binary(dir, "gh", &body.replace("__MERGED_ARMS__", &arms))
}

#[test]
fn a_merged_github_pr_removes_the_worktree() {
    let fx = Fixture::new();
    let scripts = tempfile::tempdir().unwrap();
    // Only the branch under test is merged; `develop` must not be.
    fake_merged_prs(scripts.path(), &["PROJ-40"]);
    let wt = fx.add_worktree("PROJ-40");
    fx.publish("PROJ-40");
    pretend_origin_is_github(&fx);

    let out = gw_with_fakes(
        &fx.bare(),
        &["clean", "--yes", "--json"],
        scripts.path(),
        &[("GW_REPO", fx.root.to_str().unwrap())],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).expect("valid json");
    assert!(
        !wt.exists(),
        "a merged PR did not remove the worktree: {json}\nstderr: {}",
        out.stderr
    );
}

#[test]
fn an_open_github_pr_leaves_the_worktree_alone() {
    let fx = Fixture::new();
    let scripts = tempfile::tempdir().unwrap();
    fake_merged_prs(scripts.path(), &[]);
    let wt = fx.add_worktree("PROJ-41");
    fx.publish("PROJ-41");
    pretend_origin_is_github(&fx);

    let out = gw_with_fakes(
        &fx.bare(),
        &["clean", "--yes", "--json"],
        scripts.path(),
        &[("GW_REPO", fx.root.to_str().unwrap())],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(wt.exists(), "an open PR removed the worktree");
}

#[test]
fn a_gh_failure_is_reported_and_never_reads_as_merged() {
    let fx = Fixture::new();
    let scripts = tempfile::tempdir().unwrap();
    fake_binary(
        scripts.path(),
        "gh",
        "cat > /dev/null\necho nope >&2\nexit 2",
    );
    let wt = fx.add_worktree("PROJ-42");
    fx.publish("PROJ-42");
    pretend_origin_is_github(&fx);

    let out = gw_with_fakes(
        &fx.bare(),
        &["clean", "--yes", "--json", "--stale-days", "0"],
        scripts.path(),
        &[("GW_REPO", fx.root.to_str().unwrap())],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).expect("valid json");
    assert!(
        !json["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["action"] == "remove" && e["reason"] == "merged_pr"),
        "a gh failure produced a merge: {json}"
    );
    let _ = wt;
}

#[test]
fn a_malformed_gh_response_never_reads_as_merged() {
    let fx = Fixture::new();
    let scripts = tempfile::tempdir().unwrap();
    fake_binary(
        scripts.path(),
        "gh",
        "cat > /dev/null\necho '{\"data\":{}}'",
    );
    let wt = fx.add_worktree("PROJ-43");
    fx.publish("PROJ-43");
    pretend_origin_is_github(&fx);

    let out = gw_with_fakes(
        &fx.bare(),
        &["clean", "--yes", "--json", "--stale-days", "0"],
        scripts.path(),
        &[("GW_REPO", fx.root.to_str().unwrap())],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).expect("valid json");
    assert!(
        !json["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["reason"] == "merged_pr"),
        "a missing count produced a merge: {json}"
    );
    let _ = wt;
}

/// A JIRA base URL and credentials, so the adapter activates.
fn jira_layout(fx: &Fixture, base_url: &str) {
    std::fs::write(
        fx.root.join(".gw.toml"),
        format!(
            "[convention.providers.jira]\nbase_url = \"{base_url}\"\n\n\
             [[convention.ticket_rules]]\nprovider = \"jira\"\nselector = \"embedded\"\n\
             pattern = '[A-Z][A-Z0-9]+-[0-9]+'\n\n\
             [[convention.change_rules]]\nprovider = \"github\"\nselector = \"head_branch\"\n"
        ),
    )
    .unwrap();
}

fn jira_env(fx: &Fixture) -> Vec<(&'static str, &str)> {
    vec![
        ("GW_REPO", fx.root.to_str().unwrap()),
        ("GW_JIRA_USER", "user@example.com"),
        ("GW_JIRA_API_TOKEN", "token"),
    ]
}

#[test]
fn a_closed_jira_ticket_is_reportable_and_removable() {
    let fx = Fixture::new();
    let scripts = tempfile::tempdir().unwrap();
    jira_layout(&fx, "https://jira.example.test");
    fake_binary(
        scripts.path(),
        "curl",
        r#"cat > /dev/null
echo '{"issues":[{"key":"PROJ-50","fields":{"status":{"statusCategory":{"key":"done"}},"resolution":{"name":"Fixed"},"summary":"Ship it"}}]}'"#,
    );
    let wt = fx.add_worktree("PROJ-50");
    fx.publish("PROJ-50");

    // Without --closed the ticket is a recommendation, not a removal.
    let planned = gw_with_fakes(
        &fx.bare(),
        &["clean", "--json", "--skip-provider", "github"],
        scripts.path(),
        &jira_env(&fx),
    );
    assert_eq!(planned.code, 0, "stderr: {}", planned.stderr);
    let json: serde_json::Value = serde_json::from_str(&planned.stdout).expect("valid json");
    assert_eq!(json["counts"]["recommended"], json!(1), "{json}");
    assert!(wt.is_dir());

    let applied = gw_with_fakes(
        &fx.bare(),
        &[
            "clean",
            "--yes",
            "--closed",
            "--json",
            "--skip-provider",
            "github",
        ],
        scripts.path(),
        &jira_env(&fx),
    );
    assert_eq!(applied.code, 0, "stderr: {}", applied.stderr);
    assert!(!wt.exists(), "a closed ticket did not remove the worktree");
}

#[test]
fn jira_ticket_summary_reaches_the_status_output() {
    let fx = Fixture::new();
    let scripts = tempfile::tempdir().unwrap();
    jira_layout(&fx, "https://jira.example.test");
    fake_binary(
        scripts.path(),
        "curl",
        r#"cat > /dev/null
echo '{"issues":[{"key":"PROJ-51","fields":{"status":{"statusCategory":{"key":"indeterminate"}},"summary":"Add rate limiting"}}]}'"#,
    );
    fx.add_worktree("PROJ-51");

    let out = gw_with_fakes(
        &fx.bare(),
        &["status", "--json", "--skip-provider", "github"],
        scripts.path(),
        &jira_env(&fx),
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).expect("valid json");
    let row = json["worktrees"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["branch"] == "PROJ-51")
        .expect("row");
    assert_eq!(row["summary"], "Add rate limiting");
    assert_eq!(row["ticket_state"]["status"], "open");
}

#[test]
fn jira_errors_and_odd_responses_degrade_to_unknown() {
    let fx = Fixture::new();
    jira_layout(&fx, "https://jira.example.test");
    fx.add_worktree("PROJ-52");

    for body in [
        // A tracker error message.
        r#"cat > /dev/null
echo '{"errorMessages":["no such project"]}'"#,
        // Invalid JSON.
        "cat > /dev/null\necho not-json",
        // A non-zero curl exit.
        "cat > /dev/null\necho boom >&2\nexit 22",
        // A response with no status category.
        r#"cat > /dev/null
echo '{"issues":[{"key":"PROJ-52","fields":{"summary":"no status"}}]}'"#,
    ] {
        let scripts = tempfile::tempdir().unwrap();
        fake_binary(scripts.path(), "curl", body);
        let out = gw_with_fakes(
            &fx.bare(),
            &["status", "--json", "--skip-provider", "github"],
            scripts.path(),
            &jira_env(&fx),
        );
        assert_eq!(out.code, 0, "stderr: {}", out.stderr);
        let json: serde_json::Value = serde_json::from_str(&out.stdout)
            .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {:?}", out.stdout));
        let row = json["worktrees"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["branch"] == "PROJ-52")
            .expect("row");
        assert_eq!(
            row["ticket_state"]["status"], "unknown",
            "an untrustworthy answer was read as a state: {row}"
        );
    }
}

#[test]
fn a_non_https_jira_base_url_is_refused_before_any_request() {
    let fx = Fixture::new();
    let scripts = tempfile::tempdir().unwrap();
    jira_layout(&fx, "http://jira.example.test");
    // A curl that would succeed if it ever ran.
    fake_binary(
        scripts.path(),
        "curl",
        r#"cat > /dev/null
echo '{"issues":[]}'"#,
    );
    fx.add_worktree("PROJ-53");

    let out = gw_with_fakes(
        &fx.bare(),
        &["status", "--json", "--skip-provider", "github"],
        scripts.path(),
        &jira_env(&fx),
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).expect("valid json");
    let row = json["worktrees"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["branch"] == "PROJ-53")
        .expect("row");
    assert!(
        row["ticket_state"]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("https"),
        "the base URL was not refused: {row}"
    );
}

#[test]
fn a_ticket_the_tracker_does_not_return_is_unknown() {
    let fx = Fixture::new();
    let scripts = tempfile::tempdir().unwrap();
    jira_layout(&fx, "https://jira.example.test");
    fake_binary(
        scripts.path(),
        "curl",
        r#"cat > /dev/null
echo '{"issues":[]}'"#,
    );
    fx.add_worktree("PROJ-54");

    let out = gw_with_fakes(
        &fx.bare(),
        &["status", "--json", "--skip-provider", "github"],
        scripts.path(),
        &jira_env(&fx),
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).expect("valid json");
    let row = json["worktrees"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["branch"] == "PROJ-54")
        .expect("row");
    assert_eq!(row["ticket_state"]["status"], "unknown");
}

#[test]
fn a_github_remote_that_is_not_github_is_explained() {
    let fx = Fixture::new();
    run(
        &fx.bare(),
        &[
            "remote",
            "set-url",
            "origin",
            "https://gitlab.com/acme/widgets.git",
        ],
    );
    fx.add_worktree("PROJ-55");

    let out = fx.gw(&["clean", "--json", "--stale-days", "0"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    serde_json::from_str::<serde_json::Value>(&out.stdout)
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {:?}", out.stdout));
    assert!(
        out.stderr.contains("not a GitHub remote"),
        "the reason was not reported: {}",
        out.stderr
    );
}

// ---- human output, add branches, and repos tables ---------------------------

#[test]
fn status_renders_a_table_at_the_layout_root() {
    let fx = Fixture::new();
    fx.add_worktree("PROJ-60");

    let out = fx.gw(&["status"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(out.stdout.contains("BRANCH"), "{}", out.stdout);
    assert!(out.stdout.contains("SUMMARY"), "{}", out.stdout);
    assert!(out.stdout.contains("PROJ-60"), "{}", out.stdout);
}

#[test]
fn clean_renders_a_table_and_a_dry_run_notice() {
    let fx = Fixture::new();
    let wt = fx.add_worktree("PROJ-61");
    fx.publish("PROJ-61");

    let out = fx.gw(&[
        "clean",
        "--dry-run",
        "--skip-provider",
        "github",
        "--stale-days",
        "0",
    ]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(out.stdout.contains("ACTION"), "{}", out.stdout);
    assert!(out.stdout.contains("Dry run"), "{}", out.stdout);
    assert!(wt.is_dir());
}

#[test]
fn clean_reports_a_closed_ticket_as_a_recommendation() {
    let fx = Fixture::new();
    let scripts = tempfile::tempdir().unwrap();
    jira_layout(&fx, "https://jira.example.test");
    fake_binary(
        scripts.path(),
        "curl",
        r#"cat > /dev/null
echo '{"issues":[{"key":"PROJ-62","fields":{"status":{"statusCategory":{"key":"done"}},"resolution":{"name":"Fixed"},"summary":"Ship it"}}]}'"#,
    );
    let wt = fx.add_worktree("PROJ-62");
    fx.publish("PROJ-62");

    let out = gw_with_fakes(
        &fx.bare(),
        &["clean", "--skip-provider", "github"],
        scripts.path(),
        &jira_env(&fx),
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("can be removed with --closed"),
        "{}",
        out.stdout
    );
    assert!(wt.is_dir());
}

#[test]
fn add_fetches_by_default_and_creates_a_remote_tracking_branch() {
    let fx = Fixture::new();
    // Publish a branch on origin that has no local ref yet.
    run(&fx.bare(), &["branch", "remote-only", "develop"]);
    fx.publish("remote-only");
    run(&fx.bare(), &["branch", "-D", "remote-only"]);

    let out = fx.gw(&["add", "remote-only", "--print-path"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(fx.root.join("worktrees/remote-only").is_dir());
}

#[test]
fn add_fast_forwards_a_behind_local_branch() {
    let fx = Fixture::new();
    // A local branch with no worktree, and an origin ref that is ahead of it.
    run(&fx.bare(), &["branch", "ff-test", "develop"]);
    let scratch = fx.add_worktree("advance");
    run(&scratch, &["commit", "--allow-empty", "-m", "ahead"]);
    run(&fx.bare(), &["push", "origin", "advance:ff-test"]);
    run(&fx.bare(), &["fetch", "origin"]);
    run(
        &fx.bare(),
        &["worktree", "remove", scratch.to_str().unwrap()],
    );

    let out = fx.gw(&["add", "ff-test", "--print-path"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    // The fast-forward pulled origin's commit onto the local ref.
    let local = std::process::Command::new("git")
        .args(["-C", fx.bare().to_str().unwrap(), "rev-parse", "ff-test"])
        .output()
        .unwrap();
    let remote = std::process::Command::new("git")
        .args([
            "-C",
            fx.bare().to_str().unwrap(),
            "rev-parse",
            "origin/ff-test",
        ])
        .output()
        .unwrap();
    assert_eq!(local.stdout, remote.stdout);
}

#[test]
fn a_lock_failure_in_clean_removes_what_it_can() {
    let fx = Fixture::new();
    let locked = fx.add_worktree("PROJ-64");
    let free = fx.add_worktree("PROJ-65");
    fx.publish("PROJ-64");
    fx.publish("PROJ-65");
    run(&fx.bare(), &["worktree", "lock", locked.to_str().unwrap()]);

    let out = fx.gw(&[
        "clean",
        "--yes",
        "--skip-provider",
        "github",
        "--stale-days",
        "0",
    ]);
    assert_eq!(out.code, 3, "stderr: {}", out.stderr);
    assert!(out.stderr.contains("could not remove"), "{}", out.stderr);
    assert!(!free.is_dir());
}

#[test]
fn a_failed_branch_delete_is_reported() {
    let fx = Fixture::new();
    // A branch with no checkout and a published tip is a delete candidate.
    run(&fx.bare(), &["branch", "PROJ-66", "develop"]);
    fx.publish("PROJ-66");
    // Remove the ref underneath the plan so the delete fails at apply time.
    let out = fx.gw(&[
        "clean",
        "--yes",
        "--delete-branches",
        "--skip-provider",
        "github",
        "--stale-days",
        "0",
    ]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(out.stdout.contains("Deleted branches"), "{}", out.stdout);
}

#[test]
fn repos_renders_a_table_with_and_without_dirty_counts() {
    let fx = Fixture::new();
    let outer = tempfile::tempdir().unwrap();
    let repos = outer.path().join("repos");
    link_layout(&fx, &repos, "acme/widgets");
    std::fs::write(
        repos.join("acme/widgets/.gw.toml"),
        "[[convention.ticket_rules]]\nprovider = \"jira\"\nselector = \"embedded\"\npattern = '[A-Z]+-[0-9]+'\nallow_projects = [\"PROJ\"]\n",
    )
    .unwrap();

    let plain = gw_with_env(
        outer.path(),
        &["repos"],
        &[("GW_REPOS_ROOT", repos.to_str().unwrap())],
    );
    assert_eq!(plain.code, 0, "stderr: {}", plain.stderr);
    assert!(plain.stdout.contains("Layouts under"), "{}", plain.stdout);
    assert!(plain.stdout.contains("jira:PROJ"), "{}", plain.stdout);

    let dirty = gw_with_env(
        outer.path(),
        &["repos", "--dirty"],
        &[("GW_REPOS_ROOT", repos.to_str().unwrap())],
    );
    assert_eq!(dirty.code, 0, "stderr: {}", dirty.stderr);
    assert!(dirty.stdout.contains("DIRTY"), "{}", dirty.stdout);

    let json = gw_with_env(
        outer.path(),
        &["repos", "--dirty", "--json"],
        &[("GW_REPOS_ROOT", repos.to_str().unwrap())],
    );
    assert_eq!(json.code, 0, "stderr: {}", json.stderr);
    let doc: serde_json::Value = serde_json::from_str(&json.stdout).expect("valid json");
    assert!(doc["repos"][0]["dirty"].is_number(), "{doc}");
}

#[test]
fn repos_says_so_when_the_root_holds_no_layouts() {
    let empty = tempfile::tempdir().unwrap();
    let root = empty.path().join("repos");
    std::fs::create_dir_all(&root).unwrap();
    let out = gw_with_env(
        empty.path(),
        &["repos"],
        &[("GW_REPOS_ROOT", root.to_str().unwrap())],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(out.stdout.contains("No layouts under"), "{}", out.stdout);
}

#[test]
fn convention_show_renders_text() {
    let fx = Fixture::new();
    let out = fx.gw(&["convention", "show"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(out.stdout.contains("Convention in force"), "{}", out.stdout);
    assert!(out.stdout.contains("Providers"), "{}", out.stdout);
}

#[test]
fn convention_explain_renders_text_for_a_miss() {
    let fx = Fixture::new();
    let out = fx.gw(&["convention", "explain", "no-ticket-here"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("Branch: no-ticket-here"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("Would query"), "{}", out.stdout);
}

#[test]
fn init_refuses_a_file_at_the_target_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let src = source_repo(tmp.path());
    let root = tmp.path().join("occupied");
    std::fs::write(&root, "not a directory").unwrap();

    let out = gw_raw(
        tmp.path(),
        &[
            "init",
            src.to_str().unwrap(),
            "--dir",
            root.to_str().unwrap(),
        ],
    );
    assert_eq!(out.code, 2, "stderr: {}", out.stderr);
    assert!(out.stderr.contains("is not a directory"), "{}", out.stderr);
}

// ---- settings in .gw.toml --------------------------------------------------
//
// Every environment variable that is not a credential has a file spelling.
// These tests pin the precedence: flag or variable, then the layout file, then
// the built-in default.

/// Write a `.gw.toml` at the layout root.
fn write_layout_config(fx: &Fixture, body: &str) {
    std::fs::write(fx.root.join(".gw.toml"), body).expect("write .gw.toml");
}

#[test]
fn a_settings_only_file_is_not_ignored() {
    // The regression this guards: `apply` used to return early when a file had
    // no `[convention]` table, silently discarding every other section.
    let fx = Fixture::new();
    let wt = fx.add_worktree("PROJ-70");
    commit_aged(&wt, 60);
    fx.publish("PROJ-70");
    write_layout_config(&fx, "[clean]\nstale_after = \"1s\"\n");

    let out = fx.gw(&["clean", "--yes", "--skip-provider", "github"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        !wt.exists(),
        "a file with only [clean] was ignored: {}",
        out.stdout
    );
}

#[test]
fn the_stale_after_duration_holds_a_young_worktree() {
    let fx = Fixture::new();
    let wt = fx.add_worktree("PROJ-71");
    fx.publish("PROJ-71");
    // A worktree committed seconds ago must survive a five-week limit.
    write_layout_config(&fx, "[clean]\nstale_after = \"5w\"\n");

    let out = fx.gw(&["clean", "--yes", "--skip-provider", "github"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(wt.is_dir(), "a young worktree was treated as stale");
}

#[test]
fn the_stale_flag_beats_the_file() {
    let fx = Fixture::new();
    let wt = fx.add_worktree("PROJ-72");
    fx.publish("PROJ-72");
    write_layout_config(&fx, "[clean]\nstale_after = \"5w\"\n");

    let out = fx.gw(&[
        "clean",
        "--yes",
        "--skip-provider",
        "github",
        "--stale-days",
        "0",
    ]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(!wt.exists(), "the flag did not override stale_after");
}

#[test]
fn the_stale_environment_variable_beats_the_file() {
    let fx = Fixture::new();
    let wt = fx.add_worktree("PROJ-73");
    fx.publish("PROJ-73");
    write_layout_config(&fx, "[clean]\nstale_after = \"5w\"\n");

    let out = gw_with_env(
        &fx.bare(),
        &["clean", "--yes", "--skip-provider", "github"],
        &[
            ("GW_REPO", fx.root.to_str().unwrap()),
            ("GW_STALE_DAYS", "0"),
        ],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(!wt.exists(), "GW_STALE_DAYS did not override stale_after");
}

#[test]
fn an_invalid_duration_is_a_usage_error_naming_the_file() {
    let fx = Fixture::new();
    write_layout_config(&fx, "[clean]\nstale_after = \"5x\"\n");

    let out = fx.gw(&["status"]);
    assert_eq!(out.code, 2, "stderr: {}", out.stderr);
    assert!(out.stderr.contains("stale_after"), "{}", out.stderr);
    assert!(out.stderr.contains("5x"), "{}", out.stderr);
}

#[test]
fn a_file_protected_branch_is_never_deleted() {
    let fx = Fixture::new();
    let wt = fx.add_worktree("release");
    commit_aged(&wt, 60);
    fx.publish("release");
    write_layout_config(
        &fx,
        "[clean]\nstale_after = \"1s\"\nprotect = [\"release\"]\n",
    );

    // Remove the worktree's checkout first, so the branch becomes an orphan
    // and `--delete-branches` would otherwise consider it.
    let out = fx.gw(&[
        "clean",
        "--yes",
        "--delete-branches",
        "--skip-provider",
        "github",
    ]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);

    let branches = std::process::Command::new("git")
        .current_dir(fx.bare())
        .args(["branch", "--list", "release"])
        .output()
        .expect("git branch");
    let branches = String::from_utf8_lossy(&branches.stdout).into_owned();
    assert!(
        branches.contains("release"),
        "a protected branch was deleted: {branches}"
    );
}

#[test]
fn an_invalid_protect_branch_is_a_usage_error() {
    let fx = Fixture::new();
    write_layout_config(&fx, "[clean]\nprotect = [\"-bad\"]\n");

    let out = fx.gw(&["status"]);
    assert_eq!(out.code, 2, "stderr: {}", out.stderr);
    assert!(out.stderr.contains("protect"), "{}", out.stderr);
}

#[test]
fn a_file_skipped_provider_does_not_run() {
    let fx = Fixture::new();
    write_layout_config(&fx, "[providers]\nskip = [\"github\"]\n");

    let out = fx.gw(&["convention", "show", "--json"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).expect("valid json");
    assert_eq!(json["settings"]["skip"][0], "github");
    let github = json["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "github")
        .expect("github is reported");
    assert_eq!(
        github["active"], false,
        "a file-skipped provider stayed active"
    );
}

#[test]
fn convention_show_reports_the_settings_and_their_origin() {
    let fx = Fixture::new();
    write_layout_config(&fx, "[clean]\nstale_after = \"5w\"\n");

    let out = fx.gw(&["convention", "show", "--json"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let json: serde_json::Value = serde_json::from_str(&out.stdout).expect("valid json");
    assert_eq!(json["settings"]["stale_after_secs"], 5 * 7 * 86_400);
    assert_eq!(json["settings"]["stale_after_origin"], "layout");

    let text = fx.gw(&["convention", "show"]);
    assert_eq!(text.code, 0, "stderr: {}", text.stderr);
    assert!(text.stdout.contains("stale_after 5w"), "{}", text.stdout);
}

#[test]
fn a_repos_root_in_a_layout_file_is_refused() {
    // The repos root is what gw searches to find a layout, so a value inside
    // one could never take effect. Refuse rather than silently ignore.
    let fx = Fixture::new();
    write_layout_config(&fx, "[discovery]\nrepos_root = \"~/elsewhere\"\n");

    let out = fx.gw(&["status"]);
    assert_eq!(out.code, 2, "stderr: {}", out.stderr);
    assert!(out.stderr.contains("global file"), "{}", out.stderr);
}

#[test]
fn the_global_file_sets_the_repos_root() {
    let fx = Fixture::new();
    let global = fx.root.join("global.toml");
    std::fs::write(
        &global,
        format!("[discovery]\nrepos_root = \"{}\"\n", fx.root.display()),
    )
    .unwrap();

    let out = gw_with_env(
        &fx.root,
        &["repos"],
        &[("GW_CONFIG", global.to_str().unwrap())],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout.contains(&fx.root.display().to_string()),
        "the configured repos root was not used: {}",
        out.stdout
    );
}

#[test]
fn a_malformed_global_file_does_not_silently_change_the_repos_root() {
    let fx = Fixture::new();
    let global = fx.root.join("global.toml");
    std::fs::write(&global, "[discovery]\nrepos_root = 5\n").unwrap();

    let out = gw_with_env(
        &fx.root,
        &["repos"],
        &[("GW_CONFIG", global.to_str().unwrap())],
    );
    assert_eq!(out.code, 2, "stderr: {}", out.stderr);
    assert!(out.stderr.contains("global.toml"), "{}", out.stderr);
}
