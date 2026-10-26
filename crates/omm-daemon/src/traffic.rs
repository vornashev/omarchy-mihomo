//! Traffic history: who downloaded and uploaded how much, and through what.
//!
//! The core only reports running totals per open connection. Polling
//! `/connections` and diffing by id turns that into bytes per minute,
//! attributed to the app, host, address, route and rule of the connection.
//! Minutes are kept for two days and rolled into hours kept for 30 days.

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use mihomo_api::model::Connection;
use rusqlite::types::Value as Sql;
use rusqlite::{Connection as Db, params, params_from_iter};
use serde::{Deserialize, Serialize};

use crate::mmdb;
use crate::state::State;

const POLL: Duration = Duration::from_secs(2);
/// How often to look for unusual traffic.
const CHECK_EVERY: i64 = 5 * 60_000;
pub const MINUTE: i64 = 60_000;
pub const HOUR: i64 = 3_600_000;
pub const DAY: i64 = 24 * HOUR;
const KEEP_MINUTES: i64 = 48 * HOUR;
const KEEP_HOURS: i64 = 30 * 24 * HOUR;
/// Ranges starting later than this ago are served from minutes.
const MINUTE_RANGE: i64 = 47 * HOUR;
/// Per minute, keys that moved less than this fold into one row per app
/// and route, so background chatter does not grow the database.
const SMALL: u64 = 8 * 1024;
const COUNTRY_URL: &str = "https://github.com/MetaCubeX/meta-rules-dat/releases/download/latest/country.mmdb";
const COUNTRY_FILE: &str = "traffic-country.mmdb";
const GEO_CACHE: usize = 8192;
/// An app moving this much in an hour, five times its usual, is worth a notice.
const SPIKE_MIN: u64 = 512 * 1024 * 1024;
const SPIKE_FACTOR: u64 = 5;
/// An app that used to go direct and now sends this much through a proxy.
const ROUTE_MIN: u64 = 100 * 1024 * 1024;
/// Baselines need this much history.
const HISTORY_MIN: i64 = 24 * HOUR;
/// One notice per app and kind in this long.
const ALERT_EVERY: i64 = 6 * HOUR;

/// App name for root's connections, whose processes we cannot see.
pub const ROOT_APP: &str = "[root]";

/// Columns a query can group or filter by, as `(name, column)`.
const DIMS: &[(&str, &str)] = &[
    ("app", "app"), ("domain", "domain"), ("host", "host"), ("ip", "ip"), ("country", "country"),
    ("asn", "asn"), ("outbound", "outbound"), ("group", "grp"), ("rule", "rule"), ("network", "network"),
];
const COLUMNS: &str = "app, host, domain, ip, asn, org, country, outbound, grp, rule, network";

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
struct Key {
    app: String,
    host: String,
    domain: String,
    ip: String,
    asn: u32,
    org: String,
    country: String,
    outbound: String,
    grp: String,
    rule: String,
    network: String,
}

#[derive(Debug, Clone, Copy, Default)]
struct Acc {
    up: u64,
    down: u64,
    conns: u64,
}

impl Acc {
    fn add(&mut self, o: Acc) {
        self.up += o.up;
        self.down += o.down;
        self.conns += o.conns;
    }
}

#[derive(Default)]
struct Geo {
    asn: Option<mmdb::Reader>,
    country: Option<mmdb::Reader>,
    cache: HashMap<IpAddr, (u32, String, String)>,
}

impl Geo {
    fn load(core_home: &Path, root: &Path) -> Self {
        let asn = find(core_home, &["asn.mmdb", "geoasn.mmdb"]).and_then(|p| mmdb::Reader::open(&p));
        let country = find(core_home, &["country.mmdb", "geoip.metadb"])
            .or_else(|| find(root, &[COUNTRY_FILE]))
            .and_then(|p| mmdb::Reader::open(&p));
        Self { asn, country, cache: HashMap::new() }
    }

    fn lookup(&mut self, ip: &str) -> (u32, String, String) {
        let Ok(addr) = ip.parse::<IpAddr>() else { return Default::default() };
        if let Some(hit) = self.cache.get(&addr) {
            return hit.clone();
        }
        let (asn, org) = self.asn.as_ref().and_then(|r| r.lookup(addr)).and_then(|v| mmdb::asn(&v)).unwrap_or_default();
        let country = self.country.as_ref().and_then(|r| r.lookup(addr)).and_then(|v| mmdb::country(&v)).unwrap_or_default();
        if self.cache.len() >= GEO_CACHE {
            self.cache.clear();
        }
        let hit = (asn, org, country);
        self.cache.insert(addr, hit.clone());
        hit
    }
}

/// A system user's name, for services the core could not name.
fn user_name(uid: u32) -> Option<String> {
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
    passwd.lines().find_map(|l| {
        let mut f = l.split(':');
        let name = f.next()?;
        (f.nth(1)?.parse::<u32>().ok()? == uid).then(|| name.to_owned())
    })
}

/// A file in `dir` whose name matches one of `names`, any letter case.
fn find(dir: &Path, names: &[&str]) -> Option<PathBuf> {
    let entries: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).collect();
    names.iter().find_map(|n| entries.iter().find(|p| p.file_name().and_then(|f| f.to_str()).is_some_and(|f| f.eq_ignore_ascii_case(n))).cloned())
}

pub struct Store {
    db: Db,
    root: PathBuf,
    core_home: PathBuf,
    /// Totals per open connection at the previous poll.
    prev: HashMap<String, (u64, u64)>,
    /// The first poll after start only sets the baseline: the previous run
    /// already counted what those connections moved.
    primed: bool,
    /// Apps found for connections the core could not name, by id.
    owners: HashMap<String, String>,
    /// Where `/proc` is; tests point it elsewhere.
    proc_root: PathBuf,
    /// Last notice per `(kind, app)`.
    alerted: HashMap<(&'static str, String), i64>,
    bucket: i64,
    pending: HashMap<Key, Acc>,
    geo: Geo,
    pub enabled: bool,
}

impl Store {
    pub fn open(path: &Path, core_home: &Path) -> rusqlite::Result<Self> {
        Self::with(Db::open(path)?, path.parent().unwrap_or(Path::new(".")), core_home)
    }

    pub fn in_memory() -> Self {
        Self::with(Db::open_in_memory().expect("in-memory sqlite"), Path::new("."), Path::new(".")).expect("traffic schema")
    }

    fn with(db: Db, root: &Path, core_home: &Path) -> rusqlite::Result<Self> {
        db.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS samples (
                 res INTEGER NOT NULL, bucket INTEGER NOT NULL,
                 app TEXT NOT NULL, host TEXT NOT NULL, domain TEXT NOT NULL, ip TEXT NOT NULL,
                 asn INTEGER NOT NULL, org TEXT NOT NULL, country TEXT NOT NULL,
                 outbound TEXT NOT NULL, grp TEXT NOT NULL, rule TEXT NOT NULL, network TEXT NOT NULL,
                 up INTEGER NOT NULL, down INTEGER NOT NULL, conns INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS samples_time ON samples (res, bucket);
             CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TEMP TABLE live AS SELECT * FROM samples WHERE 0;",
        )?;
        let enabled = meta(&db, "enabled").is_none_or(|v| v != "0");
        let geo = Geo::load(core_home, root);
        Ok(Self { db, root: root.to_owned(), core_home: core_home.to_owned(), prev: HashMap::new(), primed: false, owners: HashMap::new(), proc_root: PathBuf::from("/proc"), alerted: HashMap::new(), bucket: 0, pending: HashMap::new(), geo, enabled })
    }

    pub fn has_country(&self) -> bool {
        self.geo.country.is_some()
    }

    /// Our own country database is in use and a month old.
    pub fn own_country_stale(&self) -> bool {
        if find(&self.core_home, &["country.mmdb", "geoip.metadb"]).is_some() {
            return false;
        }
        let modified = std::fs::metadata(self.root.join(COUNTRY_FILE)).and_then(|m| m.modified()).ok();
        let age = modified.and_then(|t| t.elapsed().ok()).map_or(0, |d| d.as_millis() as i64);
        modified.is_some() && age > 30 * 24 * HOUR
    }

    pub fn reload_geo(&mut self) {
        self.geo = Geo::load(&self.core_home, &self.root);
    }

    /// Diff the core's connection totals against the previous poll.
    pub fn ingest(&mut self, conns: &[Connection], now: i64) {
        let bucket = now - now.rem_euclid(MINUTE);
        if bucket != self.bucket {
            if let Err(e) = self.flush() {
                tracing::warn!("saving traffic: {e}");
            }
            let new_hour = self.bucket.div_euclid(HOUR) != bucket.div_euclid(HOUR);
            self.bucket = bucket;
            if new_hour {
                if let Err(e) = self.maintain(bucket) {
                    tracing::warn!("rolling up traffic: {e}");
                }
            }
        }
        if self.enabled {
            self.resolve_owners(conns);
        }
        let mut next = HashMap::with_capacity(conns.len());
        for c in conns {
            next.insert(c.id.clone(), (c.upload, c.download));
            if !self.primed || !self.enabled {
                continue;
            }
            let (acc, is_new) = match self.prev.get(&c.id) {
                Some(&(pu, pd)) => {
                    // Totals only grow; a smaller one means a reused id.
                    let up = if c.upload >= pu { c.upload - pu } else { c.upload };
                    let down = if c.download >= pd { c.download - pd } else { c.download };
                    (Acc { up, down, conns: 0 }, false)
                }
                None => (Acc { up: c.upload, down: c.download, conns: 1 }, true),
            };
            if !is_new && acc.up == 0 && acc.down == 0 {
                continue;
            }
            let key = self.key(c);
            self.pending.entry(key).or_default().add(acc);
        }
        self.owners.retain(|id, _| next.contains_key(id));
        self.prev = next;
        self.primed = true;
    }

    /// Name the app behind connections the core left unnamed: its own
    /// user's programs by their sockets, root's as `[root]`.
    fn resolve_owners(&mut self, conns: &[Connection]) {
        let unnamed: Vec<&Connection> = conns.iter().filter(|c| c.metadata.process.is_empty() && !self.owners.contains_key(&c.id)).collect();
        if unnamed.is_empty() {
            return;
        }
        let socket = |c: &Connection| Some((c.metadata.network == "udp", c.metadata.source_port.parse::<u16>().ok()?));
        let wanted: Vec<(bool, u16)> = unnamed.iter().filter_map(|c| socket(c)).collect();
        let found = crate::procs::socket_owners(&self.proc_root, &wanted);
        for c in unnamed {
            let app = socket(c).and_then(|k| found.get(&k)).map(|o| match (&o.name, o.uid) {
                (Some(name), _) => name.clone(),
                (None, 0) => ROOT_APP.to_owned(),
                (None, uid) => format!("[{}]", user_name(uid).unwrap_or_else(|| format!("uid {uid}"))),
            });
            self.owners.insert(c.id.clone(), app.unwrap_or_default());
        }
    }

    fn key(&mut self, c: &Connection) -> Key {
        let m = &c.metadata;
        let host = [&m.host, &m.sniff_host].into_iter().find(|s| !s.is_empty()).map(|s| s.trim_end_matches('.').to_ascii_lowercase()).unwrap_or_default();
        let host = if host.parse::<IpAddr>().is_ok() { String::new() } else { host };
        let (asn, org, country) = self.geo.lookup(&m.destination_ip);
        // `/connections` says ProcessName where `/rules` says Process.
        let kind = if c.rule == "ProcessName" { "Process" } else { c.rule.as_str() };
        let rule = if c.rule_payload.is_empty() { kind.to_owned() } else { format!("{kind},{}", c.rule_payload) };
        Key {
            app: if m.process.is_empty() { self.owners.get(&c.id).cloned().unwrap_or_default() } else { m.process.trim_end_matches(" (deleted)").to_owned() },
            domain: registrable(&host),
            host,
            ip: m.destination_ip.clone(),
            asn,
            org,
            country,
            outbound: c.chains.first().cloned().unwrap_or_else(|| "DIRECT".into()),
            grp: c.chains.last().cloned().unwrap_or_else(|| "DIRECT".into()),
            rule,
            network: m.network.clone(),
        }
    }

    /// Write the current minute; small keys fold into one per app and route.
    pub fn flush(&mut self) -> rusqlite::Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let rows = fold_small(std::mem::take(&mut self.pending));
        let tx = self.db.transaction()?;
        {
            let mut ins = tx.prepare_cached(&format!("INSERT INTO samples (res, bucket, {COLUMNS}, up, down, conns) VALUES (60, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)"))?;
            for (k, a) in &rows {
                ins.execute(params![self.bucket, k.app, k.host, k.domain, k.ip, k.asn, k.org, k.country, k.outbound, k.grp, k.rule, k.network, a.up as i64, a.down as i64, a.conns as i64])?;
            }
        }
        tx.commit()
    }

    /// Roll finished hours into hourly rows and drop what is too old.
    pub fn maintain(&mut self, now: i64) -> rusqlite::Result<()> {
        let hour = now - now.rem_euclid(HOUR);
        let rolled: i64 = meta(&self.db, "rolled").and_then(|v| v.parse().ok()).unwrap_or(0);
        let tx = self.db.transaction()?;
        tx.execute(
            &format!(
                "INSERT INTO samples (res, bucket, {COLUMNS}, up, down, conns)
                 SELECT 3600, bucket - bucket % {HOUR}, {COLUMNS}, SUM(up), SUM(down), SUM(conns)
                 FROM samples WHERE res = 60 AND bucket >= ?1 AND bucket < ?2
                 GROUP BY bucket - bucket % {HOUR}, {COLUMNS}"
            ),
            params![rolled, hour],
        )?;
        tx.execute("INSERT OR REPLACE INTO meta (key, value) VALUES ('rolled', ?1)", params![hour.to_string()])?;
        tx.execute("DELETE FROM samples WHERE res = 60 AND bucket < ?1", params![now - KEEP_MINUTES])?;
        tx.execute("DELETE FROM samples WHERE res = 3600 AND bucket < ?1", params![now - KEEP_HOURS])?;
        tx.commit()?;
        self.reload_geo();
        Ok(())
    }

    pub fn set_enabled(&mut self, enabled: bool) -> rusqlite::Result<()> {
        self.enabled = enabled;
        self.db.execute("INSERT OR REPLACE INTO meta (key, value) VALUES ('enabled', ?1)", params![if enabled { "1" } else { "0" }])?;
        Ok(())
    }

    pub fn clear(&mut self) -> rusqlite::Result<()> {
        self.pending.clear();
        self.db.execute_batch("DELETE FROM samples; DELETE FROM meta WHERE key = 'rolled';")?;
        Ok(())
    }

    pub fn status(&self) -> rusqlite::Result<StatusView> {
        let (rows, since): (i64, Option<i64>) = self.db.query_row("SELECT COUNT(*), MIN(bucket) FROM samples", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let pages: i64 = self.db.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let page: i64 = self.db.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        Ok(StatusView { enabled: self.enabled, rows, since, bytes: pages * page, asn: self.geo.asn.is_some(), country: self.geo.country.is_some() })
    }

    pub fn query(&mut self, q: &Query, now: i64) -> rusqlite::Result<QueryView> {
        let col = column(&q.by).unwrap_or("app");
        let to = q.to.unwrap_or(now).min(now + MINUTE);
        let from = q.from.min(to - MINUTE);
        let minutes = from >= now - MINUTE_RANGE;
        let res = if minutes { MINUTE } else { HOUR };
        let step = q.step.filter(|s| *s >= res && *s % res == 0).unwrap_or_else(|| pick_step(to - from, res));
        let start = from - from.rem_euclid(step);
        let count = ((to - start + step - 1) / step).max(1) as usize;
        self.refresh_live()?;

        let (where_sql, args) = self.condition(q, minutes, start, to);
        // Totals per key.
        let label = if col == "asn" { "MAX(org)" } else { "''" };
        let mut stmt = self.db.prepare(&format!(
            "SELECT {col}, SUM(up), SUM(down), SUM(conns), {label}, MAX(org), MAX(country), MAX(app), COUNT(DISTINCT host)
             FROM ({}) WHERE {where_sql} GROUP BY {col} ORDER BY SUM(up) + SUM(down) DESC",
            source(minutes)
        ))?;
        let mut keys: Vec<RowView> = stmt
            .query_map(params_from_iter(args.iter()), |r| {
                let key: Sql = r.get(0)?;
                Ok(RowView {
                    key: sql_text(key),
                    label: r.get(4)?,
                    up: r.get::<_, i64>(1)? as u64,
                    down: r.get::<_, i64>(2)? as u64,
                    conns: r.get::<_, i64>(3)? as u64,
                    share: 0.0,
                    org: r.get(5)?,
                    country: r.get(6)?,
                    app: r.get(7)?,
                    hosts: r.get::<_, i64>(8)? as u64,
                    spark: Vec::new(),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        drop(stmt);
        let mut total = Acc::default();
        for k in &keys {
            total.add(Acc { up: k.up, down: k.down, conns: k.conns });
        }
        let sum = (total.up + total.down).max(1) as f64;
        for k in &mut keys {
            k.share = (k.up + k.down) as f64 / sum;
        }
        let limit = q.limit.unwrap_or(6);
        let rows_limit = q.rows.unwrap_or(50);
        keys.truncate(rows_limit.max(limit));

        // Per bucket, per key.
        let index: HashMap<&str, usize> = keys.iter().enumerate().map(|(i, k)| (k.key.as_str(), i)).collect();
        let mut per_key = vec![(vec![0u64; count], vec![0u64; count]); keys.len()];
        let mut other = (vec![0u64; count], vec![0u64; count]);
        let mut all = (vec![0u64; count], vec![0u64; count]);
        let mut stmt = self.db.prepare(&format!(
            "SELECT (bucket - ?{n}) / ?{m}, {col}, SUM(up), SUM(down) FROM ({}) WHERE {where_sql} GROUP BY 1, 2",
            source(minutes),
            n = args.len() + 1,
            m = args.len() + 2
        ))?;
        let mut a2 = args.clone();
        a2.push(Sql::Integer(start));
        a2.push(Sql::Integer(step));
        let mut rows = stmt.query(params_from_iter(a2.iter()))?;
        while let Some(r) = rows.next()? {
            let i = r.get::<_, i64>(0)?;
            if i < 0 || i as usize >= count {
                continue;
            }
            let i = i as usize;
            let key = sql_text(r.get(1)?);
            let (up, down) = (r.get::<_, i64>(2)? as u64, r.get::<_, i64>(3)? as u64);
            all.0[i] += up;
            all.1[i] += down;
            match index.get(key.as_str()) {
                Some(&k) => {
                    per_key[k].0[i] += up;
                    per_key[k].1[i] += down;
                    if k >= limit {
                        other.0[i] += up;
                        other.1[i] += down;
                    }
                }
                None => {
                    other.0[i] += up;
                    other.1[i] += down;
                }
            }
        }
        drop(rows);
        drop(stmt);
        let series = keys.iter().zip(&per_key).take(limit).map(|(k, s)| SeriesView { key: k.key.clone(), label: k.label.clone(), up: s.0.clone(), down: s.1.clone() }).collect();
        for (k, s) in keys.iter_mut().zip(&per_key) {
            k.spark = spark(&s.0, &s.1, 24);
        }
        let (peak_at, peak) = all.1.iter().enumerate().max_by_key(|(_, v)| **v).map(|(i, v)| (start + i as i64 * step, *v * 1000 / step as u64)).unwrap_or((start, 0));

        // The same span just before, for "vs yesterday".
        let span = to - from;
        let (prev_where, prev_args) = self.condition(q, from - span >= now - MINUTE_RANGE, from - span, from);
        let previous: (i64, i64) = self.db.query_row(
            &format!("SELECT COALESCE(SUM(up), 0), COALESCE(SUM(down), 0) FROM ({}) WHERE {prev_where}", source(from - span >= now - MINUTE_RANGE)),
            params_from_iter(prev_args.iter()),
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let (apps, hosts): (i64, i64) = self.db.query_row(
            &format!("SELECT COUNT(DISTINCT app), COUNT(DISTINCT host) FROM ({}) WHERE {where_sql}", source(minutes)),
            params_from_iter(args.iter()),
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let since: Option<i64> = self.db.query_row("SELECT MIN(bucket) FROM samples", [], |r| r.get(0))?;

        Ok(QueryView {
            by: q.by.clone(),
            from: start,
            to,
            step,
            series,
            other: SeriesView { key: String::new(), label: String::new(), up: other.0, down: other.1 },
            all: SeriesView { key: String::new(), label: String::new(), up: all.0, down: all.1 },
            rows: keys.into_iter().take(rows_limit).collect(),
            total: TotalView { up: total.up, down: total.down, conns: total.conns, apps: apps as u64, hosts: hosts as u64 },
            previous: TotalView { up: previous.0 as u64, down: previous.1 as u64, ..Default::default() },
            peak,
            peak_at,
            since,
        })
    }

    /// Bytes per key of one dimension since `from`; rules by lowercase
    /// `Type,payload`, matching the core's `/rules`.
    pub fn totals(&mut self, dim: &str, from: i64, now: i64) -> rusqlite::Result<HashMap<String, TotalView>> {
        let Some(col) = column(dim) else { return Ok(HashMap::new()) };
        self.refresh_live()?;
        let minutes = from >= now - MINUTE_RANGE;
        let (where_sql, args) = self.condition(&Query::default(), minutes, from, now + MINUTE);
        let key = if col == "rule" { "lower(rule)".to_owned() } else { col.to_owned() };
        let mut stmt = self.db.prepare(&format!("SELECT {key}, SUM(up), SUM(down), SUM(conns) FROM ({}) WHERE {where_sql} GROUP BY 1", source(minutes)))?;
        let rows = stmt.query_map(params_from_iter(args.iter()), |r| {
            Ok((sql_text(r.get(0)?), TotalView { up: r.get::<_, i64>(1)? as u64, down: r.get::<_, i64>(2)? as u64, conns: r.get::<_, i64>(3)? as u64, ..Default::default() }))
        })?;
        rows.collect()
    }

    /// Apps that moved far more than usual in the last hour, or started
    /// going through a proxy after going direct.
    pub fn anomalies(&mut self, now: i64) -> rusqlite::Result<Vec<Anomaly>> {
        let since: Option<i64> = self.db.query_row("SELECT MIN(bucket) FROM samples", [], |r| r.get(0))?;
        let Some(since) = since.filter(|s| now - s >= HISTORY_MIN) else { return Ok(Vec::new()) };
        self.refresh_live()?;
        let recent = self.per_app(now - HOUR, now + MINUTE, now)?;
        let base_from = (now - 7 * DAY - HOUR).max(since);
        let base = self.per_app(base_from, now - HOUR, now)?;
        let hours = ((now - HOUR - base_from) / HOUR).max(1) as u64;
        let mut out = Vec::new();
        for (app, cur) in recent {
            if app.is_empty() || app.starts_with('[') {
                continue;
            }
            let usual = base.get(&app).copied().unwrap_or_default();
            let per_hour = usual.total / hours;
            let kind = if cur.total >= SPIKE_MIN && cur.total >= SPIKE_FACTOR * per_hour {
                "traffic_spike"
            } else if cur.proxied >= ROUTE_MIN && usual.total >= 2 * ROUTE_MIN && usual.proxied * 20 < usual.total {
                "traffic_route"
            } else {
                continue;
            };
            let key = (kind, app.clone());
            if self.alerted.get(&key).is_some_and(|t| now - t < ALERT_EVERY) {
                continue;
            }
            self.alerted.insert(key, now);
            let domains = self.top_of(&app, "domain", now)?;
            let groups = self.top_of(&app, "grp", now)?;
            out.push(Anomaly { kind, app, bytes: cur.total, proxied: cur.proxied, usual_per_hour: per_hour, new: usual.total == 0, domains, groups });
        }
        Ok(out)
    }

    fn per_app(&self, from: i64, to: i64, now: i64) -> rusqlite::Result<HashMap<String, AppTotal>> {
        let minutes = from >= now - MINUTE_RANGE;
        let (where_sql, args) = self.condition(&Query::default(), minutes, from, to);
        let mut stmt = self.db.prepare(&format!(
            "SELECT app, SUM(up + down), SUM(CASE WHEN grp = 'DIRECT' THEN 0 ELSE up + down END) FROM ({}) WHERE {where_sql} GROUP BY app",
            source(minutes)
        ))?;
        let rows = stmt.query_map(params_from_iter(args.iter()), |r| Ok((r.get::<_, String>(0)?, AppTotal { total: r.get::<_, i64>(1)? as u64, proxied: r.get::<_, i64>(2)? as u64 })))?;
        rows.collect()
    }

    /// The app's three biggest values of `col` in the last hour.
    fn top_of(&self, app: &str, col: &str, now: i64) -> rusqlite::Result<Vec<(String, u64)>> {
        let q = Query { filters: vec![Filter { dim: "app".into(), value: app.into(), exclude: false }], ..Default::default() };
        let (where_sql, args) = self.condition(&q, true, now - HOUR, now + MINUTE);
        let mut stmt = self.db.prepare(&format!(
            "SELECT {col}, SUM(up + down) AS b FROM ({}) WHERE {where_sql} AND {col} != '' GROUP BY 1 ORDER BY b DESC LIMIT 3",
            source(true)
        ))?;
        let rows = stmt.query_map(params_from_iter(args.iter()), |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64)))?;
        rows.collect()
    }

    /// The unsaved minute, visible to queries without writing it.
    fn refresh_live(&mut self) -> rusqlite::Result<()> {
        let tx = self.db.transaction()?;
        tx.execute("DELETE FROM live", [])?;
        {
            let mut ins = tx.prepare_cached(&format!("INSERT INTO live (res, bucket, {COLUMNS}, up, down, conns) VALUES (60, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)"))?;
            for (k, a) in &self.pending {
                ins.execute(params![self.bucket, k.app, k.host, k.domain, k.ip, k.asn, k.org, k.country, k.outbound, k.grp, k.rule, k.network, a.up as i64, a.down as i64, a.conns as i64])?;
            }
        }
        tx.commit()
    }

    /// `WHERE` text and arguments for a range plus the query's filters.
    fn condition(&self, q: &Query, minutes: bool, from: i64, to: i64) -> (String, Vec<Sql>) {
        let mut parts = vec!["bucket >= ?1".to_owned(), "bucket < ?2".to_owned()];
        let mut args = vec![Sql::Integer(from), Sql::Integer(to)];
        if !minutes {
            let rolled: i64 = meta(&self.db, "rolled").and_then(|v| v.parse().ok()).unwrap_or(0);
            args.push(Sql::Integer(rolled));
            parts.push(format!("((res = 3600 AND bucket < ?{n}) OR (res = 60 AND bucket >= ?{n}))", n = args.len()));
        }
        let mut by_dim: Vec<(&str, bool, Vec<&str>)> = Vec::new();
        for f in &q.filters {
            let Some(col) = column(&f.dim) else { continue };
            match by_dim.iter_mut().find(|(c, e, _)| *c == col && *e == f.exclude) {
                Some(entry) => entry.2.push(&f.value),
                None => by_dim.push((col, f.exclude, vec![&f.value])),
            }
        }
        for (col, exclude, values) in by_dim {
            let mut marks = Vec::new();
            for v in values {
                args.push(if col == "asn" { Sql::Integer(v.trim_start_matches(['A', 'S', 'a', 's']).parse().unwrap_or(-1)) } else { Sql::Text(v.to_owned()) });
                marks.push(format!("?{}", args.len()));
            }
            parts.push(format!("{col} {}IN ({})", if exclude { "NOT " } else { "" }, marks.join(", ")));
        }
        let search = q.search.trim();
        if !search.is_empty() {
            args.push(Sql::Text(format!("%{}%", search.replace(['%', '_'], ""))));
            let n = args.len();
            parts.push(format!("(app LIKE ?{n} OR host LIKE ?{n} OR ip LIKE ?{n} OR org LIKE ?{n} OR country LIKE ?{n} OR outbound LIKE ?{n} OR grp LIKE ?{n})"));
        }
        (parts.join(" AND "), args)
    }
}

fn source(minutes: bool) -> String {
    let saved = if minutes { "SELECT * FROM samples WHERE res = 60" } else { "SELECT * FROM samples" };
    format!("{saved} UNION ALL SELECT * FROM live")
}

fn column(dim: &str) -> Option<&'static str> {
    DIMS.iter().find(|(d, _)| *d == dim).map(|(_, c)| *c)
}

fn sql_text(v: Sql) -> String {
    match v {
        Sql::Text(s) => s,
        Sql::Integer(n) => n.to_string(),
        _ => String::new(),
    }
}

fn meta(db: &Db, key: &str) -> Option<String> {
    db.query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| r.get(0)).ok()
}

fn fold_small(pending: HashMap<Key, Acc>) -> HashMap<Key, Acc> {
    let mut out: HashMap<Key, Acc> = HashMap::with_capacity(pending.len());
    for (k, a) in pending {
        let k = if a.up + a.down < SMALL && !k.host.is_empty() {
            Key { app: k.app, outbound: k.outbound, grp: k.grp, rule: k.rule, network: k.network, ..Default::default() }
        } else {
            k
        };
        out.entry(k).or_default().add(a);
    }
    out
}

/// About 120 points, on a round step no finer than the stored resolution.
fn pick_step(span: i64, res: i64) -> i64 {
    const STEPS: &[i64] = &[MINUTE, 2 * MINUTE, 5 * MINUTE, 10 * MINUTE, 15 * MINUTE, 30 * MINUTE, HOUR, 2 * HOUR, 3 * HOUR, 6 * HOUR, 12 * HOUR, 24 * HOUR];
    STEPS.iter().copied().find(|s| *s >= res && span / s <= 120).unwrap_or(24 * HOUR)
}

/// Down + up resampled to `n` points.
fn spark(up: &[u64], down: &[u64], n: usize) -> Vec<u64> {
    let len = up.len();
    if len == 0 {
        return Vec::new();
    }
    let n = n.min(len);
    (0..n).map(|i| (i * len / n..((i + 1) * len / n).max(i * len / n + 1)).map(|j| up[j] + down[j]).sum()).collect()
}

/// `www.bbc.co.uk` → `bbc.co.uk`, `r3---sn.googlevideo.com` → `googlevideo.com`.
pub fn registrable(host: &str) -> String {
    const SECOND: &[&str] = &["co", "com", "net", "org", "gov", "edu", "ac", "or", "ne", "go", "msk", "spb"];
    let labels: Vec<&str> = host.split('.').filter(|l| !l.is_empty()).collect();
    if labels.len() <= 2 {
        return labels.join(".");
    }
    let n = labels.len();
    let take = if labels[n - 1].len() == 2 && SECOND.contains(&labels[n - 2]) { 3 } else { 2 };
    labels[n - take..].join(".")
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Query {
    pub from: i64,
    pub to: Option<i64>,
    pub step: Option<i64>,
    pub by: String,
    pub filters: Vec<Filter>,
    pub search: String,
    /// Keys charted separately; the rest is `other`.
    pub limit: Option<usize>,
    /// Keys listed in `rows`.
    pub rows: Option<usize>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Filter {
    pub dim: String,
    pub value: String,
    pub exclude: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SeriesView {
    pub key: String,
    pub label: String,
    pub up: Vec<u64>,
    pub down: Vec<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RowView {
    pub key: String,
    pub label: String,
    pub up: u64,
    pub down: u64,
    pub conns: u64,
    pub share: f64,
    pub org: String,
    pub country: String,
    pub app: String,
    pub hosts: u64,
    pub spark: Vec<u64>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TotalView {
    pub up: u64,
    pub down: u64,
    pub conns: u64,
    pub apps: u64,
    pub hosts: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueryView {
    pub by: String,
    pub from: i64,
    pub to: i64,
    pub step: i64,
    pub series: Vec<SeriesView>,
    pub other: SeriesView,
    pub all: SeriesView,
    pub rows: Vec<RowView>,
    pub total: TotalView,
    pub previous: TotalView,
    /// Highest download rate of one step, bytes per second, and when.
    pub peak: u64,
    pub peak_at: i64,
    /// Oldest stored bucket.
    pub since: Option<i64>,
}

#[derive(Debug, Clone, Copy, Default)]
struct AppTotal {
    total: u64,
    proxied: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Anomaly {
    /// `traffic_spike` or `traffic_route`.
    pub kind: &'static str,
    pub app: String,
    /// Bytes in the last hour, and of them through a proxy.
    pub bytes: u64,
    pub proxied: u64,
    pub usual_per_hour: u64,
    /// No history for this app before the last hour.
    pub new: bool,
    pub domains: Vec<(String, u64)>,
    pub groups: Vec<(String, u64)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusView {
    pub enabled: bool,
    pub rows: i64,
    pub since: Option<i64>,
    pub bytes: i64,
    pub asn: bool,
    pub country: bool,
}

/// Poll the core's connections while it runs.
pub async fn collect(state: Arc<State>) {
    let mut status = state.watch_status();
    let mut last_download = 0i64;
    let mut last_check = crate::problems::now_ms();
    loop {
        if !state.core_running() {
            if let Err(e) = state.traffic.lock().unwrap().flush() {
                tracing::warn!("saving traffic: {e}");
            }
            state.traffic.lock().unwrap().prime_reset();
            let _ = status.changed().await;
            continue;
        }
        let now = crate::problems::now_ms();
        match state.core.connections().await {
            Ok(c) => state.traffic.lock().unwrap().ingest(&c.connections, now),
            Err(e) => tracing::debug!("traffic poll: {e}"),
        }
        if now - last_check >= CHECK_EVERY {
            last_check = now;
            let found = state.traffic.lock().unwrap().anomalies(now);
            match found {
                Ok(list) => {
                    for a in list {
                        let key = format!("{}:{}", a.kind, a.app);
                        state.notify(&key, a.kind, "normal", serde_json::to_value(&a).unwrap_or_default());
                    }
                }
                Err(e) => tracing::warn!("traffic check: {e}"),
            }
        }
        let wants_country = { let t = state.traffic.lock().unwrap(); t.enabled && (!t.has_country() || t.own_country_stale()) };
        if wants_country && now - last_download > 24 * HOUR {
            last_download = now;
            tokio::spawn(download_country(state.clone()));
        }
        tokio::time::sleep(POLL).await;
    }
}

impl Store {
    /// A restarted core reuses no ids; start a new baseline.
    pub fn prime_reset(&mut self) {
        self.prev.clear();
        self.primed = false;
    }
}

/// Fetch a country database once, direct or through the core.
async fn download_country(state: Arc<State>) {
    let path = state.config.paths().root.join(COUNTRY_FILE);
    let proxy = crate::config::proxy_of(&state);
    let mut attempts: Vec<Option<String>> = vec![None];
    if proxy.is_some() {
        attempts.push(proxy);
    }
    for via in attempts {
        let mut client = reqwest::Client::builder().timeout(Duration::from_secs(180)).no_proxy();
        if let Some(p) = &via {
            match reqwest::Proxy::all(p) {
                Ok(p) => client = client.proxy(p),
                Err(_) => continue,
            }
        }
        let Ok(client) = client.build() else { continue };
        let body = match client.get(COUNTRY_URL).send().await.and_then(|r| r.error_for_status()) {
            Ok(r) => r.bytes().await,
            Err(e) => Err(e),
        };
        match body {
            Ok(bytes) if mmdb::Reader::from_bytes(bytes.to_vec()).is_some() => {
                if let Err(e) = omm_config::paths::write_atomic(&path, &bytes) {
                    tracing::warn!("saving country database: {e}");
                    return;
                }
                tracing::info!("downloaded country database for traffic statistics");
                state.traffic.lock().unwrap().reload_geo();
                return;
            }
            Ok(_) => tracing::debug!("country database: not an mmdb"),
            Err(e) => tracing::debug!("country database: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mihomo_api::model::Metadata;

    fn conn(id: &str, process: &str, host: &str, up: u64, down: u64) -> Connection {
        Connection {
            id: id.into(),
            metadata: Metadata { process: process.into(), host: host.into(), destination_ip: "8.8.8.8".into(), network: "tcp".into(), ..Default::default() },
            upload: up,
            download: down,
            chains: vec!["NL".into(), "Proxy".into()],
            rule: "Match".into(),
            ..Default::default()
        }
    }

    const T0: i64 = 1_760_000_000_000 - 1_760_000_000_000 % HOUR;

    #[test]
    fn diffs_by_id() {
        let mut s = Store::in_memory();
        s.ingest(&[conn("a", "firefox", "www.youtube.com", 100, 1000)], T0);
        assert!(s.pending.is_empty(), "first poll only primes");
        s.ingest(&[conn("a", "firefox", "www.youtube.com", 150, 101_000), conn("b", "steam", "cdn.steam.com", 0, 50_000)], T0 + 2000);
        let total: u64 = s.pending.values().map(|a| a.down).sum();
        assert_eq!(total, 150_000);
        let conns: u64 = s.pending.values().map(|a| a.conns).sum();
        assert_eq!(conns, 1);
        // A reused id with smaller totals counts from zero.
        s.ingest(&[conn("a", "firefox", "www.youtube.com", 10, 20_000)], T0 + 4000);
        let total: u64 = s.pending.values().map(|a| a.down).sum();
        assert_eq!(total, 170_000);
    }

    #[test]
    fn unnamed_connections_get_an_owner() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("net")).unwrap();
        // Port 0xC1A6 = 49574, owned by root (uid 0).
        std::fs::write(dir.path().join("net/tcp"), "  sl  local_address rem_address   st\n   3: 0100A8C0:C1A6 CCA21268:01BB 01 00000000:00000000 02:000009F5 00000000     0        0 412233 2\n").unwrap();
        let mut s = Store::in_memory();
        s.proc_root = dir.path().to_owned();
        let mut c = conn("a", "", "pkgs.omarchy.org", 0, 0);
        c.metadata.source_port = "49574".into();
        s.ingest(&[c.clone()], T0);
        c.download = 9000;
        s.ingest(&[c], T0 + 2000);
        assert_eq!(s.pending.keys().next().unwrap().app, ROOT_APP);
    }

    #[test]
    fn registrable_domains() {
        assert_eq!(registrable("r3---sn-abc.googlevideo.com"), "googlevideo.com");
        assert_eq!(registrable("www.bbc.co.uk"), "bbc.co.uk");
        assert_eq!(registrable("github.com"), "github.com");
        assert_eq!(registrable("api.example.com.ru"), "example.com.ru");
        assert_eq!(registrable(""), "");
    }

    fn filled() -> Store {
        let mut s = Store::in_memory();
        s.ingest(&[], T0);
        let mut ff = 0;
        let mut st = 0;
        for m in 0..10 {
            ff += 1_000_000;
            st += if m < 5 { 3_000_000 } else { 0 };
            s.ingest(&[conn("a", "firefox", "www.youtube.com", 1000 * m, ff), conn("b", "steam", "cdn.steam.com", 0, st), conn("c", "curl", "tiny.example.org", 0, 10)], T0 + m as i64 * MINUTE + 1000);
        }
        s.ingest(&[], T0 + 10 * MINUTE + 1000);
        s
    }

    #[test]
    fn query_groups_and_filters() {
        let mut s = filled();
        let now = T0 + 11 * MINUTE;
        let v = s.query(&Query { from: T0, to: Some(now), by: "app".into(), ..Default::default() }, now).unwrap();
        assert_eq!(v.rows[0].key, "steam");
        assert_eq!(v.rows[0].down, 15_000_000);
        assert_eq!(v.rows[1].key, "firefox");
        assert_eq!(v.rows[1].down, 10_000_000);
        assert_eq!(v.total.down, 25_000_010);
        assert_eq!(v.step, MINUTE);
        let summed: u64 = v.all.down.iter().sum();
        assert_eq!(summed, v.total.down);

        let f = Query { from: T0, to: Some(now), by: "domain".into(), filters: vec![Filter { dim: "app".into(), value: "firefox".into(), exclude: false }], ..Default::default() };
        let v = s.query(&f, now).unwrap();
        assert_eq!(v.rows.len(), 1);
        assert_eq!(v.rows[0].key, "youtube.com");

        let x = Query { from: T0, to: Some(now), by: "app".into(), filters: vec![Filter { dim: "app".into(), value: "steam".into(), exclude: true }], limit: Some(1), ..Default::default() };
        let v = s.query(&x, now).unwrap();
        assert!(v.rows.iter().all(|r| r.key != "steam"));
        assert_eq!(v.series.len(), 1);
        assert_eq!(v.other.down.iter().sum::<u64>(), 10, "curl is in other");

        let v = s.query(&Query { from: T0, to: Some(now), by: "app".into(), search: "steam".into(), ..Default::default() }, now).unwrap();
        assert_eq!(v.rows.len(), 1);
    }

    /// Writes `bytes` for `app` through `grp` spread over the hour at `at`.
    fn hour_of(s: &mut Store, at: i64, app: &str, grp: &str, bytes: u64) {
        s.db.execute(
            &format!("INSERT INTO samples (res, bucket, {COLUMNS}, up, down, conns) VALUES (60, ?1, ?2, 'h.example.com', 'example.com', '1.1.1.1', 0, '', '', ?3, ?3, 'Match', 'tcp', 0, ?4, 1)"),
            params![at, app, grp, bytes as i64],
        )
        .unwrap();
    }

    #[test]
    fn spikes_and_route_changes() {
        let mut s = Store::in_memory();
        let now = T0 + 3 * DAY;
        const MB: u64 = 1024 * 1024;
        // Two days of history: firefox 100 MB an hour direct, steam quiet.
        for h in 1..48 {
            hour_of(&mut s, now - h * HOUR - 30 * MINUTE, "firefox", "DIRECT", 100 * MB);
            hour_of(&mut s, now - h * HOUR - 30 * MINUTE, "curl", "DIRECT", 10 * MB);
        }
        s.maintain(now - 2 * HOUR).unwrap();
        // Last hour: firefox ×8 direct, curl 150 MB via proxy, telegram new.
        hour_of(&mut s, now - 10 * MINUTE, "firefox", "DIRECT", 800 * MB);
        hour_of(&mut s, now - 10 * MINUTE, "curl", "Proxy", 150 * MB);
        hour_of(&mut s, now - 10 * MINUTE, "Telegram", "DIRECT", 600 * MB);
        hour_of(&mut s, now - 10 * MINUTE, "[root]", "DIRECT", 900 * MB);
        let mut found = s.anomalies(now).unwrap();
        found.sort_by(|a, b| a.app.cmp(&b.app));
        let got: Vec<(&str, &str, bool)> = found.iter().map(|a| (a.app.as_str(), a.kind, a.new)).collect();
        assert_eq!(got, [("Telegram", "traffic_spike", true), ("curl", "traffic_route", false), ("firefox", "traffic_spike", false)]);
        assert_eq!(found[1].groups[0].0, "Proxy");
        // Not again within six hours.
        assert!(s.anomalies(now + MINUTE).unwrap().is_empty());
    }

    #[test]
    fn no_alerts_without_history() {
        let mut s = Store::in_memory();
        hour_of(&mut s, T0, "firefox", "DIRECT", 5 << 30);
        assert!(s.anomalies(T0 + 10 * MINUTE).unwrap().is_empty());
    }

    #[test]
    fn totals_by_rule_and_group() {
        let mut s = filled();
        let now = T0 + 11 * MINUTE;
        let r = s.totals("rule", T0, now).unwrap();
        assert_eq!(r["match"].down, 25_000_010);
        let g = s.totals("group", T0, now).unwrap();
        assert_eq!(g["Proxy"].down, 25_000_010);
        assert!(s.totals("nope", T0, now).unwrap().is_empty());
    }

    #[test]
    fn small_keys_fold() {
        let s = filled();
        let tiny: i64 = s.db.query_row("SELECT COUNT(*) FROM samples WHERE app = 'curl' AND host = ''", [], |r| r.get(0)).unwrap();
        assert!(tiny > 0);
    }

    #[test]
    fn rolls_up_hours_and_queries_them() {
        let mut s = filled();
        s.maintain(T0 + 2 * HOUR).unwrap();
        let hours: i64 = s.db.query_row("SELECT COUNT(*) FROM samples WHERE res = 3600", [], |r| r.get(0)).unwrap();
        assert!(hours > 0);
        let now = T0 + 3 * 24 * HOUR;
        let v = s.query(&Query { from: T0 - HOUR, to: Some(now), by: "app".into(), ..Default::default() }, now).unwrap();
        assert_eq!(v.total.down, 25_000_010, "hours, not hours + minutes");
        // Rolling again does not double count.
        s.maintain(T0 + 3 * HOUR).unwrap();
        let v = s.query(&Query { from: T0 - HOUR, to: Some(now), by: "app".into(), ..Default::default() }, now).unwrap();
        assert_eq!(v.total.down, 25_000_010);
        // Minutes past two days go.
        s.maintain(T0 + 3 * 24 * HOUR).unwrap();
        let minutes: i64 = s.db.query_row("SELECT COUNT(*) FROM samples WHERE res = 60", [], |r| r.get(0)).unwrap();
        assert_eq!(minutes, 0);
    }

    #[test]
    fn live_minute_is_visible() {
        let mut s = Store::in_memory();
        s.ingest(&[], T0);
        s.ingest(&[conn("a", "firefox", "www.youtube.com", 0, 500_000)], T0 + 5000);
        let v = s.query(&Query { from: T0, to: Some(T0 + MINUTE), by: "app".into(), ..Default::default() }, T0 + 6000).unwrap();
        assert_eq!(v.total.down, 500_000);
    }

    #[test]
    fn disabled_records_nothing() {
        let mut s = Store::in_memory();
        s.set_enabled(false).unwrap();
        s.ingest(&[], T0);
        s.ingest(&[conn("a", "firefox", "x.com", 0, 5)], T0 + 2000);
        assert!(s.pending.is_empty());
    }

    #[test]
    fn steps() {
        assert_eq!(pick_step(HOUR, MINUTE), MINUTE);
        assert_eq!(pick_step(24 * HOUR, MINUTE), 15 * MINUTE);
        assert_eq!(pick_step(7 * 24 * HOUR, HOUR), 2 * HOUR);
        assert_eq!(pick_step(30 * 24 * HOUR, HOUR), 6 * HOUR);
    }

    /// Polls the running core read-only for a minute and a half and prints
    /// the breakdown: `cargo test -p omm-daemon live_core -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore]
    async fn live_core() {
        let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap()).join(".local/share/omarchy-mihomo/runtime");
        let client = mihomo_api::Client::new(home.join("mihomo.sock"));
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::open(&dir.path().join("traffic.db"), &home).unwrap();
        let start = crate::problems::now_ms();
        while crate::problems::now_ms() - start < 90_000 {
            let c = client.connections().await.unwrap();
            s.ingest(&c.connections, crate::problems::now_ms());
            tokio::time::sleep(POLL).await;
        }
        let now = crate::problems::now_ms();
        for by in ["app", "domain", "ip", "asn", "country", "group"] {
            let v = s.query(&Query { from: start, by: by.into(), rows: Some(5), ..Default::default() }, now).unwrap();
            println!("== {by}: down {} up {} conns {} step {} points {}", v.total.down, v.total.up, v.total.conns, v.step, v.all.down.len());
            for r in &v.rows {
                println!("   {:<40} {:>12} {:>10} {:.1}% {} {}", r.key, r.down, r.up, r.share * 100.0, r.org, r.country);
            }
        }
        println!("{:?}", s.status().map(|x| (x.rows, x.bytes, x.asn, x.country)));
    }
}
