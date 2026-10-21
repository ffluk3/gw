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
            Error::Usage(msg) => write!(f, "{msg}"),
            Error::Layout(msg) => write!(f, "{msg}"),
            Error::Git { context, stderr } => {
                if stderr.trim().is_empty() {
                    write!(f, "{context}")
                } else {
                    write!(f, "{context}: {}", stderr.trim())
                }
            }
            Error::Lookup { service, detail } => write!(f, "{service}: {detail}"),
            Error::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Other(e.to_string())
    }
}
