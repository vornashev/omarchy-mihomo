//! Checking a route before a rule is saved, and moving live connections
//! onto a new rule afterwards.

use std::net::IpAddr;

use futures_util::future::join_all;
use omm_config::rules::UserRule;
use serde::Serialize;

use crate::state::State;

pub const CHECK_TIMEOUT_MS: u32 = 5000;

#[derive(Debug, Clone, Serialize)]
pub struct Probe {
    pub url: String,
    /// The URL check may fail even when the service works: raw IPs (no
    /// matching certificate) and non-web ports.
    pub inconclusive: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub target: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// URL that exercises the real resource rather than a generic test page.
pub fn probe_for(host: &str, port: Option<u16>) -> Probe {
    let ip = host.parse::<IpAddr>().ok();
    let shown = match ip {
        Some(IpAddr::V6(_)) => format!("[{host}]"),
        _ => host.to_owned(),
    };
    let (url, odd_port) = match port {
        Some(80) => (format!("http://{shown}/"), false),
        None | Some(443) => (format!("https://{shown}/"), false),
        Some(p) => (format!("https://{shown}:{p}/"), true),
    };
    Probe { url, inconclusive: ip.is_some() || odd_port }
}

/// Measure every target in parallel through mihomo's delay API.
pub async fn check(state: &State, probe: &Probe, targets: &[String]) -> Vec<Outcome> {
    let runs = targets.iter().map(|t| async move {
        match state.core.proxy_delay(t, &probe.url, CHECK_TIMEOUT_MS).await {
            Ok(d) if d.delay > 0 => Outcome { target: t.clone(), ok: true, delay: Some(d.delay), error: None },
            Ok(_) => Outcome { target: t.clone(), ok: false, delay: None, error: Some("no response".into()) },
            Err(mihomo_api::Error::Status { status: 504, .. }) => Outcome { target: t.clone(), ok: false, delay: None, error: Some("timeout".into()) },
            Err(mihomo_api::Error::Status { message, .. }) => Outcome { target: t.clone(), ok: false, delay: None, error: Some(message) },
            Err(e) => Outcome { target: t.clone(), ok: false, delay: None, error: Some(e.to_string()) },
        }
    });
    join_all(runs).await
}

/// Close live connections the rule now covers, so apps reconnect through
/// the new route instead of keeping the old one until they idle out.
pub async fn close_matching(state: &State, rule: &UserRule) -> usize {
    let Ok(conns) = state.core.connections().await else { return 0 };
    let ids: Vec<String> = conns
        .connections
        .into_iter()
        .filter(|c| {
            let m = &c.metadata;
            let conn = crate::suggest::conn(m);
            rule.matches(&conn) == Some(true) || (!m.sniff_host.is_empty() && rule.matches(&omm_config::rules::Conn { host: &m.sniff_host, ..conn }) == Some(true))
        })
        .map(|c| c.id)
        .collect();
    let closes = ids.iter().map(|id| state.core.close_connection(id));
    join_all(closes).await.into_iter().filter(Result::is_ok).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probes() {
        let p = probe_for("api.anthropic.com", Some(443));
        assert_eq!((p.url.as_str(), p.inconclusive), ("https://api.anthropic.com/", false));
        assert_eq!(probe_for("example.org", Some(80)).url, "http://example.org/");
        let p = probe_for("149.154.167.51", Some(443));
        assert!(p.inconclusive);
        let p = probe_for("2001:db8::1", Some(8443));
        assert_eq!((p.url.as_str(), p.inconclusive), ("https://[2001:db8::1]:8443/", true));
        assert_eq!(probe_for("x.com", None).url, "https://x.com/");
    }
}
