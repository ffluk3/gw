use std::path::Path;

use serde::Deserialize;

use crate::error::Error;
use gw_core::ProviderId;

/// Per-layout settings, resolved from `.gw.toml` and the command line.
///
/// Only values that are safe to commit belong here. Credentials stay in the
/// environment (see `providers::jira`), never in a file the layout may share.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    /// JIRA project keys a branch name may reference. Empty means any key.
    pub jira_projects: Vec<String>,
    /// JIRA base URL, when one is configured.
    pub jira_base_url: Option<String>,
    /// Providers the caller asked to skip for this run.
    pub skip_providers: Vec<ProviderId>,
    /// `--skip-github`, which is the pre-provider spelling of the same idea.
    pub skip_github: bool,
}

impl Settings {
    /// Load `<root>/.gw.toml`, then apply command-line overrides.
    ///
    /// Precedence is CLI flag (or its environment binding) over the file.
    /// `cli_projects` is the raw `--jira-project` value with an empty vector
    /// meaning "not given".
    pub fn load(
        root: &Path,
        cli_projects: &[String],
        cli_base_url: Option<&str>,
    ) -> Result<Self, Error> {
        let file = SettingsFile::load(&root.join(SETTINGS_FILE))?;
        let jira = file.jira.unwrap_or_default();

        let projects = if cli_projects.is_empty() {
            jira.project.map(ProjectValue::into_vec).unwrap_or_default()
        } else {
            cli_projects.to_vec()
        };

        Ok(Self {
            jira_projects: normalize_projects(projects),
            jira_base_url: cli_base_url
                .map(str::to_string)
                .or(jira.base_url)
                .filter(|s| !s.trim().is_empty()),
            skip_providers: Vec::new(),
            skip_github: false,
        })
    }

    /// Record the providers this invocation was asked to skip.
    #[must_use]
    pub fn with_skips(mut self, providers: Vec<ProviderId>, skip_github: bool) -> Self {
        self.skip_providers = providers;
        self.skip_github = skip_github;
        self
    }
}

/// Name of the per-layout settings file, at the layout root.
pub const SETTINGS_FILE: &str = ".gw.toml";

/// Read only the JIRA projects declared by a layout, ignoring CLI overrides.
///
/// Used by `gw repos`, which reports each repository's own configuration.
pub fn declared_projects(root: &Path) -> Result<Vec<String>, Error> {
    let file = SettingsFile::load(&root.join(SETTINGS_FILE))?;
    Ok(normalize_projects(
        file.jira
            .and_then(|j| j.project)
            .map(ProjectValue::into_vec)
            .unwrap_or_default(),
    ))
}

fn normalize_projects(projects: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(projects.len());
    for project in projects {
        let trimmed = project.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !out.iter().any(|p| p.eq_ignore_ascii_case(trimmed)) {
            out.push(trimmed.to_string());
        }
    }
    out
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsFile {
    #[serde(default)]
    jira: Option<JiraFile>,
}

impl SettingsFile {
    fn load(path: &Path) -> Result<Self, Error> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(Error::Other(format!("cannot read {}: {e}", path.display()))),
        };
        toml::from_str(&text).map_err(|e| {
            Error::Usage(format!(
                "invalid {}:\n  {e}\n\
                 Supported keys are [jira] project and [jira] base_url.",
                path.display()
            ))
        })
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct JiraFile {
    #[serde(default)]
    project: Option<ProjectValue>,
    #[serde(default)]
    base_url: Option<String>,
}

/// `project = "PROJ"` or `project = ["PROJ", "OPS"]`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ProjectValue {
    One(String),
    Many(Vec<String>),
}

impl ProjectValue {
    fn into_vec(self) -> Vec<String> {
        match self {
            Self::One(value) => vec![value],
            Self::Many(values) => values,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, text: &str) {
        std::fs::write(root.join(SETTINGS_FILE), text).unwrap();
    }

    #[test]
    fn missing_file_yields_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let settings = Settings::load(dir.path(), &[], None).unwrap();
        assert!(settings.jira_projects.is_empty());
        assert!(settings.jira_base_url.is_none());
    }

    #[test]
    fn reads_projects_and_base_url() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "[jira]\nproject = [\"PROJ\", \"OPS\"]\nbase_url = \"https://acme.atlassian.net\"\n",
        );
        let settings = Settings::load(dir.path(), &[], None).unwrap();
        assert_eq!(settings.jira_projects, vec!["PROJ", "OPS"]);
        assert_eq!(
            settings.jira_base_url.as_deref(),
            Some("https://acme.atlassian.net")
        );
    }

    #[test]
    fn cli_overrides_file() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "[jira]\nproject = \"FILE\"\n");
        let cli = vec!["CLI".to_string()];
        let settings = Settings::load(dir.path(), &cli, Some("https://cli.example")).unwrap();
        assert_eq!(settings.jira_projects, vec!["CLI"]);
        assert_eq!(
            settings.jira_base_url.as_deref(),
            Some("https://cli.example")
        );
    }

    #[test]
    fn rejects_malformed_and_unknown_keys() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "[jira]\nprojct = \"PROJ\"\n");
        assert!(matches!(
            Settings::load(dir.path(), &[], None),
            Err(Error::Usage(_))
        ));
    }

    #[test]
    fn normalizes_and_deduplicates_projects() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "[jira]\nproject = [\" PROJ \", \"proj\", \"\"]\n",
        );
        let settings = Settings::load(dir.path(), &[], None).unwrap();
        assert_eq!(settings.jira_projects, vec!["PROJ"]);
    }
}
