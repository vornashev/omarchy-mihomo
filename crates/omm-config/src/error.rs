use std::path::PathBuf;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("{what}: invalid YAML: {source}")]
    Yaml { what: String, source: serde_yaml_ng::Error },
    #[error("{0}")]
    Invalid(String),
    #[error("download failed: {0}")]
    Fetch(String),
    #[error("mihomo rejected the config:\n{0}")]
    Rejected(String),
    #[error("profile {0} not found")]
    NoProfile(String),
}

impl Error {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io { path: path.into(), source }
    }

    pub fn yaml(what: impl Into<String>, source: serde_yaml_ng::Error) -> Self {
        Error::Yaml { what: what.into(), source }
    }
}
