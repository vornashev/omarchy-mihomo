//! One file that moves a setup to another machine: settings (DNS and rule
//! sets included), automation, the user's permanent rules and, unless left
//! out, the profiles with their overrides. Profiles carry subscription
//! links with tokens, so the file is private (0600).
//!
//! Importing never deletes: rules are added through the journal (undoable
//! in the rules history), profiles whose id exists are kept as they are,
//! and a machine of another platform keeps its own TUN settings. The state
//! before the import is saved next to the data first.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use omm_config::profiles::Profile;
use omm_config::rules::MatchKind;
use omm_config::settings::Settings;
use omm_proto::{ErrorCode, RpcError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{Manager, cfg_err, db_err};
use crate::state::State;

pub const FORMAT: &str = "omarchy-mihomo-backup";
const VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backup {
    pub format: String,
    pub version: u32,
    /// Unix milliseconds.
    pub created: i64,
    pub platform: String,
    pub settings: Settings,
    pub automation: Value,
    pub rules: Vec<Rule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profiles: Option<Profiles>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub kind: MatchKind,
    pub value: String,
    pub target: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profiles {
    pub current: Option<String>,
    pub items: Vec<Profile>,
    /// Profile id → its YAML.
    pub files: BTreeMap<String, String>,
    /// Scope (`global` or a profile id) → file name → text.
    pub overrides: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub rules: usize,
    pub profiles_added: Vec<String>,
    pub profiles_kept: Vec<String>,
    /// TUN stayed as it was: the backup comes from another platform.
    pub kept_tun: bool,
    /// The state before the import.
    pub before: String,
}

fn bad(m: impl Into<String>) -> RpcError {
    RpcError::new(ErrorCode::BadRequest, m)
}

fn io(path: &Path, e: std::io::Error) -> RpcError {
    RpcError::new(ErrorCode::Internal, format!("{}: {e}", path.display()))
}

/// Profile ids and override scopes become paths: nothing but a plain name.
fn plain(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) && !name.starts_with('.')
}

/// The YAML files of one override scope; editor state stays behind.
fn read_scope(dir: &Path) -> BTreeMap<String, String> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            (name.ends_with(".yaml") && plain(&name)).then(|| Some((name, std::fs::read_to_string(e.path()).ok()?))).flatten()
        })
        .collect()
}

impl Manager {
    pub async fn export_backup(&self, state: &State, with_profiles: bool) -> Result<Backup, RpcError> {
        let inner = self.inner.lock().await;
        let rules = inner
            .rules
            .all()
            .map_err(db_err)?
            .into_iter()
            // Temporary rules and trials belong to this machine's moment.
            .filter(|s| s.expires.is_none() && s.boot.is_none())
            .map(|s| Rule { kind: s.rule.kind, value: s.rule.value, target: s.rule.target, enabled: s.rule.enabled, note: s.note, source: s.source })
            .collect();
        let profiles = with_profiles.then(|| {
            let items: Vec<Profile> = inner.store.index.items.iter().filter(|p| !p.draft).cloned().collect();
            let files = items.iter().filter_map(|p| Some((p.id.clone(), inner.store.text(&p.id).ok()?))).collect();
            let overrides = std::iter::once(omm_config::profiles::GLOBAL.to_owned())
                .chain(items.iter().map(|p| p.id.clone()))
                .map(|scope| (scope.clone(), read_scope(&self.paths.overrides(&scope))))
                .filter(|(_, files)| !files.is_empty())
                .collect();
            Profiles { current: inner.store.index.current.clone(), items, files, overrides }
        });
        let automation = serde_json::to_value(&state.autofix.lock().unwrap().settings).unwrap_or_default();
        Ok(Backup {
            format: FORMAT.into(),
            version: VERSION,
            created: crate::problems::now_ms(),
            platform: omm_platform::NAME.into(),
            settings: inner.settings.clone(),
            automation,
            rules,
            profiles,
        })
    }

    pub async fn import_backup(&self, state: &State, b: Backup) -> Result<Report, RpcError> {
        if b.format != FORMAT {
            return Err(bad("not an omarchy-mihomo backup"));
        }
        if b.version > VERSION {
            return Err(bad(format!("backup version {} is newer than this omarchy-mihomo understands", b.version)));
        }
        let automation: crate::autofix::Automation = serde_json::from_value(b.automation.clone()).map_err(|e| bad(format!("automation: {e}")))?;
        if let Some(p) = &b.profiles
            && let Some(id) = p.items.iter().map(|i| &i.id).chain(p.overrides.keys()).find(|id| !plain(id))
        {
            return Err(bad(format!("unsafe profile id in the backup: {id:?}")));
        }
        let before = self.export_backup(state, true).await?;
        let global_before = before.profiles.as_ref().and_then(|p| p.overrides.get(omm_config::profiles::GLOBAL)).cloned().unwrap_or_default();
        let before_path = self.paths.root.join("backup").join(format!("before-import-{}.json", before.created));
        write(&before_path, &before)?;

        let mut report = Report { before: before_path.display().to_string(), ..Default::default() };
        let mut inner = self.inner.lock().await;
        let old_settings = inner.settings.clone();
        let old_current = inner.store.index.current.clone();

        let mut settings = b.settings;
        if b.platform != omm_platform::NAME {
            settings.tun = old_settings.tun.clone();
            report.kept_tun = true;
        }

        // Profiles first: the rules may target their groups.
        let mut added: Vec<String> = Vec::new();
        if let Some(p) = b.profiles {
            for profile in p.items {
                if inner.store.get(&profile.id).is_ok() {
                    report.profiles_kept.push(profile.name);
                    continue;
                }
                let Some(text) = p.files.get(&profile.id) else { continue };
                inner.store.write_text(&profile.id, text).map_err(cfg_err)?;
                report.profiles_added.push(profile.name.clone());
                added.push(profile.id.clone());
                inner.store.insert(profile);
            }
            for (scope, files) in &p.overrides {
                // A kept profile keeps its overrides too.
                let ours = scope == omm_config::profiles::GLOBAL || added.contains(scope);
                if !ours {
                    continue;
                }
                for (name, text) in files.iter().filter(|(n, _)| plain(n)) {
                    omm_config::paths::write_atomic(&self.paths.overrides(scope).join(name), text.as_bytes()).map_err(cfg_err)?;
                }
            }
            if old_current.is_none()
                && let Some(c) = p.current.filter(|c| inner.store.get(c).is_ok())
            {
                inner.store.index.current = Some(c);
            }
            inner.store.save().map_err(cfg_err)?;
        }

        settings.save(&self.paths.settings()).map_err(cfg_err)?;
        inner.settings = settings;
        let old_automation = state.autofix.lock().unwrap().settings.clone();
        state.autofix.lock().unwrap().set_settings(automation)?;

        let now = crate::problems::now_ms();
        let disabled: Vec<(MatchKind, String)> = b.rules.iter().filter(|r| !r.enabled).map(|r| (r.kind, r.value.clone())).collect();
        let news = b
            .rules
            .into_iter()
            .map(|r| crate::user_rules::NewRule { kind: r.kind, value: r.value, target: r.target, note: r.note, source: r.source, test: None, lifetime: Default::default() })
            .collect();
        let (_, mut changes) = inner.rules.upsert_many(news, now).map_err(db_err)?;
        for (kind, value) in disabled {
            if let Some(s) = inner.rules.find(kind, &value).map_err(db_err)?
                && let Some((_, c)) = inner.rules.update(s.rule.id, crate::user_rules::Patch { enabled: Some(false), ..Default::default() }, now).map_err(db_err)?
            {
                changes.push(c);
            }
        }
        report.rules = changes.len();

        if inner.store.current().is_none() {
            return Ok(report);
        }
        if let Err(e) = self.apply_locked(&mut inner, state).await {
            // Put this machine back as it was.
            if let Some(first) = changes.iter().map(|c| c.id).min() {
                inner.rules.discard_since(first).map_err(db_err)?;
            }
            for id in &added {
                let _ = inner.store.remove(id);
            }
            let global = self.paths.overrides(omm_config::profiles::GLOBAL);
            for name in read_scope(&global).keys().filter(|n| !global_before.contains_key(*n)) {
                let _ = std::fs::remove_file(global.join(name));
            }
            for (name, text) in &global_before {
                omm_config::paths::write_atomic(&global.join(name), text.as_bytes()).map_err(cfg_err)?;
            }
            old_settings.save(&self.paths.settings()).map_err(cfg_err)?;
            inner.settings = old_settings;
            inner.store.index.current = old_current;
            inner.store.save().map_err(cfg_err)?;
            let _ = state.autofix.lock().unwrap().set_settings(old_automation);
            if let Err(again) = self.apply_locked(&mut inner, state).await {
                tracing::warn!("re-apply after a failed import: {}", again.message);
            }
            return Err(RpcError::new(e.code, format!("the imported setup does not apply, nothing changed: {}", e.message)));
        }
        state.set_config_status(inner.status.clone());
        Ok(report)
    }
}

fn write(path: &Path, b: &Backup) -> Result<(), RpcError> {
    let text = serde_json::to_vec_pretty(b).map_err(|e| RpcError::new(ErrorCode::Internal, e.to_string()))?;
    omm_config::paths::write_atomic(path, &text).map_err(cfg_err)
}

/// `~/omarchy-mihomo-<date>.json` unless a path is given.
fn export_path(path: Option<String>) -> PathBuf {
    match path.filter(|p| !p.trim().is_empty()) {
        Some(p) => PathBuf::from(p.strip_prefix("~/").map(|rest| omm_platform::home().join(rest)).unwrap_or_else(|| PathBuf::from(&p))),
        None => {
            let day = crate::problems::now_ms() / 86_400_000;
            omm_platform::home().join(format!("omarchy-mihomo-{}.json", civil_date(day)))
        }
    }
}

/// `2026-10-27` for days since 1970-01-01 (Howard Hinnant's algorithm).
fn civil_date(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

pub async fn rpc(state: &std::sync::Arc<State>, method: &str, params: Value) -> Result<Value, RpcError> {
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct P {
        path: Option<String>,
        profiles: Option<bool>,
    }
    let p: P = if params.is_null() { P::default() } else { serde_json::from_value(params).map_err(|e| bad(e.to_string()))? };
    match method {
        "backup.export" => {
            let b = state.config.export_backup(state, p.profiles.unwrap_or(true)).await?;
            let path = export_path(p.path);
            write(&path, &b)?;
            Ok(json!({
                "path": path.display().to_string(),
                "rules": b.rules.len(),
                "profiles": b.profiles.as_ref().map_or(0, |p| p.items.len()),
            }))
        }
        "backup.import" => {
            let path = export_path(Some(p.path.ok_or_else(|| bad("path is required"))?));
            let text = std::fs::read_to_string(&path).map_err(|e| io(&path, e))?;
            let b: Backup = serde_json::from_str(&text).map_err(|e| bad(format!("{}: {e}", path.display())))?;
            let report = state.config.import_backup(state, b).await?;
            state.request_refresh();
            serde_json::to_value(report).map_err(|e| RpcError::new(ErrorCode::Internal, e.to_string()))
        }
        _ => Err(RpcError::new(ErrorCode::UnknownMethod, method.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omm_config::profiles::Source;
    use std::os::unix::fs::PermissionsExt;

    const PROFILE: &str = "proxies: []\nproxy-groups: [{name: AI, type: select, proxies: [DIRECT]}]\nrules: [MATCH,DIRECT]\n";

    /// A daemon state over `dir` whose "core" validates with `exit`.
    fn state(dir: &Path, exit: i32) -> State {
        let bin = dir.join("fake-mihomo");
        std::fs::write(&bin, format!("#!/bin/sh\nexit {exit}\n")).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let paths = omm_config::paths::Paths::new(dir.join("data"));
        paths.ensure().unwrap();
        State::new(
            mihomo_api::Client::new(dir.join("core.sock")),
            Manager::open(paths, Some(bin)).unwrap(),
            omm_problems::Collector::open(&dir.join("problems.db")).unwrap(),
            crate::system_proxy::Manager::open(dir.join("proxy.json")).unwrap(),
        )
    }

    fn rule(kind: MatchKind, value: &str, target: &str) -> crate::user_rules::NewRule {
        crate::user_rules::NewRule { kind, value: value.into(), target: target.into(), note: None, source: "user".into(), test: None, lifetime: Default::default() }
    }

    async fn source() -> (tempfile::TempDir, Backup) {
        let dir = tempfile::tempdir().unwrap();
        let a = state(dir.path(), 0);
        {
            let mut inner = a.config.inner.lock().await;
            let profile = Profile { id: "p1".into(), name: "Home".into(), source: Source::Local, updated: None, interval_hours: None, usage: None, etag: None, last_error: None, draft: false };
            inner.store.write_text("p1", PROFILE).unwrap();
            inner.store.insert(profile);
            inner.store.save().unwrap();
            inner.settings.dns.sniffer = Some(true);
            inner.settings.tun.stack = "system".into();
            inner.rules.upsert(rule(MatchKind::DomainSuffix, "example.org", "AI"), 1).unwrap();
            let (off, _) = inner.rules.upsert(rule(MatchKind::ProcessName, "steam", "DIRECT"), 1).unwrap();
            inner.rules.update(off.rule.id, crate::user_rules::Patch { enabled: Some(false), ..Default::default() }, 2).unwrap();
            let mut temporary = rule(MatchKind::Domain, "trial.org", "AI");
            temporary.lifetime.expires = Some(i64::MAX);
            inner.rules.upsert(temporary, 3).unwrap();
        }
        let global = a.config.paths().overrides("global");
        std::fs::write(global.join("merge.yaml"), "dns: {enable: true}\n").unwrap();
        std::fs::write(global.join("editor-history.json"), "{}").unwrap();
        let b = a.config.export_backup(&a, true).await.unwrap();
        // Through text, as the file goes.
        let b = serde_json::from_str(&serde_json::to_string(&b).unwrap()).unwrap();
        (dir, b)
    }

    #[tokio::test]
    async fn moves_a_setup_to_a_fresh_machine_and_again_without_duplicates() {
        let (_src, b) = source().await;
        assert_eq!(b.rules.len(), 2, "the temporary rule stays behind");
        let p = b.profiles.as_ref().unwrap();
        assert_eq!(p.overrides["global"].keys().collect::<Vec<_>>(), vec!["merge.yaml"]);

        let dir = tempfile::tempdir().unwrap();
        let s = state(dir.path(), 0);
        let report = s.config.import_backup(&s, b.clone()).await.unwrap();
        assert_eq!(report.profiles_added, vec!["Home".to_owned()]);
        assert!(!report.kept_tun);
        assert!(Path::new(&report.before).is_file());
        let inner = s.config.inner.lock().await;
        assert_eq!(inner.store.index.current.as_deref(), Some("p1"));
        assert_eq!(inner.settings.dns.sniffer, Some(true));
        let rules = inner.rules.all().unwrap();
        assert_eq!(rules.len(), 2);
        assert!(!rules.iter().find(|r| r.rule.value == "steam").unwrap().rule.enabled);
        assert!(s.config.paths().overrides("global").join("merge.yaml").is_file());
        assert!(s.config.paths().runtime_config().is_file(), "applied");
        drop(inner);

        let again = s.config.import_backup(&s, b).await.unwrap();
        assert_eq!(again.profiles_kept, vec!["Home".to_owned()]);
        assert_eq!(s.config.inner.lock().await.rules.all().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn another_platform_keeps_tun_and_a_rejected_setup_changes_nothing() {
        let (_src, mut b) = source().await;
        b.platform = "elsewhere".into();
        let dir = tempfile::tempdir().unwrap();
        let s = state(dir.path(), 0);
        let report = s.config.import_backup(&s, b.clone()).await.unwrap();
        assert!(report.kept_tun);
        assert_eq!(s.config.settings().await.tun.stack, "gvisor");

        let dir = tempfile::tempdir().unwrap();
        let s = state(dir.path(), 1);
        let e = s.config.import_backup(&s, b.clone()).await.unwrap_err();
        assert!(e.message.contains("nothing changed"), "{}", e.message);
        assert!(s.config.inner.lock().await.rules.all().unwrap().is_empty());
        assert!(s.config.inner.lock().await.store.index.items.is_empty(), "added profiles go too");
        assert!(!s.config.paths().overrides("global").join("merge.yaml").exists(), "and global overrides");
        assert_eq!(s.config.settings().await.dns.sniffer, None);

        b.format = "something".into();
        assert!(s.config.import_backup(&s, b).await.is_err());
    }

    #[test]
    fn dates_and_names() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(20_753), "2026-10-27");
        assert!(plain("p6acfcaaf") && plain("merge.yaml"));
        assert!(!plain("../x") && !plain("a/b") && !plain(".hidden") && !plain(""));
    }
}
