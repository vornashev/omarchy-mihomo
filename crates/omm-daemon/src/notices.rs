//! Turning state changes into notices: the core went down or came back,
//! rules lost their group, a group has no working node left.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;

use crate::state::State;

const GROUPS_EVERY: Duration = Duration::from_secs(60);

pub async fn watch(state: Arc<State>) {
    let mut rx = state.watch_status();
    let mut prev = rx.borrow().clone();
    // Announced a crash; the next start says so too.
    let mut down = false;
    while rx.changed().await.is_ok() {
        let now = rx.borrow_and_update().clone();
        let core = &now.core;
        // A stop or restart we run ourselves shows as an operation.
        let ours = now.core_operation.is_some() || prev.core_operation.is_some();
        let crashed = core.restarting || core.crashed_at.is_some() || core.error.is_some();
        if !core.running && crashed && !ours && (prev.core.running || (down && prev.core.restarting != core.restarting)) {
            down = true;
            let log: Vec<String> = state.logs.lock().unwrap().snapshot().iter().rev().filter(|e| e.line.level == "error" || e.line.level == "fatal").take(3).map(|e| e.line.payload.clone()).collect();
            state.notify("core", "core_down", "critical", json!({ "restarting": core.restarting, "restart_count": core.restart_count, "error": core.error, "log": log }));
        } else if core.running && down {
            down = false;
            state.notify("core", "core_up", "normal", json!({ "version": core.version }));
        }

        let before: BTreeSet<i64> = prev.config.orphans.iter().copied().collect();
        let added: Vec<i64> = now.config.orphans.iter().copied().filter(|o| !before.contains(o)).collect();
        if !added.is_empty() {
            orphans(&state, &now.config.orphans).await;
        }
        prev = now;
    }
}

/// Rules whose group is gone, each with a likely replacement: `TELEGRAM`
/// for a vanished `TELEGRAM[DE]`.
async fn orphans(state: &State, ids: &[i64]) {
    let Ok((rules, _)) = state.config.rules().await else {
        return;
    };
    let groups = state
        .core
        .proxies()
        .await
        .map(|p| crate::methods::targets(&p))
        .unwrap_or_default();
    let items: Vec<_> = rules
        .iter()
        .filter(|s| ids.contains(&s.rule.id))
        .map(|s| json!({ "id": s.rule.id, "line": s.rule.line(), "target": s.rule.target, "suggest": replacement(&s.rule.target, &groups) }))
        .collect();
    if items.is_empty() {
        return;
    }
    let profile = state.status().config.profile.map(|p| p.name);
    state.notify(
        "orphans",
        "orphans",
        "normal",
        json!({ "rules": items, "profile": profile }),
    );
}

fn replacement(gone: &str, groups: &[String]) -> Option<String> {
    let base = gone.split(['[', '(']).next().unwrap_or(gone).trim();
    if base.is_empty() {
        return None;
    }
    let lower = base.to_lowercase();
    groups
        .iter()
        .find(|g| g.to_lowercase() == lower)
        .or_else(|| groups.iter().find(|g| g.to_lowercase().starts_with(&lower)))
        .cloned()
}

/// Groups whose every node failed its latest health check.
fn dead_groups(p: &mihomo_api::model::Proxies) -> Vec<(String, usize)> {
    crate::methods::groups(p)
        .into_iter()
        .filter(|g| g.name != "GLOBAL")
        .filter_map(|g| {
            let nodes: Vec<_> = g
                .members
                .iter()
                .filter(|m| {
                    !matches!(
                        m.name.as_str(),
                        "DIRECT" | "REJECT" | "REJECT-DROP" | "PASS" | "COMPATIBLE"
                    )
                })
                .collect();
            // Never tested is unknown, not dead.
            let dead = !nodes.is_empty() && nodes.iter().all(|m| m.delay == Some(0));
            dead.then(|| (g.name.clone(), nodes.len()))
        })
        .collect()
}

pub async fn watch_groups(state: Arc<State>) {
    // `None` until the first look: groups already dead when the daemon
    // starts are not news on every restart — unless they are strict, where
    // every minute of a silent outage matters.
    let mut known: Option<BTreeSet<String>> = None;
    let mut tick = tokio::time::interval(GROUPS_EVERY);
    loop {
        tick.tick().await;
        // Health checks run while offline fail everywhere; wait for fresh ones.
        if !state.core_running() || state.network_quiet() {
            continue;
        }
        let Ok(p) = state.core.proxies().await else {
            continue;
        };
        let strict = state.autofix.lock().unwrap().settings.strict_groups.clone();
        let dead = dead_groups(&p);
        let names: BTreeSet<String> = dead.iter().map(|(n, _)| n.clone()).collect();
        let first = known.is_none();
        let before = known.replace(names.clone()).unwrap_or_default();
        let fresh: Vec<_> = dead
            .iter()
            .filter(|(n, _)| !before.contains(n) && (!first || strict.contains(n)))
            .collect();
        if !fresh.is_empty() {
            let rules = state
                .config
                .rules()
                .await
                .map(|(r, _)| r)
                .unwrap_or_default();
            let conns = state
                .core
                .connections()
                .await
                .map(|c| c.connections)
                .unwrap_or_default();
            for (group, nodes) in fresh {
                if strict.contains(group) {
                    state.notify(&format!("group:{group}"), "strict_down", "critical", json!({ "group": group, "nodes": nodes, "hosts": failing_hosts(&state, group) }));
                    continue;
                }
                let n_rules = rules
                    .iter()
                    .filter(|s| s.rule.enabled && &s.rule.target == group)
                    .count();
                let n_conns = conns
                    .iter()
                    .filter(|c| c.chains.iter().any(|x| x == group))
                    .count();
                state.notify(&format!("group:{group}"), "dead_group", "normal", json!({ "group": group, "nodes": nodes, "rules": n_rules, "connections": n_conns }));
            }
        }
        for gone in before.difference(&names) {
            let key = format!("group:{gone}");
            match state.notice(&key) {
                Some(n) if n.kind == "dead_group" => {
                    state.notify(&key, "group_alive", "low", json!({ "group": gone }))
                }
                Some(n) if n.kind == "strict_down" => {
                    let node = p.proxies.get(gone).and_then(|g| g.now.clone());
                    let delay = node
                        .as_deref()
                        .and_then(|n| p.proxies.get(n))
                        .and_then(|n| n.last_delay());
                    state.notify(&key, "strict_up", "low", json!({ "group": gone, "down_ms": crate::problems::now_ms() - n.at, "node": node, "delay": delay }));
                }
                _ => {}
            }
        }
    }
}

/// Hosts failing through `group` in the last half hour, busiest first.
fn failing_hosts(state: &State, group: &str) -> Vec<serde_json::Value> {
    let since = crate::problems::now_ms() - 30 * 60_000;
    let c = state.problems.lock().unwrap();
    let q = omm_problems::Query {
        target: Some(group.to_owned()),
        since: Some(since),
        ..Default::default()
    };
    let mut items: Vec<_> = c.list(&q).into_iter().map(|v| v.problem.clone()).collect();
    items.sort_by_key(|p| std::cmp::Reverse(p.count));
    items
        .iter()
        .take(8)
        .map(|p| json!({ "host": p.host, "count": p.count, "app": p.apps.iter().max_by_key(|(_, n)| **n).map(|(a, _)| a.clone()) }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_groups() {
        let groups = vec!["DIRECT".to_owned(), "TELEGRAM".to_owned(), "AI".to_owned(), "Streaming EU".to_owned()];
        assert_eq!(replacement("TELEGRAM[DE]", &groups).as_deref(), Some("TELEGRAM"));
        assert_eq!(replacement("streaming", &groups).as_deref(), Some("Streaming EU"));
        assert_eq!(replacement("GONE", &groups), None);
    }

    #[test]
    fn only_fully_failed_groups_are_dead() {
        let p: mihomo_api::model::Proxies = serde_json::from_value(json!({ "proxies": {
            "AI": { "name": "AI", "type": "Selector", "all": ["a", "b"] },
            "FAST": { "name": "FAST", "type": "URLTest", "all": ["a", "c"] },
            "NEW": { "name": "NEW", "type": "Selector", "all": ["d"] },
            "a": { "name": "a", "type": "Vless", "history": [{ "delay": 0 }] },
            "b": { "name": "b", "type": "Vless", "history": [{ "delay": 0 }] },
            "c": { "name": "c", "type": "Vless", "history": [{ "delay": 120 }] },
            "d": { "name": "d", "type": "Vless" }
        }}))
        .unwrap();
        assert_eq!(dead_groups(&p), vec![("AI".to_owned(), 2)]);
    }
}
