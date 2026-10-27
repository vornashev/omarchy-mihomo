//! The live config in the core's home: validation, install, snapshots.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command;

use crate::error::{Error, Result};
use crate::paths::{Paths, write_atomic};

/// Snapshots kept for rollback.
pub const KEEP_SNAPSHOTS: usize = 10;
const VALIDATE_TIMEOUT: Duration = Duration::from_secs(60);

pub struct Runtime {
    pub paths: Paths,
    /// mihomo binary used for `-t`.
    pub binary: PathBuf,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Snapshot {
    pub name: String,
    /// Unix seconds.
    pub time: u64,
    #[serde(skip)]
    millis: u64,
}

impl Runtime {
    pub fn new(paths: Paths, binary: impl Into<PathBuf>) -> Self {
        Self { paths, binary: binary.into() }
    }

    /// Run `mihomo -t` on a candidate placed in the core's home, so relative
    /// provider paths and geo data resolve exactly as they will live.
    pub async fn validate(&self, yaml: &str) -> Result<()> {
        let home = self.paths.runtime();
        let candidate = home.join(".candidate.yaml");
        write_atomic(&candidate, yaml.as_bytes())?;
        let run = Command::new(&self.binary).arg("-t").arg("-d").arg(&home).arg("-f").arg(&candidate).kill_on_drop(true).output();
        let result = tokio::time::timeout(VALIDATE_TIMEOUT, run).await;
        let _ = std::fs::remove_file(&candidate);

        let out = match result {
            Err(_) => return Err(Error::Rejected(format!("validation timed out after {VALIDATE_TIMEOUT:?}"))),
            Ok(Err(e)) => return Err(Error::io(&self.binary, e)),
            Ok(Ok(out)) => out,
        };
        if out.status.success() {
            return Ok(());
        }
        let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
        Err(Error::Rejected(summarize(&text)))
    }

    /// Replace `config.yaml`, keeping the previous one as a snapshot.
    pub fn install(&self, yaml: &str) -> Result<PathBuf> {
        let live = self.paths.runtime_config();
        if let Ok(prev) = std::fs::read(&live) {
            if prev == yaml.as_bytes() {
                return Ok(live);
            }
            let millis = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
            let name = format!("{millis}.yaml");
            write_atomic(&self.paths.snapshots().join(name), &prev)?;
            self.prune()?;
        }
        write_atomic(&live, yaml.as_bytes())?;
        Ok(live)
    }

    /// Newest first.
    pub fn snapshots(&self) -> Result<Vec<Snapshot>> {
        let dir = self.paths.snapshots();
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(Error::io(dir, e)),
        };
        let mut list: Vec<Snapshot> = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                let millis: u64 = name.strip_suffix(".yaml")?.parse().ok()?;
                Some(Snapshot { name, time: millis / 1000, millis })
            })
            .collect();
        list.sort_by_key(|s| std::cmp::Reverse(s.millis));
        Ok(list)
    }

    pub fn snapshot_text(&self, name: &str) -> Result<String> {
        // Names come from clients; never let them walk out of the directory.
        if name.contains('/') || name.starts_with('.') {
            return Err(Error::Invalid(format!("bad snapshot name {name:?}")));
        }
        let path = self.paths.snapshots().join(name);
        std::fs::read_to_string(&path).map_err(|e| Error::io(path, e))
    }

    fn prune(&self) -> Result<()> {
        for old in self.snapshots()?.into_iter().skip(KEEP_SNAPSHOTS) {
            let _ = std::fs::remove_file(self.paths.snapshots().join(old.name));
        }
        Ok(())
    }
}

/// Keep the lines that say what is wrong; mihomo prints a lot of noise.
fn summarize(output: &str) -> String {
    let useful: Vec<&str> = output
        .lines()
        .filter(|l| l.contains("level=error") || l.contains("level=fatal") || l.contains("test failed") || l.contains("parse config error"))
        .map(|l| l.split_once("msg=").map_or(l, |(_, m)| m.trim_matches('"')))
        .collect();
    if useful.is_empty() { output.trim().lines().rev().take(5).collect::<Vec<_>>().join("\n") } else { useful.join("\n") }
}

/// Look for a mihomo binary: explicit path, our install, then `$PATH`.
pub fn find_binary(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        return p.is_file().then(|| p.to_owned());
    }
    let ours = Some(crate::core_bin::default_path());
    let path_dirs = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default();
    ours.into_iter()
        .chain(path_dirs.into_iter().map(|d| d.join("mihomo")))
        .find(|p| p.is_file())
}

/// The mihomo binary integration tests run against (`OMM_TEST_MIHOMO`).
/// Without one they skip, except on CI (`CI` set), where a missing core
/// means the setup broke and must not pass silently.
#[doc(hidden)]
pub fn test_binary() -> Option<PathBuf> {
    match std::env::var_os("OMM_TEST_MIHOMO") {
        Some(p) => Some(PathBuf::from(p)),
        None if std::env::var_os("CI").is_some() => panic!("CI runs the real-core tests: set OMM_TEST_MIHOMO to a mihomo binary"),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_picks_errors() {
        let out = "time=\"x\" level=info msg=\"Start initial configuration in progress\"\ntime=\"x\" level=error msg=\"proxy group[0]: 'AI' not found\"\nconfiguration file /x test failed\n";
        assert_eq!(summarize(out), "proxy group[0]: 'AI' not found\nconfiguration file /x test failed");
    }

    #[test]
    fn install_keeps_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        paths.ensure().unwrap();
        let rt = Runtime::new(paths.clone(), "/nonexistent");
        rt.install("a: 1\n").unwrap();
        assert!(rt.snapshots().unwrap().is_empty());
        rt.install("a: 1\n").unwrap();
        assert!(rt.snapshots().unwrap().is_empty(), "identical config is not snapshotted");
        rt.install("a: 2\n").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        rt.install("a: 3\n").unwrap();
        let snaps = rt.snapshots().unwrap();
        assert_eq!(snaps.len(), 2, "quick successive installs keep separate snapshots");
        assert_eq!(rt.snapshot_text(&snaps[0].name).unwrap(), "a: 2\n", "newest first");
        assert!(rt.snapshot_text("../settings.yaml").is_err());
        assert_eq!(std::fs::read_to_string(paths.runtime_config()).unwrap(), "a: 3\n");
    }

    /// Runs only where a mihomo binary is available (CI may lack one).
    #[tokio::test]
    async fn validate_with_real_core() {
        let Some(bin) = test_binary() else { return };
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        paths.ensure().unwrap();
        let rt = Runtime::new(paths, bin);
        rt.validate("mixed-port: 17897\nproxies: []\nproxy-groups: [{name: G, type: select, proxies: [DIRECT]}]\nrules: ['MATCH,G']\n").await.unwrap();
        let err = rt.validate("proxy-groups: [{name: G, type: select, proxies: [Nope]}]\nrules: ['MATCH,G']\n").await.unwrap_err();
        assert!(err.to_string().contains("Nope"), "{err}");
    }
}
