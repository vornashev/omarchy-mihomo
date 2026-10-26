//! Sleep and network changes. While the machine is offline, and for a short
//! while after it wakes up or joins another network, failures say nothing
//! about routes: the collector and autofix skip them and dead-group notices
//! wait. Then stale connections are closed and the groups that pick nodes
//! by themselves are tested again, so they do not sit on a node chosen
//! while nothing worked.

use std::sync::Arc;
use std::time::Duration;

use futures_util::future::join_all;
use omm_proto::NetworkStatus;

use crate::problems::now_ms;
use crate::state::State;

const TICK: Duration = Duration::from_secs(3);
/// A gap in the suspend clock shorter than this is no sleep.
const MIN_SLEEP_MS: i64 = 2_000;
/// Failures are not counted this long after waking up or a new network.
const SETTLE_MS: i64 = 30_000;
/// DHCP and DNS take a moment after the route appears.
const RECOVER_AFTER: Duration = Duration::from_secs(5);

#[derive(Debug, Default)]
pub struct Net {
    offline: bool,
    settle_until: i64,
    /// Bumped on every change; a recovery started for an older one gives up.
    generation: u64,
}

impl Net {
    pub fn quiet(&self, now: i64) -> bool {
        self.offline || now < self.settle_until
    }

    fn status(&self, now: i64) -> NetworkStatus {
        NetworkStatus { offline: self.offline, settling: !self.offline && now < self.settle_until }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Change {
    Offline,
    /// Woke up after this many milliseconds.
    Resumed(i64),
    /// Another network, or back online.
    Switched,
}

/// What happened between two looks at the uplinks and the suspend clock.
fn classify(before: Option<&[String]>, after: Option<&[String]>, slept: i64) -> Option<Change> {
    if after.is_some_and(<[String]>::is_empty) {
        return (before != after).then_some(Change::Offline);
    }
    if slept >= MIN_SLEEP_MS {
        return Some(Change::Resumed(slept));
    }
    (before != after).then_some(Change::Switched)
}

pub async fn watch(state: Arc<State>) {
    let mut links = omm_platform::net::uplinks();
    let mut slept = omm_platform::net::slept_ms();
    state.network.lock().unwrap().offline = links.as_ref().is_some_and(Vec::is_empty);
    loop {
        state.set_network_status(state.network.lock().unwrap().status(now_ms()));
        tokio::time::sleep(TICK).await;
        let (now_links, now_slept) = (omm_platform::net::uplinks(), omm_platform::net::slept_ms());
        let change = classify(links.as_deref(), now_links.as_deref(), now_slept - slept);
        slept = now_slept;
        if let Some(change) = change {
            tracing::info!(?change, uplinks = ?now_links, "network");
            react(&state, change);
        }
        links = now_links;
    }
}

fn react(state: &Arc<State>, change: Change) {
    {
        let mut a = state.autofix.lock().unwrap();
        a.forget_failures();
        if let Change::Resumed(ms) = change {
            a.postpone_trials(ms);
        }
    }
    let generation = {
        let mut net = state.network.lock().unwrap();
        net.generation += 1;
        net.offline = change == Change::Offline;
        net.settle_until = now_ms() + SETTLE_MS;
        net.generation
    };
    if change != Change::Offline {
        tokio::spawn(recover(state.clone(), generation));
    }
}

/// Drop connections bound to the old network and re-test auto groups.
async fn recover(state: Arc<State>, generation: u64) {
    tokio::time::sleep(RECOVER_AFTER).await;
    if state.network.lock().unwrap().generation != generation || !state.core_running() {
        return;
    }
    // Clash Verge's core, observed read-only, is not ours to disturb.
    if state.config.owns_core(&state)
        && let Err(e) = state.core.close_all_connections().await
    {
        tracing::warn!("closing connections after a network change: {e}");
    }
    let Ok(p) = state.core.proxies().await else { return };
    let groups: Vec<String> = crate::methods::groups(&p)
        .into_iter()
        .filter(|g| matches!(g.kind.as_str(), "URLTest" | "Fallback" | "LoadBalance"))
        .map(|g| g.name)
        .collect();
    let tests = groups.iter().map(|g| state.core.group_delay(g, crate::methods::DEFAULT_TEST_URL, crate::methods::DEFAULT_TEST_TIMEOUT_MS));
    let alive = join_all(tests).await.iter().filter(|r| r.as_ref().is_ok_and(|d| d.values().any(|&ms| ms > 0))).count();
    tracing::info!("re-tested {} auto group(s) after a network change, {alive} with a working node", groups.len());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn links(l: &[&str]) -> Vec<String> {
        l.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn changes() {
        let home = links(&["wlp5s0 192.168.0.5", "wlp5s0 via 192.168.0.1"]);
        let cafe = links(&["wlp5s0 10.0.0.7", "wlp5s0 via 10.0.0.1"]);
        let none: Vec<String> = Vec::new();
        assert_eq!(classify(Some(&home), Some(&home), 0), None);
        assert_eq!(classify(Some(&home), Some(&home), 1_500), None, "clock noise is no sleep");
        assert_eq!(classify(Some(&home), Some(&home), 60_000), Some(Change::Resumed(60_000)));
        assert_eq!(classify(Some(&home), Some(&cafe), 0), Some(Change::Switched));
        assert_eq!(classify(Some(&home), Some(&none), 0), Some(Change::Offline));
        assert_eq!(classify(Some(&none), Some(&none), 60_000), None, "still offline after a sleep");
        assert_eq!(classify(Some(&none), Some(&home), 0), Some(Change::Switched));
        // Where uplinks cannot be told, only sleep counts.
        assert_eq!(classify(None, None, 0), None);
        assert_eq!(classify(None, None, 5_000), Some(Change::Resumed(5_000)));
    }

    #[test]
    fn quiet_while_offline_and_settling() {
        let mut net = Net::default();
        assert!(!net.quiet(1_000));
        net.offline = true;
        assert!(net.quiet(1_000));
        assert_eq!(net.status(1_000), NetworkStatus { offline: true, settling: false });
        net.offline = false;
        net.settle_until = 2_000;
        assert!(net.quiet(1_999));
        assert!(!net.quiet(2_000));
        assert_eq!(net.status(1_000), NetworkStatus { offline: false, settling: true });
    }
}
