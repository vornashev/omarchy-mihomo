//! Feeding mihomo's log into the problem collector.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mihomo_api::model::LogLine;
use omm_problems::{Collector, Counts, Problem, parse};
use omm_proto::topic;
use serde::Serialize;

use crate::state::State;

const FLUSH_EVERY: Duration = Duration::from_secs(2);
const PURGE_EVERY: Duration = Duration::from_secs(3600);
/// Traffic is written once a minute, so an unnamed IP is looked up again
/// no sooner than that.
const NAME_EVERY: Duration = Duration::from_secs(60);

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
                    // Offline or just woke up: everything fails, nothing is learned.
                    if state.network_quiet() {
                        continue;
                    }
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
    let mut tried = HashMap::new();
    loop {
        tick.tick().await;
        let named = name_ips(&state, &mut tried);
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
        if update.recorded > 0 || named > 0 || Some(update.counts) != last {
            last = Some(update.counts);
            state.publish(topic::PROBLEMS, &update);
        }
    }
}

/// Name IP-only problems from the traffic history. Returns how many got a name.
fn name_ips(state: &State, tried: &mut HashMap<String, tokio::time::Instant>) -> usize {
    let now = tokio::time::Instant::now();
    let unnamed = state.problems.lock().unwrap().unnamed();
    tried.retain(|h, _| unnamed.iter().any(|p| &p.host == h));
    let todo: Vec<&Problem> = unnamed.iter().filter(|p| tried.get(&p.host).is_none_or(|t| now - *t >= NAME_EVERY)).collect();
    if todo.is_empty() {
        return 0;
    }
    let found: Vec<(String, String)> = {
        let traffic = state.traffic.lock().unwrap();
        todo.into_iter()
            .filter_map(|p| {
                tried.insert(p.host.clone(), now);
                match traffic.names(&p.host) {
                    Ok(seen) => pick_name(p, &seen).map(|n| (p.host.clone(), n)),
                    Err(e) => {
                        tracing::warn!("naming {}: {e}", p.host);
                        None
                    }
                }
            })
            .collect()
    };
    let mut c = state.problems.lock().unwrap();
    for (host, name) in &found {
        c.set_name(host, name.clone());
    }
    found.len()
}

/// The name an IP-only problem most likely stands for. Connections that took
/// the same domain rule as the failure settle it; otherwise one host has to
/// carry most of the IP's traffic, since a CDN address serves many sites.
fn pick_name(p: &Problem, seen: &[crate::traffic::Seen]) -> Option<String> {
    let r = &p.route;
    let rule = if r.payload.is_empty() { r.rule.clone() } else { format!("{},{}", r.rule, r.payload) };
    let by_domain = r.rule.starts_with("Domain");
    let same_rule = |s: &crate::traffic::Seen| !r.rule.is_empty() && r.rule != "Match" && s.rule.eq_ignore_ascii_case(&rule);
    let same_app = |s: &crate::traffic::Seen| p.apps.contains_key(&s.app);
    let tier = |s: &crate::traffic::Seen| 2 * same_rule(s) as u8 + same_app(s) as u8;
    let best = seen.iter().map(tier).max()?;
    let mut hosts: HashMap<&str, u64> = HashMap::new();
    for s in seen.iter().filter(|s| tier(s) == best) {
        *hosts.entry(s.host.as_str()).or_default() += s.conns;
    }
    let total: u64 = hosts.values().sum();
    let (host, n) = hosts.into_iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))?;
    (best >= 2 && by_domain || n * 3 >= total * 2).then(|| host.to_owned())
}

/// Publish counts right away (after hide, mark_seen...).
pub fn announce(state: &State) {
    let counts = state.problems.lock().unwrap().counts();
    state.publish(topic::PROBLEMS, &Update { counts, recorded: 0 });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::Seen;

    fn problem(line: &str) -> Problem {
        let mut c = Collector::in_memory().unwrap();
        let line = parse::parse(line).expect("parses");
        c.ingest(&line, 1);
        c.unnamed().pop().expect("an IP problem")
    }

    fn seen(host: &str, app: &str, rule: &str, conns: u64) -> Seen {
        Seen { host: host.into(), app: app.into(), rule: rule.into(), conns }
    }

    #[test]
    fn a_domain_rule_picks_among_its_own_hosts() {
        let p = problem("[TCP] dial AI (match DomainSuffix/chatgpt.com) 198.18.0.1:1(ChatGPT) --> 8.6.112.0:443 error: i/o timeout");
        let s = [
            seen("openrouter.ai", "zen-bin", "DomainSuffix,openrouter.ai", 1500),
            seen("ab.chatgpt.com", "ChatGPT", "DomainSuffix,chatgpt.com", 200),
            seen("chatgpt.com", "ChatGPT", "DomainSuffix,chatgpt.com", 600),
            seen("chatgpt.com", "zen-bin", "DomainSuffix,chatgpt.com", 6000),
        ];
        assert_eq!(pick_name(&p, &s).as_deref(), Some("chatgpt.com"));
    }

    #[test]
    fn otherwise_one_host_must_dominate() {
        let p = problem("[TCP] dial DIRECT (match Match/) 198.18.0.1:1(chrome) --> 5.35.92.49:443 error: i/o timeout");
        assert_eq!(pick_name(&p, &[seen("api.tabka.pro", "chrome", "Match", 480), seen("tabka.pro", "chrome", "Match", 5)]).as_deref(), Some("api.tabka.pro"));
        assert_eq!(pick_name(&p, &[seen("a.example", "chrome", "Match", 50), seen("b.example", "chrome", "Match", 40)]), None);
        assert_eq!(pick_name(&p, &[]), None);
    }
}
