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
            mixed_port: c.mixed_port,
            error: None,
            tun_issue: tun_issue(state, c.tun.enable).await,
        },
        (Err(e), _) | (_, Err(e)) => CoreStatus {
            running: false,
            socket,
            // Absent core is the normal "off" state, not an error worth showing.
            error: (!e.is_unreachable()).then(|| e.to_string()),
            ..Default::default()
        },
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
