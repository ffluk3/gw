use std::collections::HashMap;
use std::process::{Command, Stdio};

use gw_core::{TicketKey, TicketStatus};

use crate::error::Error;

/// Ticket state for a set of keys.
#[derive(Debug, Clone, Default)]
pub struct TicketInfo {
    pub status: HashMap<String, TicketStatus>,
    pub summary: HashMap<String, String>,
}

pub trait JiraBackend {
    fn lookup(&self, keys: &[TicketKey]) -> Result<TicketInfo, Error>;
}

/// JIRA Cloud REST adapter.
///
/// Transport is `curl`, which the shell implementation already required. That
/// avoids linking a TLS stack into a tool whose only network call is one
/// blocking POST, and keeps the build dependency-free. Base URL, credentials,
/// and fields fetched are all configuration; nothing about a specific tenant is
/// compiled in.
pub struct JiraRest {
    base_url: String,
    user: String,
    token: String,
}

impl JiraRest {
    /// Build an adapter from the resolved settings, or `None` when JIRA is not
    /// configured. Callers treat absence as "lookup disabled", never an error.
    ///
    /// The base URL comes from `--jira-url`, `GW_JIRA_BASE_URL`, or `.gw.toml`.
    /// Credentials come from the environment only, so they are never written to
    /// a file that might be committed.
    pub fn from_settings(settings: &crate::settings::Settings) -> Option<Self> {
        let base_url = settings.jira_base_url.clone()?;
        let user = std::env::var("GW_JIRA_USER")
            .or_else(|_| std::env::var("JIRA_USER"))
            .ok()?;
        let token = std::env::var("GW_JIRA_API_TOKEN")
            .or_else(|_| std::env::var("JIRA_API_TOKEN"))
            .ok()?;
        if base_url.trim().is_empty() || user.trim().is_empty() || token.trim().is_empty() {
            return None;
        }
        Some(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
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

impl JiraBackend for JiraRest {
    fn lookup(&self, keys: &[TicketKey]) -> Result<TicketInfo, Error> {
        let mut info = TicketInfo::default();
        if keys.is_empty() {
            return Ok(info);
        }

        let jql = format!(
            "key in ({})",
            keys.iter()
                .map(gw_core::TicketKey::as_str)
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
            let done = fields
                .and_then(|f| f.get("status"))
                .and_then(|s| s.get("statusCategory"))
                .and_then(|c| c.get("key"))
                .and_then(|k| k.as_str())
                == Some("done");
            let status = if done {
                let resolution = fields
                    .and_then(|f| f.get("resolution"))
                    .and_then(|r| r.get("name"))
                    .and_then(|n| n.as_str())
                    .map(str::to_string);
                TicketStatus::Closed { resolution }
            } else {
                TicketStatus::Open
            };
            info.status.insert(key.to_string(), status);
            if let Some(summary) = fields
                .and_then(|f| f.get("summary"))
                .and_then(|s| s.as_str())
            {
                info.summary.insert(key.to_string(), summary.to_string());
            }
        }
        Ok(info)
    }
}

/// A backend for when JIRA is not configured.
pub struct NoJira;

impl JiraBackend for NoJira {
    fn lookup(&self, _keys: &[TicketKey]) -> Result<TicketInfo, Error> {
        Ok(TicketInfo::default())
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

    #[test]
    fn no_jira_backend_is_inert() {
        let info = NoJira.lookup(&[]).unwrap();
        assert!(info.status.is_empty());
    }
}
