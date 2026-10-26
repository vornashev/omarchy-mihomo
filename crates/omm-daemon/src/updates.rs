//! Keeping the core and its databases current. Once a day the daemon asks
//! GitHub for the newest stable mihomo and offers it (a setup issue and a
//! notice per version); it never replaces the binary on its own. The core
//! refreshes GeoIP, GeoSite and the MMDBs itself (`geo-auto-update`); the
//! daemon only re-reads them for traffic statistics when they change.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use omm_config::core_bin;
use omm_proto::{ErrorCode, RpcError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::problems::now_ms;
use crate::state::State;

const CHECK_EVERY_MS: i64 = 24 * 3_600_000;
/// A failed check is retried sooner than a day.
const RETRY_MS: i64 = 3 * 3_600_000;
const LOOK_EVERY: Duration = Duration::from_secs(10 * 60);
/// The core's databases, as mihomo names them.
const GEO_FILES: &[&str] = &["geoip.dat", "geosite.dat", "geoip.metadb", "country.mmdb", "asn.mmdb", "geoasn.mmdb"];

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Known {
    /// Newest stable release tag.
    pub latest: Option<String>,
    pub checked_at: i64,
    pub error: Option<String>,
    /// The release a notice went out for.
    pub notified: Option<String>,
}

pub struct Updates {
    path: PathBuf,
    pub known: Known,
    checking: bool,
    geo_updating: bool,
}

impl Updates {
    pub fn load(root: &Path) -> Self {
        let path = root.join("updates.json");
        let known = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        Self { path, known, checking: false, geo_updating: false }
    }

    fn save(&self) {
        let text = serde_json::to_vec_pretty(&self.known).expect("updates serialize");
        if let Err(e) = omm_config::paths::write_atomic(&self.path, &text) {
            tracing::warn!("saving update state: {e}");
        }
    }

    fn due(&self, now: i64) -> bool {
        let every = if self.known.error.is_some() { RETRY_MS } else { CHECK_EVERY_MS };
        self.known.checked_at == 0 || now - self.known.checked_at >= every
    }
}

/// The running core's version, when it is ours and a release.
fn current(state: &State) -> Option<String> {
    state.status().core.version.filter(|v| core_bin::release_version(v).is_some())
}

/// The newer release the running core could be updated to.
pub fn available(state: &State) -> Option<String> {
    if omm_platform::IS_MACOS || !state.config.owns_core(state) {
        return None;
    }
    let current = current(state)?;
    let latest = state.updates.lock().unwrap().known.latest.clone()?;
    core_bin::is_newer(&latest, &current).then_some(latest)
}

pub async fn watch(state: Arc<State>) {
    // Let the core come up and the network settle after boot.
    tokio::time::sleep(Duration::from_secs(60)).await;
    let mut geo_stamp = geo_files(&state.config.paths().runtime());
    loop {
        let wanted = !omm_platform::IS_MACOS && state.config.owns_core(&state) && !state.network_quiet();
        if wanted && state.updates.lock().unwrap().due(now_ms()) {
            check(&state).await;
        }
        // The core replaced its databases: traffic statistics read them anew.
        let stamp = geo_files(&state.config.paths().runtime());
        if stamp != geo_stamp {
            geo_stamp = stamp;
            state.traffic.lock().unwrap().reload_geo();
        }
        tokio::time::sleep(LOOK_EVERY).await;
    }
}

/// Ask GitHub for the newest release, direct and then through the core.
async fn check(state: &Arc<State>) {
    {
        let mut u = state.updates.lock().unwrap();
        if u.checking {
            return;
        }
        u.checking = true;
    }
    let mut result = core_bin::resolve(Some("latest"), None).await;
    if result.is_err()
        && let Some(proxy) = crate::config::proxy_of(state)
    {
        result = core_bin::resolve(Some("latest"), Some(&proxy)).await;
    }
    {
        let mut u = state.updates.lock().unwrap();
        u.checking = false;
        u.known.checked_at = now_ms();
        match result {
            Ok(rel) => {
                u.known.latest = Some(rel.version);
                u.known.error = None;
            }
            Err(e) => {
                tracing::info!("checking for a newer core: {e}");
                u.known.error = Some(e.to_string());
            }
        }
        u.save();
    }
    let Some(latest) = available(state) else {
        state.request_refresh();
        return;
    };
    tracing::info!(%latest, "a newer core is available");
    let fresh = state.updates.lock().unwrap().known.notified.as_ref() != Some(&latest);
    if fresh {
        state.notify("core_update", "core_update", "low", json!({ "current": current(state), "latest": latest }));
        let mut u = state.updates.lock().unwrap();
        u.known.notified = Some(latest);
        u.save();
    }
    state.request_refresh();
}

/// Install the newest release and restart the core on it. Only without
/// TUN capabilities to keep; with them, `omm fix update_core` runs in a
/// terminal where sudo can put them back.
pub async fn update_core(state: &Arc<State>) -> Result<(), RpcError> {
    let latest = state.updates.lock().unwrap().known.latest.clone().ok_or_else(|| RpcError::new(ErrorCode::NotFound, "no newer core known; check for updates first"))?;
    let version = crate::health::download_core(state, Some(&latest), &latest).await.map_err(|e| RpcError::new(ErrorCode::Fetch, e))?;
    tracing::info!(%version, "core updated");
    restart(state).await
}

/// Put back the core the last update replaced.
pub async fn rollback(state: &Arc<State>) -> Result<(), RpcError> {
    let dest = core_bin::default_path();
    tokio::task::spawn_blocking(move || core_bin::rollback(&dest))
        .await
        .map_err(|e| RpcError::new(ErrorCode::Internal, e.to_string()))?
        .map_err(|e| RpcError::new(ErrorCode::NotFound, e.to_string()))?;
    tracing::info!("core rolled back");
    restart(state).await
}

async fn restart(state: &Arc<State>) -> Result<(), RpcError> {
    let result = if state.core_running() { crate::session::command(state, "restart").await.map(drop) } else { Ok(()) };
    state.request_refresh();
    result
}

/// Ask the core to fetch its databases now.
async fn update_geo(state: &Arc<State>) -> Result<(), RpcError> {
    if !state.config.owns_core(state) {
        return Err(RpcError::new(ErrorCode::Unsupported, "the core is not ours to update"));
    }
    {
        let mut u = state.updates.lock().unwrap();
        if u.geo_updating {
            return Err(RpcError::new(ErrorCode::Conflict, "the databases are being updated already"));
        }
        u.geo_updating = true;
    }
    let result = state.core.update_geo().await;
    state.updates.lock().unwrap().geo_updating = false;
    result.map_err(|e| RpcError::new(ErrorCode::Core, e.to_string()))?;
    state.traffic.lock().unwrap().reload_geo();
    Ok(())
}

/// `(name, bytes, modified ms)` of the databases in the core's home.
fn geo_files(home: &Path) -> Vec<(String, u64, i64)> {
    let mut out: Vec<_> = std::fs::read_dir(home)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if !GEO_FILES.contains(&name.to_ascii_lowercase().as_str()) {
                return None;
            }
            let meta = e.metadata().ok()?;
            Some((name, meta.len(), modified_ms(&meta)))
        })
        .collect();
    out.sort();
    out
}

fn modified_ms(meta: &std::fs::Metadata) -> i64 {
    meta.modified().ok().and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as i64)
}

async fn info(state: &Arc<State>) -> Value {
    let current = state.status().core.version;
    let available = available(state);
    let (known, checking, geo_updating) = {
        let u = state.updates.lock().unwrap();
        (u.known.clone(), u.checking, u.geo_updating)
    };
    let prev = core_bin::previous_path(&core_bin::default_path());
    let previous = match prev.is_file() {
        true => core_bin::binary_version(&prev).await.ok().map(|line| line.split_whitespace().find(|w| w.starts_with('v')).unwrap_or(&line).to_owned()),
        false => None,
    };
    let settings = state.config.settings().await;
    let geo: Vec<Value> = geo_files(&state.config.paths().runtime()).into_iter().map(|(name, size, modified)| json!({ "name": name, "size": size, "modified": modified })).collect();
    let fix = state.status().issues.into_iter().find(|i| i.id == "core_update").and_then(|i| i.fix);
    json!({
        "supported": !omm_platform::IS_MACOS && state.config.owns_core(state),
        "current": current,
        "latest": known.latest,
        "available": available,
        "checked_at": (known.checked_at > 0).then_some(known.checked_at),
        "error": known.error,
        "checking": checking,
        "fix": fix,
        "previous": previous,
        "geo": geo,
        "geo_auto_update": settings.geo_auto_update,
        "geo_update_interval": settings.geo_update_interval,
        "geo_updating": geo_updating,
    })
}

pub async fn rpc(state: &Arc<State>, method: &str) -> Result<Value, RpcError> {
    match method {
        "core.updates" => {}
        "core.check_updates" => {
            check(state).await;
            // The issue list, and with it the fix, follows the new answer.
            state.set_issues(crate::health::check(state).await);
        }
        "core.update_geo" => update_geo(state).await?,
        "core.rollback" => rollback(state).await?,
        _ => return Err(RpcError::new(ErrorCode::UnknownMethod, method.to_owned())),
    }
    Ok(info(state).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_daily_and_retries_failures_sooner() {
        let mut u = Updates { path: PathBuf::new(), known: Known::default(), checking: false, geo_updating: false };
        assert!(u.due(1), "never checked");
        u.known.checked_at = 1_000;
        assert!(!u.due(1_000 + RETRY_MS));
        assert!(u.due(1_000 + CHECK_EVERY_MS));
        u.known.error = Some("timeout".into());
        assert!(u.due(1_000 + RETRY_MS));
    }

    #[test]
    fn geo_files_by_mihomo_names() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["GeoSite.dat", "ASN.mmdb", "config.yaml", "cache.db"] {
            std::fs::write(dir.path().join(name), "x").unwrap();
        }
        let names: Vec<String> = geo_files(dir.path()).into_iter().map(|(n, _, _)| n).collect();
        assert_eq!(names, vec!["ASN.mmdb", "GeoSite.dat"]);
    }
}
