//! Feeding mihomo's log into the problem collector.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use mihomo_api::model::LogLine;
use omm_problems::{Collector, Counts, parse};
use omm_proto::topic;
use serde::Serialize;

use crate::state::State;

const FLUSH_EVERY: Duration = Duration::from_secs(2);
const PURGE_EVERY: Duration = Duration::from_secs(3600);

pub type Shared = Mutex<Collector>;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Update {
    pub counts: Counts,
    /// Failures recorded since the previous event.
    pub recorded: u64,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Keep a `/logs` stream open while the core runs and record every line.
pub async fn collect(state: Arc<State>, recorded: Arc<std::sync::atomic::AtomicU64>) {
    use std::sync::atomic::Ordering;
    let mut status = state.watch_status();
    loop {
        if !state.core_running() {
            let _ = status.changed().await;
            continue;
        }
        let mut stream = match state.core.logs("info").await {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!("log stream: {e}");
                state.request_refresh();
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };
        tracing::info!("collecting connection errors");
        loop {
            match stream.next::<LogLine>().await {
                Ok(Some(l)) => {
                    let entry = state.logs.lock().unwrap().push(l.clone(), now_ms());
                    state.publish(topic::LOGS, &entry);
                    // "Start TUN listening error: configure tun interface: operation not permitted"
                    if l.level == "error" && l.payload.contains("TUN") {
                        *state.tun_error.lock().unwrap() = Some(l.payload.clone());
                        state.request_refresh();
                    }
                    let Some(line) = parse::parse(&l.payload) else { continue };
                    state.seen.lock().unwrap().record(&line, now_ms());
                    let next = state.autofix.lock().unwrap().observe(&line, now_ms());
                    match next {
                        Some(crate::autofix::Next::Check(host)) => drop(tokio::spawn(crate::autofix::check(state.clone(), host))),
                        Some(crate::autofix::Next::RollBack(host)) => drop(tokio::spawn(crate::autofix::roll_back(state.clone(), host))),
                        None => {}
                    }
                    let mut c = state.problems.lock().unwrap();
                    if c.ingest(&line, now_ms()) {
                        recorded.fetch_add(1, Ordering::Relaxed);
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    tracing::debug!("log stream: {e}");
                    break;
                }
            }
        }
        // A closed log stream is often the first signal of a crashed core.
        // Probe immediately so the footer can show systemd's restart delay.
        state.request_refresh();
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// Persist in batches and tell subscribers when something changed.
pub async fn flush(state: Arc<State>, recorded: Arc<std::sync::atomic::AtomicU64>) {
    use std::sync::atomic::Ordering;
    let mut last = None;
    let mut purged = tokio::time::Instant::now();
    let mut tick = tokio::time::interval(FLUSH_EVERY);
    loop {
        tick.tick().await;
        let update = {
            let mut c = state.problems.lock().unwrap();
            if let Err(e) = c.flush() {
                tracing::warn!("saving problems: {e}");
            }
            if purged.elapsed() >= PURGE_EVERY {
                purged = tokio::time::Instant::now();
                match c.purge(now_ms()) {
                    Ok(n) if n > 0 => tracing::info!("forgot {n} old problems"),
                    Err(e) => tracing::warn!("purging problems: {e}"),
                    _ => {}
                }
            }
            Update { counts: c.counts(), recorded: recorded.swap(0, Ordering::Relaxed) }
        };
        if update.recorded > 0 || Some(update.counts) != last {
            last = Some(update.counts);
            state.publish(topic::PROBLEMS, &update);
        }
    }
}

/// Publish counts right away (after hide, mark_seen...).
pub fn announce(state: &State) {
    let counts = state.problems.lock().unwrap().counts();
    state.publish(topic::PROBLEMS, &Update { counts, recorded: 0 });
}
