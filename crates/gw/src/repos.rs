use std::path::{Path, PathBuf};

use crate::error::Error;

/// Environment variable that overrides the repos root.
pub const REPOS_ROOT_ENV: &str = "GW_REPOS_ROOT";

/// Root directory that holds layouts, one per `<owner>/<name>`.
///
/// `GW_REPOS_ROOT` wins; otherwise `~/repos`. A leading `~/` in the variable
/// is expanded, because a value set programmatically is not shell-expanded.
#[must_use]
pub fn repos_root() -> PathBuf {
    match std::env::var(REPOS_ROOT_ENV) {
        Ok(value) if !value.trim().is_empty() => expand_tilde(value.trim()),
        _ => home_dir().join("repos"),
    }
}

/// Resolve a `--repo` value that may be a path or a slug.
///
/// 1. An existing path (absolute, or relative to `cwd`) is used as given.
/// 2. A value that looks like a path but does not exist is an error, so a typo
///    is never mistaken for a repository name.
/// 3. `owner/name` resolves under the repos root.
/// 4. A bare `name` resolves by scanning the repos root; it must be unique.
pub fn resolve_repo_arg(arg: &str, cwd: &Path) -> Result<PathBuf, Error> {
    let value = arg.trim();
    if value.is_empty() {
        return Err(Error::Usage("--repo is empty".into()));
    }
    let expanded = expand_tilde(value);

    let candidate = if expanded.is_absolute() {
        expanded.clone()
    } else {
        cwd.join(&expanded)
    };
    if candidate.is_dir() {
        return Ok(canonical(candidate));
    }
    if is_path_shaped(value) {
        return Err(Error::Usage(format!(
            "{} is not a directory",
            candidate.display()
        )));
    }

    let root = repos_root();
    if value.contains('/') {
        let path = root.join(value);
        if path.is_dir() {
            return Ok(canonical(path));
        }
        return Err(Error::Usage(not_found(value, &root)));
    }

    let matches = find_by_name(&root, value);
    match matches.len() {
        1 => Ok(canonical(
            matches.into_iter().next().expect("length checked"),
        )),
        0 => Err(Error::Usage(not_found(value, &root))),
        _ => Err(Error::Usage(ambiguous(value, &root, &matches))),
    }
}

/// Layout roots directly under `root`, as `<owner>/<name>` or `<name>`.
///
/// Both shapes are accepted so a flat repos directory keeps working.
#[must_use]
pub fn discover(root: &Path) -> Vec<(String, PathBuf)> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return found;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if is_layout(&path) {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                found.push((name.to_string(), path));
            }
            continue;
        }
        let Ok(children) = std::fs::read_dir(&path) else {
            continue;
        };
        for child in children.flatten() {
            let child_path = child.path();
            if !child_path.is_dir() || !is_layout(&child_path) {
                continue;
            }
            let Some(name) = child_path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some(owner) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            found.push((format!("{owner}/{name}"), child_path));
        }
    }

    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

/// Repositories under `root` that are named exactly `name`.
fn find_by_name(root: &Path, name: &str) -> Vec<PathBuf> {
    discover(root)
        .into_iter()
        .filter(|(slug, _)| slug.rsplit('/').next() == Some(name))
        .map(|(_, path)| path)
        .collect()
}

/// A layout root is recognizable by its bare repository, not its children.
#[must_use]
pub fn is_layout(path: &Path) -> bool {
    path.join(".bare").join("HEAD").is_file()
}

/// Sibling layouts under `dir`, for error messages.
pub fn layouts_under(dir: &Path) -> Vec<String> {
    discover(dir).into_iter().map(|(slug, _)| slug).collect()
}

fn not_found(arg: &str, root: &Path) -> String {
    let available = layouts_under(root);
    let mut message = format!("no repository named '{arg}' under {}", root.display());
    if available.is_empty() {
        message.push_str("\n\nSet GW_REPOS_ROOT to the directory that holds your layouts.");
    } else {
        message.push_str("\n\nAvailable repositories:");
        for slug in available.iter().take(20) {
            message.push_str(&format!("\n  {slug}"));
        }
        if available.len() > 20 {
            message.push_str(&format!("\n  ... and {} more", available.len() - 20));
        }
    }
    message
}

fn ambiguous(arg: &str, root: &Path, matches: &[PathBuf]) -> String {
    let mut message = format!(
        "'{arg}' matches more than one repository under {}:",
        root.display()
    );
    for path in matches {
        let relative = path.strip_prefix(root).unwrap_or(path);
        message.push_str(&format!("\n  {}", relative.display()));
    }
    message.push_str("\n\nPass the owner as well, for example --repo owner/");
    message.push_str(arg);
    message
}

fn is_path_shaped(value: &str) -> bool {
    value.starts_with('/')
        || value.starts_with("./")
        || value.starts_with("../")
        || value.starts_with('~')
        || value == "."
        || value == ".."
}

fn expand_tilde(value: &str) -> PathBuf {
    if value == "~" {
        return home_dir();
    }
    value
        .strip_prefix("~/")
        .map_or_else(|| PathBuf::from(value), |rest| home_dir().join(rest))
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from)
}

/// Resolve symlinks so a layout is compared the same way git reports it.
fn canonical(path: PathBuf) -> PathBuf {
    std::fs::canonicalize(&path).unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_layout(root: &Path, slug: &str) -> PathBuf {
        let path = root.join(slug);
        std::fs::create_dir_all(path.join(".bare")).unwrap();
        std::fs::write(path.join(".bare").join("HEAD"), "ref: refs/heads/main\n").unwrap();
        path
    }

    #[test]
    fn discovers_nested_and_flat_layouts() {
        let dir = tempfile::tempdir().unwrap();
        make_layout(dir.path(), "acme/widgets");
        make_layout(dir.path(), "solo");
        std::fs::create_dir_all(dir.path().join("acme/not-a-repo")).unwrap();

        let slugs: Vec<String> = discover(dir.path()).into_iter().map(|(s, _)| s).collect();
        assert_eq!(slugs, vec!["acme/widgets", "solo"]);
    }

    #[test]
    fn resolves_owner_name_under_root() {
        let dir = tempfile::tempdir().unwrap();
        let layout = make_layout(dir.path(), "acme/widgets");
        // SAFETY: tests in this module are serialized by the environment guard
        // below; see `env_guard`.
        let _guard = env_guard(dir.path());

        let resolved = resolve_repo_arg("acme/widgets", Path::new("/tmp")).unwrap();
        assert_eq!(resolved, canonical(layout));
    }

    #[test]
    fn resolves_unique_bare_name() {
        let dir = tempfile::tempdir().unwrap();
        let layout = make_layout(dir.path(), "acme/widgets");
        let _guard = env_guard(dir.path());

        let resolved = resolve_repo_arg("widgets", Path::new("/tmp")).unwrap();
        assert_eq!(resolved, canonical(layout));
    }

    #[test]
    fn ambiguous_bare_name_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        make_layout(dir.path(), "acme/widgets");
        make_layout(dir.path(), "other/widgets");
        let _guard = env_guard(dir.path());

        assert!(matches!(
            resolve_repo_arg("widgets", Path::new("/tmp")),
            Err(Error::Usage(message)) if message.contains("more than one")
        ));
    }

    #[test]
    fn path_wins_over_name() {
        let dir = tempfile::tempdir().unwrap();
        let local = dir.path().join("checkout");
        std::fs::create_dir_all(&local).unwrap();

        let resolved = resolve_repo_arg("./checkout", dir.path()).unwrap();
        assert_eq!(resolved, canonical(local));
    }

    /// Set `GW_REPOS_ROOT` for the duration of one test.
    ///
    /// Tests that touch process environment must not run concurrently with
    /// each other. `cargo test` runs the module's tests in parallel by default,
    /// so this guard serializes the ones that call it.
    fn env_guard(root: &Path) -> EnvGuard {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let previous = std::env::var_os(REPOS_ROOT_ENV);
        std::env::set_var(REPOS_ROOT_ENV, root);
        EnvGuard {
            _lock: lock,
            previous,
        }
    }

    /// Share the same process-wide lock with the coverage tests.
    pub(super) fn env_guard_for_root(root: &Path) -> EnvGuard {
        env_guard(root)
    }

    pub(super) struct EnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        previous: Option<std::ffi::OsString>,
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(value) => std::env::set_var(REPOS_ROOT_ENV, value),
                None => std::env::remove_var(REPOS_ROOT_ENV),
            }
        }
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::*;

    fn layout(dir: &Path, slug: &str) -> PathBuf {
        let path = dir.join(slug);
        std::fs::create_dir_all(path.join(".bare")).unwrap();
        std::fs::write(path.join(".bare").join("HEAD"), "ref: refs/heads/main\n").unwrap();
        path
    }

    #[test]
    fn an_empty_argument_is_a_usage_error() {
        assert!(matches!(
            resolve_repo_arg("   ", Path::new("/tmp")),
            Err(Error::Usage(_))
        ));
    }

    #[test]
    fn a_path_shaped_argument_that_is_missing_is_a_usage_error() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(matches!(
            resolve_repo_arg("./nope", tmp.path()),
            Err(Error::Usage(message)) if message.contains("not a directory")
        ));
        assert!(matches!(
            resolve_repo_arg("/definitely/not/here", tmp.path()),
            Err(Error::Usage(_))
        ));
    }

    #[test]
    fn an_owner_name_that_is_missing_lists_the_root() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = super::tests::env_guard_for_root(tmp.path());
        let err = resolve_repo_arg("acme/missing", Path::new("/tmp")).unwrap_err();
        assert!(err.to_string().contains("no repository named"));
    }

    #[test]
    fn an_unknown_name_lists_available_repositories() {
        let tmp = tempfile::tempdir().unwrap();
        layout(tmp.path(), "acme/widgets");
        let _guard = super::tests::env_guard_for_root(tmp.path());
        let err = resolve_repo_arg("nothing", Path::new("/tmp")).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("acme/widgets"), "{message}");
    }

    #[test]
    fn more_than_twenty_layouts_are_summarized() {
        let tmp = tempfile::tempdir().unwrap();
        for i in 0..25 {
            layout(tmp.path(), &format!("acme/repo{i:02}"));
        }
        let _guard = super::tests::env_guard_for_root(tmp.path());
        let err = resolve_repo_arg("nothing", Path::new("/tmp")).unwrap_err();
        assert!(err.to_string().contains("... and 5 more"), "{err}");
    }

    #[test]
    fn an_ambiguous_name_suggests_the_owner_form() {
        let tmp = tempfile::tempdir().unwrap();
        layout(tmp.path(), "acme/widgets");
        layout(tmp.path(), "other/widgets");
        let _guard = super::tests::env_guard_for_root(tmp.path());
        let err = resolve_repo_arg("widgets", Path::new("/tmp")).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("more than one"), "{message}");
        assert!(message.contains("--repo owner/widgets"), "{message}");
    }

    #[test]
    fn a_tilde_path_is_expanded() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = super::tests::env_guard_for_root(tmp.path());
        // `~/<name>` must be treated as a path, not a repository name.
        let err = resolve_repo_arg("~/nope-not-here", Path::new("/tmp")).unwrap_err();
        assert!(err.to_string().contains("not a directory"), "{err}");
    }

    #[test]
    fn the_repos_root_env_wins_and_expands_a_tilde() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = super::tests::env_guard_for_root(tmp.path());
        assert_eq!(repos_root(), tmp.path());

        std::env::set_var(REPOS_ROOT_ENV, "~/repos-test");
        let home = std::env::var_os("HOME").unwrap();
        assert_eq!(repos_root(), PathBuf::from(&home).join("repos-test"));

        // A blank value falls back to the default.
        std::env::set_var(REPOS_ROOT_ENV, "  ");
        assert_eq!(repos_root(), PathBuf::from(&home).join("repos"));
    }

    #[test]
    fn a_missing_root_discovers_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(discover(&tmp.path().join("absent")).is_empty());
    }
}
