//! Tracking the mihomo core: liveness, base config and the traffic stream.

use std::sync::Arc;
use std::time::Duration;

use mihomo_api::model::{Mode, Traffic};
use omm_proto::{CoreStatus, TunIssue, topic};

use crate::state::State;

/// Poll faster while the core is down so the UI recovers quickly after a
/// restart; when it is up, changes made through us trigger an explicit refresh.
const POLL_DOWN: Duration = Duration::from_secs(2);
const POLL_UP: Duration = Duration::from_secs(15);

pub async fn watch(state: Arc<State>) {
    loop {
        let status = probe(&state).await;
        let running = status.running;
        state.set_core_status(status);
        {
            let mut proxy = state.system_proxy.lock().await;
            let core = state.status().core;
            let port = if core.running { core.mixed_port } else { 0 };
            state.set_system_proxy_status(proxy.refresh(port).await);
        }
        state.set_issues(crate::health::check(&state).await);
        let wait = if running { POLL_UP } else { POLL_DOWN };
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = state.refresh_requested() => {}
        }
    }
}

async fn probe(state: &State) -> CoreStatus {
    let socket = state.core.socket().display().to_string();
    let (version, configs) = tokio::join!(state.core.version(), state.core.configs());
    match (version, configs) {
        (Ok(v), Ok(c)) => CoreStatus {
            running: true,
            socket,
            version: Some(v.version),
            mode: Some(mode_name(c.mode).into()),
            tun: c.tun.enable,
            tun_stack: c.tun.stack,
            mixed_port: c.mixed_port,
            error: None,
            tun_issue: tun_issue(state, c.tun.enable).await,
            ..Default::default()
        },
        (Err(e), _) | (_, Err(e)) => {
            let mut core = CoreStatus {
                socket,
                error: (!e.is_unreachable()).then(|| e.to_string()),
                ..Default::default()
            };
            if !omm_platform::IS_MACOS && state.config.owns_core(state) {
                let settings = state.config.settings().await;
                core.tun_stack = settings.tun.stack;
                core.mixed_port = settings.mixed_port;
                let output = tokio::time::timeout(Duration::from_secs(2), tokio::process::Command::new("systemctl")
                    .args(["--user", "show", omm_config::host::CORE_UNIT,
                        "--property=ActiveState,SubState,NRestarts,ExecMainExitTimestampMonotonic"])
                    .output()).await;
                if let Ok(Ok(out)) = output && out.status.success() {
                    let uptime = std::fs::read_to_string("/proc/uptime").ok()
                        .and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok());
                    apply_lifecycle(&mut core, &String::from_utf8_lossy(&out.stdout), uptime, crate::problems::now_ms().max(0) as u64);
                }
            }
            core
        }
    }
}

fn apply_lifecycle(core: &mut CoreStatus, properties: &str, uptime: Option<f64>, now: u64) {
    let values: std::collections::HashMap<_, _> = properties.lines().filter_map(|s| s.split_once('=')).collect();
    let sub = values.get("SubState").copied().unwrap_or_default();
    core.restarting = sub == "auto-restart" || sub == "auto-restart-queued";
    core.starting = !core.restarting && matches!(values.get("ActiveState"), Some(&"activating") | Some(&"active"));
    core.restart_count = values.get("NRestarts").and_then(|s| s.parse().ok()).unwrap_or_default();
    let exit_us = values.get("ExecMainExitTimestampMonotonic").and_then(|s| s.parse::<u64>().ok()).unwrap_or_default();
    if core.restarting && exit_us > 0 && let Some(uptime) = uptime {
        let ago_ms = (uptime * 1000.0 - exit_us as f64 / 1000.0).max(0.0) as u64;
        core.crashed_at = Some(now.saturating_sub(ago_ms));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_distinguishes_restart_start_and_stop() {
        let mut core = CoreStatus::default();
        apply_lifecycle(&mut core, "ActiveState=activating\nSubState=auto-restart\nNRestarts=2\nExecMainExitTimestampMonotonic=60000000", Some(100.0), 200_000);
        assert!(core.restarting);
        assert!(!core.starting);
        assert_eq!(core.restart_count, 2);
        assert_eq!(core.crashed_at, Some(160_000));
        let mut core = CoreStatus::default();
        apply_lifecycle(&mut core, "ActiveState=active\nSubState=running", None, 0);
        assert!(core.starting);
        let mut core = CoreStatus::default();
        apply_lifecycle(&mut core, "ActiveState=failed\nSubState=failed\nNRestarts=5", None, 0);
        assert!(!core.restarting && !core.starting);
    }
}

/// mihomo turns TUN off by itself when it cannot open the device, and says
/// so only in its log. Explain the gap between the settings and the core.
async fn tun_issue(state: &State, tun_on: bool) -> Option<TunIssue> {
    if tun_on {
        *state.tun_error.lock().unwrap() = None;
        return None;
    }
    if !state.config.tun_wanted(state).await {
        return None;
    }
    let no_capabilities = omm_config::core_bin::has_capabilities(&omm_config::core_bin::default_path()) == Some(false);
    let message = state.tun_error.lock().unwrap().clone();
    Some(TunIssue { no_capabilities, other_core: omm_config::host::other_core().map(Into::into), message })
}

pub fn mode_name(m: Mode) -> &'static str {
    match m {
        Mode::Rule => "rule",
        Mode::Global => "global",
        Mode::Direct => "direct",
    }
}

/// Forward `/traffic` to subscribers. The stream stays open only while
/// someone listens, so an idle daemon does no work.
pub async fn pump_traffic(state: Arc<State>) {
    let mut status = state.watch_status();
    loop {
        if !(state.core_running() && state.listeners(topic::TRAFFIC) > 0) {
            tokio::select! {
                _ = status.changed() => {}
                _ = tokio::time::sleep(Duration::from_secs(1)) => {}
            }
            continue;
        }
        let mut stream = match state.core.traffic().await {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!("traffic stream: {e}");
                tokio::time::sleep(POLL_DOWN).await;
                continue;
            }
        };
        while state.listeners(topic::TRAFFIC) > 0 {
            match stream.next::<Traffic>().await {
                Ok(Some(t)) => state.publish(topic::TRAFFIC, &t),
                Ok(None) => break,
                Err(e) => {
                    tracing::debug!("traffic stream: {e}");
                    break;
                }
            }
        }
    }
}
