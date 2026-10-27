//! Node latency over time. The core keeps only a node's last few delay
//! tests; every minute the daemon folds the new ones into 5-minute buckets
//! kept for a week, so a node's quality is judged by more than one probe.
//! Nodes the core never tests on its own (members of selector groups
//! only) are tested every 15 minutes, unless switched off.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use futures_util::stream::{self, StreamExt};
use mihomo_api::model::Proxies;
use rusqlite::{Connection as Db, params};
use serde::{Deserialize, Serialize};

use crate::problems::now_ms;
use crate::state::State;

const POLL: Duration = Duration::from_secs(60);
const BUCKET: i64 = 5 * 60_000;
const KEEP: i64 = 7 * 24 * 3_600_000;
/// A node without a test this long gets one.
const PROBE_AFTER: i64 = 15 * 60_000;
const PROBE_CONCURRENCY: usize = 4;
/// Outbounds that are not nodes.
const BUILTIN: &[&str] = &["Direct", "Reject", "RejectDrop", "Pass", "PassRule", "Compatible", "Dns"];

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Acc {
    tests: u32,
    failures: u32,
    sum: u64,
    min: u32,
    max: u32,
}

impl Acc {
    fn add(&mut self, delay: u32) {
        self.tests += 1;
        if delay == 0 {
            self.failures += 1;
            return;
        }
        self.sum += u64::from(delay);
        self.min = if self.min == 0 { delay } else { self.min.min(delay) };
        self.max = self.max.max(delay);
    }
}

pub struct Store {
    db: Db,
    /// Each node's newest history entry already counted, by its time.
    seen: HashMap<String, String>,
    primed: bool,
    /// Last time each node was tested, by us or the core.
    tested: HashMap<String, i64>,
}

/// One node over a window: totals and a series of evenly spaced points.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct NodeView {
    pub tests: u32,
    pub failures: u32,
    /// Mean delay of the successful tests, ms.
    pub avg: Option<u32>,
    pub min: Option<u32>,
    pub max: Option<u32>,
    /// Mean delay per point, `None` where nothing was measured; `0` where
    /// every test failed.
    pub points: Vec<Option<u32>>,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Query {
    pub hours: u32,
    pub points: u32,
    pub nodes: Option<Vec<String>>,
}

impl Default for Query {
    fn default() -> Self {
        Self { hours: 24, points: 24, nodes: None }
    }
}

fn is_node(p: &mihomo_api::model::Proxy) -> bool {
    !p.is_group() && !BUILTIN.contains(&p.kind.as_str())
}

impl Store {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        Self::init(Db::open(path)?)
    }

    pub fn in_memory() -> Self {
        Self::init(Db::open_in_memory().expect("in-memory sqlite")).expect("schema")
    }

    fn init(db: Db) -> rusqlite::Result<Self> {
        db.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS delays (
                 node TEXT NOT NULL, bucket INTEGER NOT NULL,
                 tests INTEGER NOT NULL, failures INTEGER NOT NULL,
                 sum_ms INTEGER NOT NULL, min_ms INTEGER NOT NULL, max_ms INTEGER NOT NULL,
                 PRIMARY KEY (node, bucket)
             ) WITHOUT ROWID;",
        )?;
        Ok(Self { db, seen: HashMap::new(), primed: false, tested: HashMap::new() })
    }

    /// Count the history entries that appeared since the last look. The
    /// first look only remembers where each node's history ends: what was
    /// there before the daemon started is already counted or lost.
    pub fn ingest(&mut self, proxies: &Proxies, now: i64) -> rusqlite::Result<()> {
        let mut fresh: HashMap<&str, Acc> = HashMap::new();
        for p in proxies.proxies.values().filter(|p| is_node(p)) {
            let Some(last) = p.history.last() else { continue };
            let seen = self.seen.get(&p.name);
            if self.primed {
                // RFC 3339 times of one clock sort as text.
                for h in p.history.iter().filter(|h| seen.is_none_or(|s| h.time.as_str() > s.as_str())) {
                    fresh.entry(&p.name).or_default().add(h.delay);
                }
            }
            if seen != Some(&last.time) {
                self.tested.insert(p.name.clone(), now);
                self.seen.insert(p.name.clone(), last.time.clone());
            }
        }
        self.primed = true;
        let bucket = now - now.rem_euclid(BUCKET);
        let tx = self.db.transaction()?;
        for (node, a) in fresh {
            tx.execute(
                "INSERT INTO delays (node, bucket, tests, failures, sum_ms, min_ms, max_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT (node, bucket) DO UPDATE SET tests = tests + ?3, failures = failures + ?4, sum_ms = sum_ms + ?5,
                   min_ms = CASE WHEN min_ms = 0 THEN ?6 WHEN ?6 = 0 THEN min_ms ELSE MIN(min_ms, ?6) END, max_ms = MAX(max_ms, ?7)",
                params![node, bucket, a.tests, a.failures, a.sum as i64, a.min, a.max],
            )?;
        }
        tx.execute("DELETE FROM delays WHERE bucket < ?1", params![now - KEEP])?;
        tx.commit()
    }

    /// Nodes nobody tested for a while.
    fn untested(&self, proxies: &Proxies, now: i64) -> Vec<String> {
        proxies
            .proxies
            .values()
            .filter(|p| is_node(p) && self.tested.get(&p.name).is_none_or(|t| now - t >= PROBE_AFTER))
            .map(|p| p.name.clone())
            .collect()
    }

    pub fn query(&self, q: &Query, now: i64) -> rusqlite::Result<BTreeMap<String, NodeView>> {
        let hours = q.hours.clamp(1, 24 * 7);
        let points = q.points.clamp(1, 168) as i64;
        let from = now - i64::from(hours) * 3_600_000;
        let step = ((now - from) / points).max(BUCKET);
        let mut stmt = self.db.prepare_cached("SELECT node, bucket, tests, failures, sum_ms, min_ms, max_ms FROM delays WHERE bucket >= ?1 ORDER BY node, bucket")?;
        let rows = stmt.query_map(params![from - from.rem_euclid(BUCKET)], |r| {
            let acc = Acc { tests: r.get(2)?, failures: r.get(3)?, sum: r.get::<_, i64>(4)? as u64, min: r.get(5)?, max: r.get(6)? };
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, acc))
        })?;
        let mut out: BTreeMap<String, (Acc, Vec<Acc>)> = BTreeMap::new();
        for row in rows {
            let (node, bucket, a) = row?;
            if q.nodes.as_ref().is_some_and(|n| !n.contains(&node)) {
                continue;
            }
            let (total, series) = out.entry(node).or_insert_with(|| (Acc::default(), vec![Acc::default(); points as usize]));
            let i = (((bucket - from).max(0)) / step).min(points - 1) as usize;
            for s in [total, &mut series[i]] {
                s.tests += a.tests;
                s.failures += a.failures;
                s.sum += a.sum;
                if a.min > 0 {
                    s.min = if s.min == 0 { a.min } else { s.min.min(a.min) };
                }
                s.max = s.max.max(a.max);
            }
        }
        let mean = |a: &Acc| (a.tests > a.failures).then(|| (a.sum / u64::from(a.tests - a.failures)) as u32);
        Ok(out
            .into_iter()
            .map(|(node, (t, series))| {
                let points = series.iter().map(|a| if a.tests == 0 { None } else { Some(mean(a).unwrap_or(0)) }).collect();
                let view = NodeView { tests: t.tests, failures: t.failures, avg: mean(&t), min: (t.min > 0).then_some(t.min), max: (t.max > 0).then_some(t.max), points };
                (node, view)
            })
            .collect())
    }
}

pub async fn collect(state: Arc<State>) {
    let mut status = state.watch_status();
    let mut tick = tokio::time::interval(POLL);
    loop {
        tick.tick().await;
        if !state.core_running() {
            let _ = status.changed().await;
            continue;
        }
        let Ok(proxies) = state.core.proxies().await else { continue };
        let now = now_ms();
        // Offline, every test fails and says nothing about the node.
        let quiet = state.network_quiet();
        let untested = {
            let mut s = state.latency.lock().unwrap();
            if quiet {
                s.primed = false;
            } else if let Err(e) = s.ingest(&proxies, now) {
                tracing::warn!("saving node delays: {e}");
            }
            s.untested(&proxies, now)
        };
        let probe = state.autofix.lock().unwrap().settings.probe_nodes;
        if quiet || !probe || untested.is_empty() {
            continue;
        }
        // The core records these tests in each node's history; the next
        // look counts them.
        stream::iter(untested)
            .for_each_concurrent(PROBE_CONCURRENCY, |name| {
                let state = state.clone();
                async move {
                    let _ = state.core.proxy_delay(&name, crate::methods::DEFAULT_TEST_URL, crate::methods::DEFAULT_TEST_TIMEOUT_MS).await;
                    state.latency.lock().unwrap().tested.insert(name, now_ms());
                }
            })
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn proxies(history: &[(&str, &[(&str, u32)])]) -> Proxies {
        let mut map = serde_json::Map::new();
        map.insert("AI".into(), json!({ "name": "AI", "type": "Selector", "all": ["FI"], "history": [{ "time": "t9", "delay": 5 }] }));
        map.insert("DIRECT".into(), json!({ "name": "DIRECT", "type": "Direct", "history": [{ "time": "t9", "delay": 1 }] }));
        for (name, h) in history {
            let h: Vec<_> = h.iter().map(|(t, d)| json!({ "time": t, "delay": d })).collect();
            map.insert((*name).into(), json!({ "name": name, "type": "Vless", "history": h }));
        }
        serde_json::from_value(json!({ "proxies": map })).unwrap()
    }

    #[test]
    fn counts_new_history_once_and_skips_groups() {
        let mut s = Store::in_memory();
        s.ingest(&proxies(&[("FI", &[("t1", 40)])]), 0).unwrap();
        s.ingest(&proxies(&[("FI", &[("t1", 40), ("t2", 60), ("t3", 0)])]), 60_000).unwrap();
        s.ingest(&proxies(&[("FI", &[("t2", 60), ("t3", 0)])]), 120_000).unwrap();
        let v = s.query(&Query { hours: 1, points: 4, nodes: None }, 3_600_000).unwrap();
        assert_eq!(v.keys().collect::<Vec<_>>(), vec!["FI"], "groups and DIRECT are not nodes");
        let fi = &v["FI"];
        assert_eq!((fi.tests, fi.failures, fi.avg, fi.min, fi.max), (2, 1, Some(60), Some(60), Some(60)), "the entry seen at start is not counted");
        assert_eq!(fi.points, vec![Some(60), None, None, None]);
    }

    #[test]
    fn failed_buckets_show_zero_and_old_ones_go() {
        let mut s = Store::in_memory();
        s.ingest(&proxies(&[("KZ", &[])]), 0).unwrap();
        s.ingest(&proxies(&[("KZ", &[("t1", 0)])]), 10 * BUCKET).unwrap();
        let v = s.query(&Query { hours: 1, points: 12, nodes: Some(vec!["KZ".into()]) }, 12 * BUCKET).unwrap();
        assert_eq!(v["KZ"].points[10], Some(0));
        assert_eq!(v["KZ"].avg, None);
        s.ingest(&proxies(&[("KZ", &[("t1", 0)])]), KEEP + 11 * BUCKET).unwrap();
        assert!(s.query(&Query::default(), KEEP + 11 * BUCKET).unwrap().is_empty());
    }

    #[test]
    fn untested_nodes_get_a_probe() {
        let mut s = Store::in_memory();
        let p = proxies(&[("FI", &[("t1", 40)]), ("KZ", &[])]);
        s.ingest(&p, 0).unwrap();
        assert_eq!(s.untested(&p, 1_000), vec!["KZ".to_owned()]);
        assert_eq!(s.untested(&p, PROBE_AFTER).len(), 2);
    }
}
