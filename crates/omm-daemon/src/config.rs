//! Profiles, settings and the generated runtime config.
#[path = "proxy_config.rs"]
mod proxy_config;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use omm_config::build::{self, Inputs};
use omm_config::fetch::{self, Fetched};
use omm_config::paths::Paths;
use omm_config::profiles::{self, GLOBAL, Overrides, Profile, Source, Store};
use omm_config::runtime::{Runtime, Snapshot};
use omm_config::settings::Settings;
use omm_config::yaml::parse_mapping;
use omm_proto::{ConfigStatus, ErrorCode, ProfileRef, RpcError, topic};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::Mutex;

use crate::state::State;

const AUTO_UPDATE_EVERY: Duration = Duration::from_secs(10 * 60);
/// How often local profile files are checked for saves from an editor.
const FILE_POLL_EVERY: Duration = Duration::from_secs(1);
/// Drafts never filled in are dropped on startup after this long.
const DRAFT_TTL: Duration = Duration::from_secs(24 * 3600);

pub struct Manager {
    paths: Paths,
    /// Validates configs; set later when the core gets installed.
    binary: std::sync::RwLock<Option<PathBuf>>,
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
            binary: std::sync::RwLock::new(binary),
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
        let _operation = state.begin_operation("applying", &profile.name);
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

    /// TUN is on in the settings of a core we run.
    pub async fn tun_wanted(&self, state: &State) -> bool {
        self.owns_core(state) && self.inner.lock().await.settings.tun.enable
    }

    pub fn set_binary(&self, path: PathBuf) {
        *self.binary.write().unwrap() = Some(path);
    }

    pub fn paths(&self) -> &Paths {
        &self.paths
    }

    pub async fn has_profile(&self) -> bool {
        self.inner.lock().await.store.current().is_some()
    }

    /// Only the core living in our runtime directory is ever reloaded.
    /// Pointing the daemon at someone else's core (Clash Verge during
    /// migration) must never replace that core's config.
    pub fn owns_core(&self, state: &State) -> bool {
        state.core.socket() == self.paths.controller_socket() || (omm_platform::IS_MACOS && state.core.socket() == omm_platform::core_socket())
    }

    fn runtime(&self) -> Result<Runtime, RpcError> {
        let binary = self.binary.read().unwrap().clone().ok_or_else(|| {
            RpcError::new(ErrorCode::Config, "mihomo binary not found: install it to ~/.local/lib/omarchy-mihomo/mihomo or set OMM_CORE_BIN")
        })?;
        Ok(Runtime::new(self.paths.clone(), binary))
    }

    pub async fn read_profile(&self, id: &str) -> Result<Value, RpcError> {
        let inner = self.inner.lock().await;
        inner.store.get(id).map_err(cfg_err)?;
        let content = inner.store.text(id).map_err(cfg_err)?;
        Ok(serde_json::json!({"content":content}))
    }
    pub async fn save_profile(&self, state: &State, id: &str, expected: &str, content: &str) -> Result<Value, RpcError> {
        let inner = self.inner.lock().await;
        inner.store.get(id).map_err(cfg_err)?;
        if inner.store.text(id).map_err(cfg_err)? != expected {
            return Err(RpcError::new(ErrorCode::Conflict, "Profile changed on disk; reopen it before saving"));
        }
        parse_mapping(content, "profile editor").map_err(cfg_err)?;
        self.runtime()?.validate(content).await.map_err(cfg_err)?;
        omm_config::paths::write_atomic(&self.paths.profile(id), content.as_bytes()).map_err(cfg_err)?;
        drop(inner);
        self.file_saved(state, id).await;
        Ok(serde_json::json!({"ok":true}))
    }

    // ------------------------------------------------------------ profiles

    pub async fn list(&self) -> Value {
        let inner = self.inner.lock().await;
        // `root` lets clients open profile and override files in an editor.
        serde_json::json!({ "current": inner.store.index.current, "items": inner.store.index.items, "root": self.paths.root })
    }

    /// An empty local profile to be filled in an editor; [`watch_files`]
    /// picks up every save.
    pub async fn create(&self, name: String) -> Result<Value, RpcError> {
        let mut inner = self.inner.lock().await;
        let p = inner.store.create_draft(&name).map_err(cfg_err)?;
        inner.store.save().map_err(cfg_err)?;
        Ok(serde_json::json!({ "id": p.id, "path": self.paths.profile(&p.id) }))
    }

    /// A local profile's file changed on disk: take its name, check it, and
    /// apply it when it is (or just became) the current profile.
    async fn file_saved(&self, state: &State, id: &str) {
        let apply = {
            let mut inner = self.inner.lock().await;
            let Ok(text) = inner.store.text(id) else { return };
            let had_current = inner.store.index.current.is_some();
            let Ok(p) = inner.store.get_mut(id) else { return };
            if let Some(name) = profiles::name_hint(&text) {
                p.name = name;
            }
            let usable = match parse_mapping(&text, &p.name) {
                Ok(m) if m.is_empty() => {
                    // Still the bare template: nothing to report for a draft.
                    p.last_error = (!p.draft).then(|| "the file holds no config".into());
                    false
                }
                Ok(_) => {
                    p.draft = false;
                    p.last_error = None;
                    p.updated = Some(profiles::now());
                    true
                }
                Err(e) => {
                    p.last_error = Some(e.to_string());
                    false
                }
            };
            if usable && !had_current {
                inner.store.index.current = Some(id.into());
            }
            if let Err(e) = inner.store.save() {
                tracing::warn!("saving profiles: {e}");
            }
            usable && inner.store.index.current.as_deref() == Some(id)
        };

        if apply {
            match self.apply(state).await {
                Ok(_) => crate::health::start_core_if_free(state).await,
                // Shown on the profile, where the user is looking.
                Err(e) => {
                    if let Ok(p) = self.inner.lock().await.store.get_mut(id) {
                        p.last_error = Some(e.message);
                    }
                }
            }
        }
        let inner = self.inner.lock().await;
        // A draft still holding its template has nothing to report.
        if let Ok(p) = inner.store.get(id)
            && !(p.draft && p.last_error.is_none())
        {
            tracing::info!(profile = %p.name, error = p.last_error.as_deref(), "local profile saved");
            state.publish(topic::PROFILES, &serde_json::json!({ "id": p.id, "name": p.name, "draft": p.draft, "error": p.last_error }));
        }
    }

    /// Size and mtime of every local profile file.
    async fn local_files(&self) -> Vec<(String, bool, Option<FileStamp>)> {
        let inner = self.inner.lock().await;
        inner
            .store
            .index
            .items
            .iter()
            .filter(|p| p.is_local())
            .map(|p| (p.id.clone(), p.draft, stamp(&self.paths.profile(&p.id))))
            .collect()
    }

    /// Forget drafts left empty for a day (an editor closed without saving).
    async fn drop_stale_drafts(&self) {
        let mut inner = self.inner.lock().await;
        let stale: Vec<String> = inner
            .store
            .index
            .items
            .iter()
            .filter(|p| p.draft)
            .filter(|p| stamp(&self.paths.profile(&p.id)).is_none_or(|s| s.mtime.elapsed().is_ok_and(|age| age > DRAFT_TTL)))
            .map(|p| p.id.clone())
            .collect();
        for id in &stale {
            let _ = inner.store.remove(id);
        }
        if !stale.is_empty() {
            tracing::info!(count = stale.len(), "dropped empty drafts");
            if let Err(e) = inner.store.save() {
                tracing::warn!("saving profiles: {e}");
            }
        }
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
            draft: false,
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
            draft: false,
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
            crate::health::start_core_if_free(state).await;
        }
        Ok(())
    }

    /// Bring over Clash Verge Rev's profiles with their enhancements, make
    /// its current profile ours and apply it. Profiles imported before
    /// (same subscription URL, or same name and text) are not duplicated.
    pub async fn import_cvr(&self, state: &State) -> Result<Value, RpcError> {
        let dir = omm_config::host::cvr_data_dir().ok_or_else(|| RpcError::new(ErrorCode::NotFound, "no Clash Verge Rev profiles found"))?;
        let found = omm_config::cvr::read(&dir).map_err(cfg_err)?;
        let mut warnings = found.warnings;
        let (mut imported, mut skipped) = (Vec::new(), Vec::new());
        let mut autotune = Vec::new();
        let had_current = {
            let mut inner = self.inner.lock().await;
            let had_current = inner.store.current().is_some();
            let mut current = None;
            for p in found.profiles {
                let store = &inner.store;
                let dup = store.index.items.iter().find(|e| match (&e.source, &p.source) {
                    (Source::Remote { url: a, .. }, Source::Remote { url: b, .. }) => a == b,
                    (Source::Local, Source::Local) => e.name == p.name && store.text(&e.id).is_ok_and(|t| t == p.body),
                    _ => false,
                });
                if let Some(e) = dup {
                    if p.current {
                        current = Some(e.id.clone());
                    }
                    skipped.push(p.name);
                    continue;
                }
                autotune.extend(p.autotune_rules.iter().cloned());
                let id = inner.store.new_id();
                inner.store.write_text(&id, &p.body).map_err(cfg_err)?;
                for (file, text) in &p.overrides {
                    omm_config::paths::write_atomic(&self.paths.overrides(&id).join(file), text.as_bytes()).map_err(cfg_err)?;
                }
                inner.store.insert(Profile {
                    id: id.clone(),
                    name: p.name.clone(),
                    source: p.source,
                    updated: Some(profiles::now()),
                    interval_hours: p.interval_hours,
                    usage: None,
                    etag: None,
                    last_error: None,
                    draft: false,
                });
                if p.current {
                    current = Some(id);
                }
                imported.push(p.name);
            }
            if let Some(text) = found.global_merge {
                let path = self.paths.overrides(GLOBAL).join("merge.yaml");
                let ours = omm_config::paths::read_optional(&path).map_err(cfg_err)?.unwrap_or_default();
                if parse_mapping(&ours, "global/merge.yaml").is_ok_and(|m| m.is_empty()) {
                    omm_config::paths::write_atomic(&path, text.as_bytes()).map_err(cfg_err)?;
                } else {
                    warnings.push("global merge.yaml already has content; Clash Verge's global Merge was not copied over it".into());
                }
            }
            if let Some(id) = current {
                inner.store.index.current = Some(id);
            }
            inner.store.save().map_err(cfg_err)?;
            // site-autotune's picks become ordinary user rules: visible,
            // editable, and undoable through the journal like any other.
            let now = crate::problems::now_ms();
            for r in &autotune {
                let new = crate::user_rules::NewRule {
                    kind: r.kind,
                    value: r.value.clone(),
                    target: r.target.clone(),
                    note: None,
                    source: "site-autotune".into(),
                    test: None,
                    lifetime: Default::default(),
                };
                if inner.rules.find(r.kind, &r.value).map_err(db_err)?.is_none() {
                    inner.rules.upsert(new, now).map_err(db_err)?;
                }
            }
            had_current
        };
        tracing::info!(imported = imported.len(), skipped = skipped.len(), rules = autotune.len(), "imported Clash Verge profiles");
        if self.has_profile().await {
            if let Err(e) = self.apply(state).await {
                warnings.push(format!("apply: {}", e.message));
            } else if !had_current {
                crate::health::start_core_if_free(state).await;
            }
        }
        state.request_refresh();
        Ok(serde_json::json!({ "imported": imported, "skipped": skipped, "rules": autotune.len(), "warnings": warnings }))
    }

    /// Move site-autotune's `AUTO-*` rules out of every profile's rules
    /// override into the user's rules, then apply. For profiles imported
    /// before the import learned to do this; a second run finds nothing.
    pub async fn migrate_autotune(&self, state: &State) -> Result<Value, RpcError> {
        let moved = {
            let mut inner = self.inner.lock().await;
            let ids: Vec<String> = inner.store.index.items.iter().map(|p| p.id.clone()).collect();
            let now = crate::problems::now_ms();
            let mut moved = 0;
            for id in ids {
                let path = self.paths.overrides(&id).join("rules.yaml");
                let Some(text) = omm_config::paths::read_optional(&path).map_err(cfg_err)? else { continue };
                let (rest, rules) = omm_config::cvr::split_autotune(&text).map_err(cfg_err)?;
                if rules.is_empty() {
                    continue;
                }
                for r in &rules {
                    if inner.rules.find(r.kind, &r.value).map_err(db_err)?.is_none() {
                        let new = crate::user_rules::NewRule { kind: r.kind, value: r.value.clone(), target: r.target.clone(), note: None, source: "site-autotune".into(), test: None, lifetime: Default::default() };
                        inner.rules.upsert(new, now).map_err(db_err)?;
                        moved += 1;
                    }
                }
                omm_config::paths::write_atomic(&path, rest.unwrap_or_default().as_bytes()).map_err(cfg_err)?;
            }
            moved
        };
        if moved > 0 {
            self.apply(state).await?;
        }
        Ok(serde_json::json!({ "moved": moved }))
    }

    // ------------------------------------------------------------ user rules

    pub async fn rules(&self) -> Result<(Vec<crate::user_rules::Stored>, Vec<i64>), RpcError> {
        let inner = self.inner.lock().await;
        Ok((inner.rules.all().map_err(db_err)?, inner.status.orphans.clone()))
    }

    pub async fn rule_redirects(&self) -> Result<omm_config::nodes::Managed, RpcError> {
        let inner = self.inner.lock().await;
        inner.store.current().map(|p| omm_config::nodes::Managed::load(&self.paths, &p.id).map_err(cfg_err)).transpose().map(Option::unwrap_or_default)
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
        let managed = inner.store.current().map(|p| omm_config::nodes::Managed::load(&self.paths, &p.id).map_err(cfg_err)).transpose()?.unwrap_or_default();
        let (value, changes) = f(&mut inner.rules)?;
        // Refuse unknown targets up front when we know the current profile.
        let unknown: Vec<&str> = changes
            .iter()
            .filter_map(|c| c.after.as_ref())
            .filter(|a| strict && a.rule.enabled)
            .map(|a| a.rule.target.as_str())
            .filter(|t| !known.is_empty() && !known.contains(&managed.target(t)))
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
        self.drop_stale_drafts().await;
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
                state.notify(&format!("subscription:{}", r.id), "subscription", "normal", serde_json::json!({ "id": r.id, "name": r.name, "error": e }));
            }
        }
        tokio::time::sleep(AUTO_UPDATE_EVERY).await;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct FileStamp {
    mtime: SystemTime,
    len: u64,
}

fn stamp(path: &std::path::Path) -> Option<FileStamp> {
    let m = std::fs::metadata(path).ok()?;
    Some(FileStamp { mtime: m.modified().ok()?, len: m.len() })
}

/// Notice saves of local profile files (pasted into an editor, Ctrl+S) and
/// take them in without any further action from the user.
///
/// Polling keeps this free of inotify plumbing and copes with every way
/// editors save: in place, or a new file renamed over the old one.
/// Switch off temporary rules whose time is up (or whose boot is over),
/// then let the affected connections reconnect through the usual route.
pub async fn expire_rules(state: Arc<State>) {
    let boot = crate::user_rules::boot_id();
    let mut tick = tokio::time::interval(Duration::from_secs(30));
    loop {
        tick.tick().await;
        let now = crate::problems::now_ms();
        let Ok((rules, _)) = state.config.rules().await else { continue };
        let due: Vec<crate::user_rules::Stored> = rules.into_iter().filter(|s| s.expired(now, &boot)).collect();
        if due.is_empty() {
            continue;
        }
        let result = state
            .config
            .change_rules(&state, false, |store| {
                let mut changes = Vec::new();
                for s in &due {
                    let patch = crate::user_rules::Patch { enabled: Some(false), ..Default::default() };
                    if let Some((_, c)) = store.update(s.rule.id, patch, now).map_err(db_err)? {
                        changes.push(c);
                    }
                }
                Ok(((), changes))
            })
            .await;
        match result {
            Ok(((), report)) => {
                tracing::info!("switched off {} expired rule(s)", due.len());
                if report.reloaded {
                    for s in &due {
                        crate::route_check::close_matching(&state, &s.rule).await;
                    }
                }
            }
            Err(e) => tracing::warn!("expiring rules: {}", e.message),
        }
    }
}

pub async fn watch_files(state: Arc<State>) {
    let mut seen: HashMap<String, FileStamp> = HashMap::new();
    loop {
        let files = state.config.local_files().await;
        seen.retain(|id, _| files.iter().any(|(f, ..)| f == id));
        for (id, draft, stamp) in files {
            let Some(stamp) = stamp else { continue };
            // A draft saved before its first poll still counts as a save;
            // other files seen for the first time are just remembered.
            let changed = match seen.insert(id.clone(), stamp) {
                Some(old) => old != stamp,
                None => draft,
            };
            if changed {
                state.config.file_saved(&state, &id).await;
            }
        }
        tokio::time::sleep(FILE_POLL_EVERY).await;
    }
}

/// Download through our own core when it runs: the subscription host may be
/// unreachable directly.
pub(crate) fn proxy_of(state: &State) -> Option<String> {
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
