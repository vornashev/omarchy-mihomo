//! Setup checks and their fixes.
//!
//! [`check`] lists what stands in the way of a working proxy, each issue
//! with the fix that removes it. Harmless fixes also run on their own:
//! the core is downloaded when missing and started once a profile is
//! usable, unless another client's core is running. Fixes that need sudo
//! (`setcap`, switching off Clash Verge's root service) are only offered:
//! the shell runs `omm fix <id>` in a terminal where sudo can ask.

use std::sync::Arc;
use std::time::Duration;

use omm_config::core_bin;
use omm_config::host::{self, AUTOTUNE_COLLECTOR, AUTOTUNE_TIMER, CORE_UNIT};
use omm_proto::{ErrorCode, Fix, Issue, IssueLevel, RpcError};
use serde_json::{Value, json};

use crate::state::State;

pub const INSTALL_CORE: &str = "install_core";
pub const START_CORE: &str = "start_core";
pub const RESTART_CORE: &str = "restart_core";
pub const IMPORT_CVR: &str = "import_cvr";
pub const DISABLE_AUTOTUNE: &str = "disable_autotune";
/// Privileged when TUN needs capabilities on the new binary.
pub const UPDATE_CORE: &str = "update_core";
/// Privileged: run by `omm fix` in a terminal.
pub const SETCAP: &str = "setcap";
pub const MIGRATE_CVR: &str = "migrate_cvr";

#[derive(Debug, Clone, Default)]
pub enum Install {
    #[default]
    Idle,
    Running,
    Failed(String),
}

fn fix(id: &str, privileged: bool, confirm: bool) -> Option<Fix> {
    Some(Fix { id: id.into(), privileged, confirm })
}

fn issue(id: &str, level: IssueLevel, detail: Option<String>, fix: Option<Fix>) -> Issue {
    Issue { id: id.into(), level, detail, fix }
}

/// Everything wrong with the setup right now, most blocking first.
pub async fn check(state: &State) -> Vec<Issue> {
    let status = state.status();
    let ours = state.config.owns_core(state);
    let tun_wanted = state.config.tun_wanted(state).await;
    let has_profile = state.config.has_profile().await;
    let runtime_config = state.config.paths().runtime_config().is_file();
    let install = state.core_install.lock().unwrap().clone();
    // Process and systemd queries block; keep them off the event loop.
    let host = tokio::task::spawn_blocking(|| {
        let bin = core_bin::default_path();
        let exists = bin.is_file();
        Host {
            core_exists: exists,
            core_caps: exists && core_bin::has_capabilities(&bin) == Some(true),
            other_core: host::other_core(),
            core_active: host::unit_active(true, CORE_UNIT),
            cvr_data: host::cvr_data_dir().is_some(),
            autotune: host::unit_active(true, AUTOTUNE_TIMER) || host::unit_active(true, AUTOTUNE_COLLECTOR),
        }
    })
    .await
    .unwrap_or_default();

    let mut out = Vec::new();
    if !ours {
        // Watching someone else's core: nothing of ours to fix.
        return out;
    }
    match install {
        Install::Running => out.push(issue("core_installing", IssueLevel::Info, None, None)),
        Install::Failed(e) if !host.core_exists => out.push(issue("core_install_failed", IssueLevel::Error, Some(e), fix(INSTALL_CORE, false, false))),
        _ if !host.core_exists => out.push(issue("core_missing", IssueLevel::Error, None, fix(INSTALL_CORE, false, false))),
        _ => {}
    }
    if let Some(app) = host.other_core {
        let level = if tun_wanted { IssueLevel::Error } else { IssueLevel::Warn };
        out.push(issue("other_core", level, Some(app.into()), fix(MIGRATE_CVR, true, true)));
    }
    if host.core_exists && !host.core_caps && tun_wanted {
        out.push(issue("core_no_caps", IssueLevel::Error, None, fix(SETCAP, true, false)));
    }
    if !has_profile {
        out.push(issue("no_profile", IssueLevel::Warn, None, if host.cvr_data { fix(IMPORT_CVR, false, false) } else { None }));
    }
    if host.core_exists && runtime_config && !status.core.running && host.other_core.is_none() {
        let id = if host.core_active { RESTART_CORE } else { START_CORE };
        out.push(issue("core_stopped", IssueLevel::Warn, status.core.error.clone(), fix(id, false, false)));
    }
    if let Some(latest) = crate::updates::available(state) {
        // A new binary loses the capabilities TUN needs; sudo puts them back.
        out.push(issue("core_update", IssueLevel::Info, Some(latest), fix(UPDATE_CORE, host.core_caps, false)));
    }
    // TUN failures with a cause listed above need no second line.
    if let Some(t) = &status.core.tun_issue
        && !t.no_capabilities
        && t.other_core.is_none()
    {
        out.push(issue("tun_failed", IssueLevel::Error, t.message.clone(), fix(RESTART_CORE, false, false)));
    }
    if host.autotune {
        out.push(issue("autotune", IssueLevel::Warn, None, fix(DISABLE_AUTOTUNE, false, true)));
    }
    out
}

#[derive(Default)]
struct Host {
    core_exists: bool,
    core_caps: bool,
    other_core: Option<&'static str>,
    core_active: bool,
    cvr_data: bool,
    autotune: bool,
}

/// Run an unprivileged fix.
pub async fn run(state: &Arc<State>, id: &str) -> Result<Value, RpcError> {
    let _operation = matches!(id, START_CORE | RESTART_CORE).then(|| state.begin_operation("starting", ""));
    let result = match id {
        INSTALL_CORE => install_core(state).await,
        UPDATE_CORE => crate::updates::update_core(state).await,
        START_CORE => systemctl(&["enable", "--now", CORE_UNIT]).await,
        RESTART_CORE => systemctl(&["restart", CORE_UNIT]).await,
        IMPORT_CVR => return state.config.import_cvr(state).await,
        DISABLE_AUTOTUNE => systemctl(&["disable", "--now", AUTOTUNE_TIMER, AUTOTUNE_COLLECTOR]).await,
        SETCAP | MIGRATE_CVR => Err(RpcError::new(ErrorCode::BadRequest, format!("{id} needs sudo: run `omm fix {id}` in a terminal"))),
        _ => Err(RpcError::new(ErrorCode::NotFound, format!("no fix {id}"))),
    };
    state.request_refresh();
    result.map(|_| json!({ "ok": true }))
}

async fn systemctl(args: &[&str]) -> Result<(), RpcError> {
    let args: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
    tokio::task::spawn_blocking(move || host::user_systemctl(&args.iter().map(String::as_str).collect::<Vec<_>>()))
        .await
        .map_err(|e| RpcError::new(ErrorCode::Internal, e.to_string()))?
        .map_err(|e| RpcError::new(ErrorCode::Internal, e))
}

/// Download the pinned core when it is missing.
async fn install_core(state: &State) -> Result<(), RpcError> {
    let version = download_core(state, None, core_bin::PINNED_VERSION).await.map_err(|e| RpcError::new(ErrorCode::Fetch, e))?;
    tracing::info!(%version, "core installed");
    state.config.set_binary(core_bin::default_path());
    if state.config.has_profile().await {
        if let Err(e) = state.config.apply(state).await {
            tracing::warn!("apply after install: {}", e.message);
        }
        start_core_if_free(state).await;
    }
    state.request_refresh();
    Ok(())
}

/// Download `version` (`None` = pinned) over our core binary and return
/// the version line it prints. GitHub may be blocked, so a failed direct
/// download is retried through whichever proxy listens on the mixed port.
pub async fn download_core(state: &State, version: Option<&str>, label: &str) -> Result<String, String> {
    {
        let mut s = state.core_install.lock().unwrap();
        if matches!(*s, Install::Running) {
            return Err("the core is being installed already".into());
        }
        *s = Install::Running;
    }
    state.request_refresh();
    let port = state.config.settings().await.mixed_port;
    let operation = state.begin_operation("downloading", label);
    let proxy = format!("http://127.0.0.1:{port}");
    let dest = core_bin::default_path();
    let mut result = Err(String::new());
    for via in [None, Some(proxy.as_str())] {
        result = async {
            let rel = core_bin::resolve(version, via).await.map_err(|e| e.to_string())?;
            core_bin::install_with_progress(&rel, &dest, via, |n, total| operation.progress(n, total)).await.map_err(|e| e.to_string())
        }
        .await;
        if result.is_ok() {
            break;
        }
    }
    drop(operation);
    *state.core_install.lock().unwrap() = match &result {
        Ok(_) => Install::Idle,
        Err(e) => {
            tracing::warn!("core download failed: {e}");
            Install::Failed(e.clone())
        }
    };
    state.request_refresh();
    result
}

/// Start our core when it is safe to do so: there is a config for it and no
/// other client's core competes for the TUN device and routes.
pub async fn start_core_if_free(state: &State) {
    if !state.config.owns_core(state) || state.core_running() || !state.config.paths().runtime_config().is_file() {
        return;
    }
    let free = tokio::task::spawn_blocking(|| core_bin::default_path().is_file() && host::other_core().is_none()).await.unwrap_or(false);
    if !free {
        return;
    }
    let _operation = state.begin_operation("starting", "");
    match systemctl(&["enable", "--now", CORE_UNIT]).await {
        Ok(()) => tracing::info!("core started"),
        Err(e) => tracing::warn!("starting the core: {}", e.message),
    }
    state.request_refresh();
}

/// On startup: fetch the core if it was never installed.
pub async fn auto(state: Arc<State>) {
    if !state.config.owns_core(&state) || core_bin::default_path().is_file() {
        return;
    }
    // Let the shell connect first so it can show the progress.
    tokio::time::sleep(Duration::from_secs(2)).await;
    tracing::info!("no core binary: installing {}", core_bin::PINNED_VERSION);
    let _ = install_core(&state).await;
}
