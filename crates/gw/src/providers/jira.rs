use std::process::{Command, Stdio};

use gw_core::{ProviderId, TicketId, TicketObservation, TicketStatus};

use crate::error::Error;
use crate::providers::{Limits, TicketProvider};

/// A JIRA Cloud REST adapter.
///
/// Transport is `curl`, which the shell implementation already required. That
/// avoids linking a TLS stack into a tool whose only network call is one
/// blocking POST, and keeps the build dependency-free. Base URL, credentials,
/// and fields fetched are all configuration; nothing about a specific tenant is
/// compiled in.
pub struct JiraRest {
    id: ProviderId,
    base_url: String,
    done_category: String,
    user: String,
    token: String,
}

impl JiraRest {
    /// Build an adapter from the merged configuration, or `None` when JIRA is
    /// not configured. Callers treat absence as "provider inactive", never an
    /// error.
    ///
    /// The base URL comes from `GW_JIRA_BASE_URL` first, then from
    /// `[convention.providers.jira] base_url` in whichever configuration file
    /// supplies it. The environment is a per-invocation override, so it sits
    /// above every file. Credentials come from the environment only, so they
    /// are never written to a file that might be committed.
    pub fn from_config(loaded: &crate::convention_file::Loaded) -> Option<Self> {
        let provider = loaded.providers.iter().find(|p| p.id.as_str() == "jira");
        // A `jira` entry must exist before the environment is consulted.
        // Otherwise an unrelated `GW_JIRA_BASE_URL` in someone's profile would
        // switch on a provider this layout deliberately left out.
        provider?;
        let base_url = std::env::var("GW_JIRA_BASE_URL")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| provider.and_then(|p| p.base_url.clone()))
            .filter(|value| !value.is_empty())?;
        let user = std::env::var("GW_JIRA_USER").ok()?;
        let token = std::env::var("GW_JIRA_API_TOKEN").ok()?;
        if base_url.trim().is_empty() || user.trim().is_empty() || token.trim().is_empty() {
            return None;
        }
        Some(Self {
            id: ProviderId::parse("jira").expect("literal is a valid provider id"),
            base_url: base_url.trim_end_matches('/').to_string(),
            done_category: provider
                .and_then(|p| p.done_status_category.clone())
                .unwrap_or_else(|| "done".to_string()),
            user,
            token,
        })
    }

    fn post(&self, path: &str, body: &str) -> Result<String, Error> {
        let url = format!("{}{}", self.base_url, path);
        if !url.starts_with("https://") {
            return Err(Error::Lookup {
                service: "jira",
                detail: format!("base URL must be https: {url}"),
            });
        }

        // Credentials travel through a curl config file with 0600 permissions,
        // so unlike `-u user:token` they never appear in the process list.
        let config = ConfigFile::new(&format!(
            "user = \"{}\"\nheader = \"Content-Type: application/json\"\nheader = \"Accept: application/json\"\n",
            escape_curl_config(&format!("{}:{}", self.user, self.token))
        ))
        .map_err(|e| Error::Lookup {
            service: "jira",
            detail: format!("cannot create credential file: {e}"),
        })?;

        let output = Command::new("curl")
            .args([
                "--silent",
                "--show-error",
                "--fail-with-body",
                "--max-time",
                "30",
                "--config",
                &config.path().to_string_lossy(),
                "-X",
                "POST",
                "--data-binary",
                "@-",
                &url,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(body.as_bytes())?;
                }
                child.wait_with_output()
            })
            .map_err(|e| Error::Lookup {
                service: "jira",
                detail: format!("could not run `curl` ({e})"),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Lookup {
                service: "jira",
                detail: stderr.trim().to_string(),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

impl TicketProvider for JiraRest {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn lookup(&self, ids: &[TicketId], limits: Limits) -> Result<Vec<TicketObservation>, Error> {
        let mut observations = Vec::with_capacity(ids.len());
        if ids.is_empty() {
            return Ok(observations);
        }

        // The batch may exceed the provider's request limit; ask in chunks so a
        // large worktree directory cannot produce one oversized JQL request.
        for chunk in ids.chunks(limits.max_queries.max(1)) {
            observations.extend(self.lookup_chunk(chunk)?);
        }
        Ok(observations)
    }
}

impl JiraRest {
    fn lookup_chunk(&self, ids: &[TicketId]) -> Result<Vec<TicketObservation>, Error> {
        let jql = format!(
            "key in ({})",
            ids.iter()
                .map(TicketId::lookup_key)
                .collect::<Vec<_>>()
                .join(",")
        );
        let payload = serde_json::json!({
            "jql": jql,
            "fields": ["status", "resolution", "summary"],
            "maxResults": 100,
        })
        .to_string();

        let body = self.post("/rest/api/3/search/jql", &payload)?;
        let parsed: serde_json::Value = serde_json::from_str(&body).map_err(|e| Error::Lookup {
            service: "jira",
            detail: format!("invalid JSON: {e}"),
        })?;

        if let Some(err) = parsed
            .get("errorMessages")
            .and_then(|v| v.get(0))
            .and_then(|v| v.as_str())
        {
            return Err(Error::Lookup {
                service: "jira",
                detail: err.to_string(),
            });
        }

        let mut found = std::collections::HashMap::new();
        for issue in parsed
            .get("issues")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
        {
            let Some(key) = issue.get("key").and_then(|v| v.as_str()) else {
                continue;
            };
            let fields = issue.get("fields");
            let status = self.status_of(fields);
            let summary = fields
                .and_then(|f| f.get("summary"))
                .and_then(|s| s.as_str())
                .map(str::to_string);
            if let Ok(id) = TicketId::parse(key) {
                found.insert(id.clone(), (status, summary));
            }
        }

        // Every identifier gets an answer. An identifier the tracker did not
        // return is `Unknown`: the convention proved the branch names it, but
        // no state could be observed, and silence is not "open" or "closed".
        Ok(ids
            .iter()
            .map(|id| {
                let (status, summary) = found.remove(id).unwrap_or_else(|| {
                    (
                        TicketStatus::Unknown {
                            detail: "not returned by the tracker".into(),
                        },
                        None,
                    )
                });
                TicketObservation {
                    provider: self.id.clone(),
                    id: id.clone(),
                    strength: gw_core::Strength::Identity,
                    status,
                    summary,
                }
            })
            .collect())
    }

    fn status_of(&self, fields: Option<&serde_json::Value>) -> TicketStatus {
        let category = fields
            .and_then(|f| f.get("status"))
            .and_then(|s| s.get("statusCategory"))
            .and_then(|c| c.get("key"))
            .and_then(|k| k.as_str());
        match category {
            Some(category) if category.eq_ignore_ascii_case(&self.done_category()) => {
                let resolution = fields
                    .and_then(|f| f.get("resolution"))
                    .and_then(|r| r.get("name"))
                    .and_then(|n| n.as_str())
                    .map(str::to_string);
                TicketStatus::Closed { resolution }
            }
            Some(_) => TicketStatus::Open,
            // No status category is unreadable, not open: treating it as open
            // would silently discard a "done" answer.
            None => TicketStatus::Unknown {
                detail: "response had no status category".into(),
            },
        }
    }

    fn done_category(&self) -> String {
        self.done_category.clone()
    }
}

/// A temporary file holding curl config, removed on drop.
struct ConfigFile {
    path: std::path::PathBuf,
}

impl ConfigFile {
    fn new(contents: &str) -> std::io::Result<Self> {
        use std::io::Write;
        use std::sync::atomic::{AtomicU64, Ordering};

        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "gw-curl-{}-{}-{}.conf",
            std::process::id(),
            nanos,
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));

        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path)?;
        file.write_all(contents.as_bytes())?;
        Ok(Self { path })
    }

    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for ConfigFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Escape a value for inclusion in a double-quoted curl config value.
fn escape_curl_config(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curl_config_escaping() {
        assert_eq!(escape_curl_config("a\"b"), "a\\\"b");
        assert_eq!(escape_curl_config("a\\b"), "a\\\\b");
    }

    fn adapter() -> JiraRest {
        JiraRest {
            id: ProviderId::parse("jira").unwrap(),
            base_url: "https://example.atlassian.net".into(),
            done_category: "done".into(),
            user: "u".into(),
            token: "t".into(),
        }
    }

    #[test]
    fn done_category_is_closed_with_its_resolution() {
        let fields = serde_json::json!({
            "status": { "statusCategory": { "key": "done" } },
            "resolution": { "name": "Fixed" }
        });
        assert_eq!(
            adapter().status_of(Some(&fields)),
            TicketStatus::Closed {
                resolution: Some("Fixed".into())
            }
        );
    }

    #[test]
    fn other_categories_are_open() {
        let fields =
            serde_json::json!({ "status": { "statusCategory": { "key": "indeterminate" } } });
        assert_eq!(adapter().status_of(Some(&fields)), TicketStatus::Open);
    }

    #[test]
    fn a_missing_category_is_unknown_not_open() {
        let fields = serde_json::json!({ "summary": "no status" });
        assert!(matches!(
            adapter().status_of(Some(&fields)),
            TicketStatus::Unknown { .. }
        ));
    }

    #[test]
    fn configuration_without_a_base_url_or_credentials_is_inactive() {
        let _lock = crate::providers::test_support::credential_lock();
        std::env::remove_var("GW_JIRA_BASE_URL");
        std::env::remove_var("GW_JIRA_USER");
        std::env::remove_var("GW_JIRA_API_TOKEN");

        // No provider table at all.
        assert!(JiraRest::from_config(&crate::convention_file::Loaded::default()).is_none());

        // A table without a base URL.
        let missing_url = crate::convention_file::Loaded {
            providers: vec![crate::convention_file::ProviderConfig {
                id: ProviderId::parse("jira").unwrap(),
                kind: "jira".into(),
                base_url: None,
                done_status_category: None,
                command: None,
            }],
            ..Default::default()
        };
        assert!(JiraRest::from_config(&missing_url).is_none());

        // A base URL but no credentials.
        let configured = crate::convention_file::Loaded {
            providers: vec![crate::convention_file::ProviderConfig {
                id: ProviderId::parse("jira").unwrap(),
                kind: "jira".into(),
                base_url: Some("https://example.atlassian.net/".into()),
                done_status_category: None,
                command: None,
            }],
            ..Default::default()
        };
        assert!(JiraRest::from_config(&configured).is_none());
    }

    fn jira_loaded(base_url: Option<&str>) -> crate::convention_file::Loaded {
        crate::convention_file::Loaded {
            providers: vec![crate::convention_file::ProviderConfig {
                id: ProviderId::parse("jira").unwrap(),
                kind: "jira".into(),
                base_url: base_url.map(str::to_string),
                done_status_category: None,
                command: None,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn the_environment_supplies_a_missing_base_url() {
        let _lock = crate::providers::test_support::credential_lock();
        std::env::set_var("GW_JIRA_BASE_URL", "https://from-env.atlassian.net");
        std::env::set_var("GW_JIRA_USER", "me@example.com");
        std::env::set_var("GW_JIRA_API_TOKEN", "token");

        let loaded = jira_loaded(None);
        let adapter = JiraRest::from_config(&loaded).expect("the environment supplied the URL");
        assert_eq!(adapter.base_url, "https://from-env.atlassian.net");

        std::env::remove_var("GW_JIRA_BASE_URL");
        std::env::remove_var("GW_JIRA_USER");
        std::env::remove_var("GW_JIRA_API_TOKEN");
    }

    #[test]
    fn the_environment_wins_over_the_file_base_url() {
        let _lock = crate::providers::test_support::credential_lock();
        std::env::set_var("GW_JIRA_BASE_URL", "https://from-env.atlassian.net");
        std::env::set_var("GW_JIRA_USER", "me@example.com");
        std::env::set_var("GW_JIRA_API_TOKEN", "token");

        let loaded = jira_loaded(Some("https://from-file.atlassian.net"));
        let adapter = JiraRest::from_config(&loaded).expect("configured");
        assert_eq!(adapter.base_url, "https://from-env.atlassian.net");

        std::env::remove_var("GW_JIRA_BASE_URL");
        std::env::remove_var("GW_JIRA_USER");
        std::env::remove_var("GW_JIRA_API_TOKEN");
    }

    #[test]
    fn an_empty_environment_base_url_falls_back_to_the_file() {
        let _lock = crate::providers::test_support::credential_lock();
        std::env::set_var("GW_JIRA_BASE_URL", "   ");
        std::env::set_var("GW_JIRA_USER", "me@example.com");
        std::env::set_var("GW_JIRA_API_TOKEN", "token");

        let loaded = jira_loaded(Some("https://from-file.atlassian.net"));
        let adapter = JiraRest::from_config(&loaded).expect("configured");
        assert_eq!(adapter.base_url, "https://from-file.atlassian.net");

        std::env::remove_var("GW_JIRA_BASE_URL");
        std::env::remove_var("GW_JIRA_USER");
        std::env::remove_var("GW_JIRA_API_TOKEN");
    }

    #[test]
    fn the_environment_cannot_activate_a_layout_that_omits_jira() {
        let _lock = crate::providers::test_support::credential_lock();
        std::env::set_var("GW_JIRA_BASE_URL", "https://from-env.atlassian.net");
        std::env::set_var("GW_JIRA_USER", "me@example.com");
        std::env::set_var("GW_JIRA_API_TOKEN", "token");

        // No `jira` provider entry: the fallback must not switch it on.
        assert!(JiraRest::from_config(&crate::convention_file::Loaded::default()).is_none());

        std::env::remove_var("GW_JIRA_BASE_URL");
        std::env::remove_var("GW_JIRA_USER");
        std::env::remove_var("GW_JIRA_API_TOKEN");
    }

    #[test]
    fn a_non_https_base_url_is_refused_without_running_curl() {
        let mut adapter = adapter();
        adapter.base_url = "http://example.atlassian.net".into();
        let error = adapter.post("/rest/api/3/search/jql", "{}").unwrap_err();
        assert!(error.to_string().contains("must be https"), "{error}");
    }

    #[test]
    fn empty_lookup_is_inert() {
        let observations = adapter().lookup(&[], Limits::default()).unwrap();
        assert!(observations.is_empty());
    }
}
