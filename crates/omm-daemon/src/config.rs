//! Profiles, settings and the generated runtime config.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use omm_config::build::{self, Inputs};
use omm_config::fetch::{self, Fetched};
use omm_config::paths::Paths;
use omm_config::profiles::{self, GLOBAL, Overrides, Profile, Source, Store};
use omm_config::runtime::{Runtime, Snapshot};
use omm_config::settings::Settings;
use omm_proto::{ConfigStatus, ErrorCode, ProfileRef, RpcError};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::Mutex;

use crate::state::State;

const AUTO_UPDATE_EVERY: Duration = Duration::from_secs(10 * 60);

pub struct Manager {
    paths: Paths,
    binary: Option<PathBuf>,
    inner: Mutex<Inner>,
}

struct Inner {
    store: Store,
    settings: Settings,
    rules: crate::user_rules::Store,
    status: ConfigStatus,
    /// Names rules may target, from the last successful build.
    targets: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ApplyReport {
    pub profile: Option<ProfileRef>,
    /// Valid rule targets: `DIRECT` and the profile's groups.
    pub groups: Vec<String>,
    pub orphans: Vec<i64>,
    /// False when the core is not ours or not running.
    pub reloaded: bool,
}

#[derive(Debug, Serialize)]
pub struct UpdateResult {
    pub id: String,
    pub name: String,
    /// `updated`, `unchanged` or `failed`.
    pub outcome: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Manager {
    pub fn open(paths: Paths, binary: Option<PathBuf>) -> anyhow::Result<Self> {
        paths.ensure()?;
        let store = Store::open(paths.clone())?;
        let settings = Settings::load(&paths.settings())?;
        let rules = crate::user_rules::Store::open(&paths.root.join("state.db"))?;
        Ok(Self {
            paths,
            binary,
            inner: Mutex::new(Inner { store, settings, rules, status: ConfigStatus::default(), targets: Vec::new() }),
        })
    }

    // ------------------------------------------------------------ apply

    /// Rebuild, validate, install and hot-reload the runtime config.
    pub async fn apply(&self, state: &State) -> Result<ApplyReport, RpcError> {
        let mut inner = self.inner.lock().await;
        let result = self.apply_locked(&mut inner, state).await;
        if let Err(e) = &result {
            inner.status.error = Some(e.message.clone());
        }
        state.set_config_status(inner.status.clone());
        result
    }

    async fn apply_locked(&self, inner: &mut Inner, state: &State) -> Result<ApplyReport, RpcError> {
        let profile = inner.store.current().cloned().ok_or_else(|| RpcError::new(ErrorCode::NotFound, "no profile yet: add one first"))?;
        let text = inner.store.text(&profile.id).map_err(cfg_err)?;
        let global = Overrides::load(&self.paths, GLOBAL).map_err(cfg_err)?;
        let overrides = Overrides::load(&self.paths, &profile.id).map_err(cfg_err)?;
        let user_rules = inner.rules.rules().map_err(db_err)?;
        let out = build::build(&Inputs {
            profile: &text,
            profile_name: &profile.name,
            global: &global,
            overrides: &overrides,
            user_rules: &user_rules,
            settings: &inner.settings,
        })
        .map_err(cfg_err)?;
        let yaml = out.yaml().map_err(cfg_err)?;

        let runtime = self.runtime()?;
        runtime.validate(&yaml).await.map_err(cfg_err)?;
        let live = runtime.install(&yaml).map_err(cfg_err)?;
        let reloaded = self.reload(state, &runtime, &live).await?;

        let profile = Some(ProfileRef { id: profile.id, name: profile.name });
        inner.status = ConfigStatus { profile: profile.clone(), applied_at: Some(profiles::now()), orphans: out.orphans.clone(), error: None };
        inner.targets = out.targets;
        tracing::info!(reloaded, orphans = out.orphans.len(), "config applied");
        Ok(ApplyReport { profile, groups: out.groups, orphans: out.orphans, reloaded })
    }

    /// Hot-reload our core; on failure put the previous config back.
    async fn reload(&self, state: &State, runtime: &Runtime, live: &std::path::Path) -> Result<bool, RpcError> {
        if !self.owns_core(state) || !state.core_running() {
            return Ok(false);
        }
        let Err(e) = state.core.reload_config(live, false).await else {
            state.request_refresh();
            return Ok(true);
        };
        tracing::warn!("reload failed, restoring previous config: {e}");
        if let Some(prev) = runtime.snapshots().ok().and_then(|s| s.into_iter().next())
            && let Ok(text) = runtime.snapshot_text(&prev.name)
            && runtime.install(&text).is_ok()
        {
            let _ = state.core.reload_config(live, false).await;
        }
        Err(RpcError::new(ErrorCode::Core, format!("core rejected the new config: {e}")))
    }

    /// Only the core living in our runtime directory is ever reloaded.
    /// Pointing the daemon at someone else's core (Clash Verge during
    /// migration) must never replace that core's config.
    fn owns_core(&self, state: &State) -> bool {
        state.core.socket() == self.paths.controller_socket()
    }

    fn runtime(&self) -> Result<Runtime, RpcError> {
        let binary = self.binary.clone().ok_or_else(|| {
            RpcError::new(ErrorCode::Config, "mihomo binary not found: install it to ~/.local/lib/omarchy-mihomo/mihomo or set OMM_CORE_BIN")
        })?;
        Ok(Runtime::new(self.paths.clone(), binary))
    }

    // ------------------------------------------------------------ profiles

    pub async fn list(&self) -> Value {
        let inner = self.inner.lock().await;
        serde_json::json!({ "current": inner.store.index.current, "items": inner.store.index.items })
    }

    pub async fn add_remote(&self, state: &State, url: String, name: Option<String>, user_agent: Option<String>) -> Result<Value, RpcError> {
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(RpcError::new(ErrorCode::BadRequest, "subscription URL must start with http:// or https://"));
        }
        let req = fetch::Request { url: &url, user_agent: user_agent.as_deref(), etag: None, fallback_proxy: proxy_of(state) };
        let Fetched::New(dl) = fetch::fetch(&req).await.map_err(cfg_err)? else {
            return Err(RpcError::new(ErrorCode::Fetch, "server answered 304 to a first download"));
        };
        let name = name.filter(|n| !n.trim().is_empty()).or(dl.filename.clone()).unwrap_or_else(|| host_of(&url));

        let mut inner = self.inner.lock().await;
        let id = inner.store.new_id();
        inner.store.write_text(&id, &dl.body).map_err(cfg_err)?;
        let became_current = inner.store.index.current.is_none();
        inner.store.insert(Profile {
            id: id.clone(),
            name,
            source: Source::Remote { url, user_agent },
            updated: Some(profiles::now()),
            interval_hours: dl.interval_hours,
            usage: dl.usage,
            etag: dl.etag,
            last_error: None,
        });
        inner.store.save().map_err(cfg_err)?;
        drop(inner);
        self.after_profile_change(state, became_current).await?;
        Ok(serde_json::json!({ "id": id }))
    }

    pub async fn add_local(&self, state: &State, name: String, text: String) -> Result<Value, RpcError> {
        let mut inner = self.inner.lock().await;
        let id = inner.store.new_id();
        inner.store.write_text(&id, &text).map_err(cfg_err)?;
        let became_current = inner.store.index.current.is_none();
        inner.store.insert(Profile {
            id: id.clone(),
            name,
            source: Source::Local,
            updated: Some(profiles::now()),
            interval_hours: None,
            usage: None,
            etag: None,
            last_error: None,
        });
        inner.store.save().map_err(cfg_err)?;
        drop(inner);
        self.after_profile_change(state, became_current).await?;
        Ok(serde_json::json!({ "id": id }))
    }

    /// Refresh one profile, or every due remote profile when `id` is None.
    pub async fn update(&self, state: &State, id: Option<String>, force: bool) -> Vec<UpdateResult> {
        let targets: Vec<Profile> = {
            let inner = self.inner.lock().await;
            let now = profiles::now();
            let hours = inner.settings.update_interval_hours;
            inner
                .store
                .index
                .items
                .iter()
                .filter(|p| p.is_remote())
                .filter(|p| match &id {
                    Some(id) => &p.id == id,
                    None => force || p.is_due(hours, now),
                })
                .cloned()
                .collect()
        };

        let mut results = Vec::new();
        let mut current_changed = false;
        for p in targets {
            let Source::Remote { url, user_agent } = &p.source else { continue };
            // A forced update re-downloads even if the server says unchanged.
            let etag = if force || id.is_some() { None } else { p.etag.as_deref() };
            let req = fetch::Request { url, user_agent: user_agent.as_deref(), etag, fallback_proxy: proxy_of(state) };
            let fetched = fetch::fetch(&req).await;

            let mut inner = self.inner.lock().await;
            let is_current = inner.store.index.current.as_deref() == Some(p.id.as_str());
            let outcome = match fetched {
                Ok(Fetched::NotModified) => Ok("unchanged"),
                Ok(Fetched::New(dl)) => inner.store.write_text(&p.id, &dl.body).map(|_| {
                    if let Ok(e) = inner.store.get_mut(&p.id) {
                        e.etag = dl.etag;
                        e.usage = dl.usage.or(e.usage);
                        e.interval_hours = dl.interval_hours.or(e.interval_hours);
                    }
                    current_changed |= is_current;
                    "updated"
                }),
                Err(e) => Err(e),
            };
            let entry = inner.store.get_mut(&p.id);
            let result = match (outcome, entry) {
                (Ok(outcome), Ok(e)) => {
                    e.updated = Some(profiles::now());
                    e.last_error = None;
                    UpdateResult { id: p.id.clone(), name: p.name.clone(), outcome, error: None }
                }
                (Err(err), Ok(e)) => {
                    e.last_error = Some(err.to_string());
                    UpdateResult { id: p.id.clone(), name: p.name.clone(), outcome: "failed", error: Some(err.to_string()) }
                }
                // Removed while we were downloading.
                (_, Err(_)) => continue,
            };
            if let Err(e) = inner.store.save() {
                tracing::warn!("saving profiles: {e}");
            }
            tracing::info!(profile = %result.name, outcome = result.outcome, "profile update");
            results.push(result);
        }

        if current_changed && let Err(e) = self.apply(state).await {
            tracing::warn!("apply after update: {}", e.message);
        }
        results
    }

    pub async fn select(&self, state: &State, id: &str) -> Result<ApplyReport, RpcError> {
        {
            let mut inner = self.inner.lock().await;
            inner.store.select(id).map_err(cfg_err)?;
            inner.store.save().map_err(cfg_err)?;
        }
        self.apply(state).await
    }

    pub async fn remove(&self, state: &State, id: &str) -> Result<Value, RpcError> {
        let current_changed = {
            let mut inner = self.inner.lock().await;
            let was_current = inner.store.index.current.as_deref() == Some(id);
            inner.store.remove(id).map_err(cfg_err)?;
            inner.store.save().map_err(cfg_err)?;
            was_current && inner.store.index.current.is_some()
        };
        if current_changed {
            self.apply(state).await?;
        }
        Ok(serde_json::json!({ "ok": true }))
    }

    async fn after_profile_change(&self, state: &State, became_current: bool) -> Result<(), RpcError> {
        if became_current {
            self.apply(state).await?;
        }
        Ok(())
    }

    // ------------------------------------------------------------ user rules

    pub async fn rules(&self) -> Result<(Vec<crate::user_rules::Stored>, Vec<i64>), RpcError> {
        let inner = self.inner.lock().await;
        Ok((inner.rules.all().map_err(db_err)?, inner.status.orphans.clone()))
    }

    pub async fn rule_history(&self, limit: usize) -> Result<Vec<crate::user_rules::Change>, RpcError> {
        self.inner.lock().await.rules.history(limit).map_err(db_err)
    }

    /// Change rules and apply. If the config cannot be applied, or a rule
    /// this change touched ends up without its target, the change is undone
    /// and nothing reaches the core.
    ///
    /// `strict` (save, edit) refuses targets the profile lacks; rollback and
    /// delete may leave orphans, which are skipped and flagged instead.
    pub async fn change_rules<T>(
        &self,
        state: &State,
        strict: bool,
        f: impl FnOnce(&mut crate::user_rules::Store) -> Result<(T, Vec<crate::user_rules::Change>), RpcError>,
    ) -> Result<(T, ApplyReport), RpcError> {
        let mut inner = self.inner.lock().await;
        let known = inner.targets.clone();
        let (value, changes) = f(&mut inner.rules)?;
        // Refuse unknown targets up front when we know the current profile.
        let unknown: Vec<&str> = changes
            .iter()
            .filter_map(|c| c.after.as_ref())
            .filter(|a| strict && a.rule.enabled)
            .map(|a| a.rule.target.as_str())
            .filter(|t| !known.is_empty() && !known.iter().any(|k| k == t))
            .collect();
        if !unknown.is_empty() {
            let msg = format!("no group or proxy named {} in the current profile", unknown.join(", "));
            if let Some(first) = changes.iter().map(|c| c.id).min() {
                inner.rules.discard_since(first).map_err(db_err)?;
            }
            return Err(RpcError::new(ErrorCode::NotFound, msg));
        }
        let first = changes.iter().map(|c| c.id).min();
        let touched: Vec<i64> = if strict { changes.iter().filter(|c| c.after.as_ref().is_some_and(|a| a.rule.enabled)).map(|c| c.rule_id).collect() } else { Vec::new() };

        let result = match self.apply_locked(&mut inner, state).await {
            Ok(r) if r.orphans.iter().any(|o| touched.contains(o)) => {
                let bad: Vec<String> = changes.iter().filter_map(|c| c.after.as_ref()).filter(|a| r.orphans.contains(&a.rule.id)).map(|a| a.rule.target.clone()).collect();
                Err(RpcError::new(ErrorCode::NotFound, format!("no group or proxy named {} in the current profile", bad.join(", "))))
            }
            other => other,
        };
        match result {
            Ok(report) => {
                state.set_config_status(inner.status.clone());
                Ok((value, report))
            }
            Err(e) => {
                if let Some(first) = first {
                    inner.rules.discard_since(first).map_err(db_err)?;
                    // Put the previous state back into the live config as well.
                    if let Err(again) = self.apply_locked(&mut inner, state).await {
                        tracing::warn!("re-apply after undo: {}", again.message);
                    }
                }
                inner.status.error = Some(e.message.clone());
                state.set_config_status(inner.status.clone());
                Err(e)
            }
        }
    }

    // ------------------------------------------------------------ settings

    pub async fn settings(&self) -> Settings {
        self.inner.lock().await.settings.clone()
    }

    /// Merge a JSON patch into the settings, save and apply.
    pub async fn set_settings(&self, state: &State, patch: Value) -> Result<ApplyReport, RpcError> {
        {
            let mut inner = self.inner.lock().await;
            let mut current = serde_json::to_value(&inner.settings).map_err(internal)?;
            json_merge(&mut current, patch);
            let next: Settings = serde_json::from_value(current).map_err(|e| RpcError::new(ErrorCode::BadRequest, e.to_string()))?;
            next.save(&self.paths.settings()).map_err(cfg_err)?;
            inner.settings = next;
        }
        self.apply(state).await
    }

    /// Remember a runtime change so the next rebuild keeps it.
    pub async fn persist(&self, f: impl FnOnce(&mut Settings)) -> Result<(), RpcError> {
        let mut inner = self.inner.lock().await;
        f(&mut inner.settings);
        inner.settings.save(&self.paths.settings()).map_err(cfg_err)
    }

    // ------------------------------------------------------------ rollback

    pub fn snapshots(&self) -> Result<Vec<Snapshot>, RpcError> {
        self.runtime()?.snapshots().map_err(cfg_err)
    }

    /// Put a previous runtime config back (the newest when `name` is None).
    pub async fn rollback(&self, state: &State, name: Option<String>) -> Result<Value, RpcError> {
        let _guard = self.inner.lock().await;
        let runtime = self.runtime()?;
        let name = match name {
            Some(n) => n,
            None => runtime.snapshots().map_err(cfg_err)?.into_iter().next().map(|s| s.name).ok_or_else(|| RpcError::new(ErrorCode::NotFound, "no snapshots"))?,
        };
        let text = runtime.snapshot_text(&name).map_err(cfg_err)?;
        runtime.validate(&text).await.map_err(cfg_err)?;
        let live = runtime.install(&text).map_err(cfg_err)?;
        let reloaded = self.reload(state, &runtime, &live).await?;
        Ok(serde_json::json!({ "restored": name, "reloaded": reloaded }))
    }

    // ------------------------------------------------------------ startup

    /// Publish what we know on startup and generate a config if none exists.
    pub async fn init(&self, state: &State) {
        let has_profile = {
            let mut inner = self.inner.lock().await;
            inner.status.profile = inner.store.current().map(|p| ProfileRef { id: p.id.clone(), name: p.name.clone() });
            state.set_config_status(inner.status.clone());
            inner.store.current().is_some()
        };
        if has_profile && !self.paths.runtime_config().exists() && let Err(e) = self.apply(state).await {
            tracing::warn!("initial apply: {}", e.message);
        }
    }
}

/// Periodically refresh due subscriptions.
pub async fn auto_update(state: Arc<State>) {
    tokio::time::sleep(Duration::from_secs(30)).await;
    loop {
        for r in state.config.update(&state, None, false).await {
            if let Some(e) = r.error {
                tracing::warn!(profile = %r.name, "auto-update failed: {e}");
            }
        }
        tokio::time::sleep(AUTO_UPDATE_EVERY).await;
    }
}

/// Download through our own core when it runs: the subscription host may be
/// unreachable directly.
fn proxy_of(state: &State) -> Option<String> {
    let s = state.status();
    (s.core.running && s.core.mixed_port > 0).then(|| format!("http://127.0.0.1:{}", s.core.mixed_port))
}

fn host_of(url: &str) -> String {
    url.split_once("://").map_or(url, |(_, r)| r).split(['/', '?', '#', ':']).next().unwrap_or("profile").to_owned()
}

fn json_merge(dst: &mut Value, src: Value) {
    match (dst, src) {
        (Value::Object(d), Value::Object(s)) => {
            for (k, v) in s {
                json_merge(d.entry(k).or_insert(Value::Null), v);
            }
        }
        (d, s) => *d = s,
    }
}

pub fn cfg_err(e: omm_config::Error) -> RpcError {
    use omm_config::Error as E;
    let code = match &e {
        E::Rejected(_) | E::Yaml { .. } => ErrorCode::Config,
        E::Fetch(_) => ErrorCode::Fetch,
        E::NoProfile(_) => ErrorCode::NotFound,
        E::Invalid(_) => ErrorCode::BadRequest,
        E::Io { .. } => ErrorCode::Internal,
    };
    RpcError::new(code, e.to_string())
}

pub fn db_err(e: impl std::fmt::Display) -> RpcError {
    RpcError::new(ErrorCode::Internal, format!("database: {e}"))
}

fn internal(e: impl std::fmt::Display) -> RpcError {
    RpcError::new(ErrorCode::Internal, e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_names() {
        assert_eq!(host_of("https://sub.example.com:8443/api?token=x"), "sub.example.com");
        assert_eq!(host_of("http://a.b/c"), "a.b");
    }

    #[test]
    fn settings_patch_merges_nested() {
        let mut v = serde_json::json!({"tun": {"enable": true, "stack": "gvisor"}, "mixed-port": 7897});
        json_merge(&mut v, serde_json::json!({"tun": {"enable": false}}));
        assert_eq!(v, serde_json::json!({"tun": {"enable": false, "stack": "gvisor"}, "mixed-port": 7897}));
    }
}
