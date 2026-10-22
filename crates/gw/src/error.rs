use std::fmt;

/// Failures surfaced to the user. Each variant maps to a documented exit code.
#[derive(Debug)]
pub enum Error {
    /// Bad invocation or unusable configuration.
    Usage(String),
    /// The repository is not in the expected layout.
    Layout(String),
    /// A git command failed.
    Git { context: String, stderr: String },
    /// A network-backed lookup failed.
    Lookup {
        service: &'static str,
        detail: String,
    },
    /// Anything else.
    Other(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Usage(msg) | Error::Layout(msg) | Error::Other(msg) => write!(f, "{msg}"),
            Error::Git { context, stderr } => {
                if stderr.trim().is_empty() {
                    write!(f, "{context}")
                } else {
                    write!(f, "{context}: {}", stderr.trim())
                }
            }
            Error::Lookup { service, detail } => write!(f, "{service}: {detail}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Other(e.to_string())
    }
}

#[cfg(test)]
mod coverage_tests {
    use super::*;

    #[test]
    fn every_variant_renders_a_message() {
        assert_eq!(Error::Usage("u".into()).to_string(), "u");
        assert_eq!(Error::Layout("l".into()).to_string(), "l");
        assert_eq!(Error::Other("o".into()).to_string(), "o");
        assert_eq!(
            Error::Lookup {
                service: "jira",
                detail: "boom".into()
            }
            .to_string(),
            "jira: boom"
        );
    }

    #[test]
    fn a_git_error_drops_empty_stderr() {
        assert_eq!(
            Error::Git {
                context: "git status".into(),
                stderr: "   ".into()
            }
            .to_string(),
            "git status"
        );
        assert_eq!(
            Error::Git {
                context: "git status".into(),
                stderr: "fatal".into()
            }
            .to_string(),
            "git status: fatal"
        );
    }

    #[test]
    fn an_io_error_becomes_other() {
        let io = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
        assert_eq!(Error::from(io).to_string(), "gone");
    }
}
