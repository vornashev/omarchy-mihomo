use std::io;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot reach mihomo at {path}: {source}")]
    Connect { path: String, source: io::Error },
    #[error("http: {0}")]
    Http(#[from] hyper::Error),
    #[error("request: {0}")]
    Request(#[from] hyper::http::Error),
    #[error("mihomo returned {status}: {message}")]
    Status { status: u16, message: String },
    #[error("decode: {0}")]
    Json(#[from] serde_json::Error),
    #[error("timed out after {0:?}")]
    Timeout(std::time::Duration),
}

impl Error {
    /// True when the core is not running or its socket is gone.
    pub fn is_unreachable(&self) -> bool {
        matches!(self, Error::Connect { .. })
    }
}
