//! On-disk layout under `$XDG_DATA_HOME/omarchy-mihomo`.
//!
//! ```text
//! settings.yaml            managed settings
//! profiles.yaml            profile metadata and the current profile
//! profiles/<id>.yaml       downloaded or imported profile
//! overrides/global/        merge.yaml applied to every profile
//! overrides/<id>/          merge.yaml, rules.yaml, proxies.yaml, groups.yaml
//! runtime/                 mihomo home (-d): config.yaml, cache.db, geo data
//! snapshots/               previous runtime configs for rollback
//! ```

use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// `$XDG_DATA_HOME/omarchy-mihomo`, falling back to `~/.local/share`.
    pub fn default_root() -> PathBuf {
        omm_platform::data_root()
    }

    pub fn settings(&self) -> PathBuf {
        self.root.join("settings.yaml")
    }
    pub fn profiles_index(&self) -> PathBuf {
        self.root.join("profiles.yaml")
    }
    pub fn profile(&self, id: &str) -> PathBuf {
        self.root.join("profiles").join(format!("{id}.yaml"))
    }
    pub fn overrides(&self, scope: &str) -> PathBuf {
        self.root.join("overrides").join(scope)
    }
    pub fn runtime(&self) -> PathBuf {
        self.root.join("runtime")
    }
    pub fn runtime_config(&self) -> PathBuf {
        self.runtime().join("config.yaml")
    }
    pub fn controller_socket(&self) -> PathBuf {
        self.runtime().join(crate::settings::CONTROLLER_SOCKET)
    }
    pub fn snapshots(&self) -> PathBuf {
        self.root.join("snapshots")
    }

    /// Create the tree with private permissions: profiles hold credentials.
    pub fn ensure(&self) -> Result<()> {
        for dir in [self.root.clone(), self.root.join("profiles"), self.overrides("global"), self.runtime(), self.snapshots()] {
            fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).map_err(|e| Error::io(&dir, e))?;
        }
        Ok(())
    }
}

/// Write via a temp file in the same directory and rename, so readers
/// (and mihomo's file watcher) never see a half-written file. Mode 0600.
pub fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    let tmp = dir.join(format!(".{}.tmp", path.file_name().and_then(|n| n.to_str()).unwrap_or("file")));
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(|e| Error::io(&tmp, e))?;
    f.write_all(data).and_then(|_| f.sync_all()).map_err(|e| Error::io(&tmp, e))?;
    fs::rename(&tmp, path).map_err(|e| Error::io(path, e))
}

/// Read a file, treating a missing one as `None`.
pub fn read_optional(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io(path, e)),
    }
}
