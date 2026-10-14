//! Problem resources: failed connections from mihomo's log, merged per host.
//!
//! The collector only *records*. It never marks anything as blocked and
//! never changes routing; deciding what to do is left to the user.

pub mod classify;
pub mod parse;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::net::IpAddr;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

pub use classify::{ErrorKind, classify};
pub use parse::{Line, Proto, Route};

pub type Result<T, E = rusqlite::Error> = std::result::Result<T, E>;

/// Keep at most this many distinct ports per host.
const MAX_PORTS: usize = 8;
/// Keep at most this many distinct apps per host.
const MAX_APPS: usize = 16;
pub const RETENTION_DAYS: i64 = 14;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Problem {
    pub host: String,
    pub is_ip: bool,
    pub ports: BTreeSet<u16>,
    pub protos: BTreeSet<Proto>,
    /// Process name → failures.
    pub apps: BTreeMap<String, u64>,
    pub kinds: BTreeMap<ErrorKind, u64>,
    pub last_kind: ErrorKind,
    pub last_error: String,
    /// Route of the latest failure.
    pub route: Route,
    /// Unix milliseconds.
    pub first_seen: i64,
    pub last_seen: i64,
    pub count: u64,
    /// Successful connections since the latest failure; a host that keeps
    /// working is probably just flaky, not unreachable.
    pub ok_since_error: u64,
    #[serde(default)]
    pub last_ok: Option<i64>,
    /// Route of the latest success, e.g. `AI[FI]`.
    #[serde(default)]
    pub last_ok_chain: Option<String>,
    /// For IP-only entries: the domain a domain rule matched. With
    /// `override-destination: false` mihomo logs the IP even though the
    /// sniffed name decided the route; the name is what a rule should use.
    #[serde(default)]
    pub domain_hint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IgnoreKind {
    /// Exactly this host.
    Host,
    /// The domain and its subdomains.
    Suffix,
    /// Every failure of one application.
    App,
}

impl IgnoreKind {
    fn as_str(self) -> &'static str {
        match self {
            IgnoreKind::Host => "host",
            IgnoreKind::Suffix => "suffix",
            IgnoreKind::App => "app",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        [IgnoreKind::Host, IgnoreKind::Suffix, IgnoreKind::App].into_iter().find(|k| k.as_str() == s)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ignore {
    pub id: i64,
    pub kind: IgnoreKind,
    pub value: String,
    pub created: i64,
}

impl Ignore {
    fn matches(&self, p: &Problem) -> bool {
        match self.kind {
            IgnoreKind::Host => p.host == self.value,
            IgnoreKind::Suffix => p.host == self.value || p.host.strip_suffix(&self.value).is_some_and(|rest| rest.ends_with('.')),
            IgnoreKind::App => p.apps.contains_key(&self.value),
        }
    }
}

/// A problem as shown to clients.
#[derive(Debug, Clone, Serialize)]
pub struct View<'a> {
    #[serde(flatten)]
    pub problem: &'a Problem,
    pub new: bool,
    pub hidden: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Query {
    pub only_new: bool,
    /// Show the hidden problems instead of the visible ones.
    pub hidden: bool,
    pub kind: Option<ErrorKind>,
    pub app: Option<String>,
    /// Rule target (`DIRECT`, a group).
    pub target: Option<String>,
    /// Substring of host or app.
    pub search: Option<String>,
    /// Only problems seen after this unix-ms time.
    pub since: Option<i64>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Counts {
    pub new: usize,
    pub visible: usize,
    pub hidden: usize,
}

pub struct Collector {
    db: Connection,
    problems: HashMap<String, Problem>,
    dirty: HashSet<String>,
    ignores: Vec<Ignore>,
    /// Problems seen after this unix-ms time are "new".
    seen_mark: i64,
}

impl Collector {
    pub fn open(path: &Path) -> Result<Self> {
        Self::with(Connection::open(path)?)
    }

    pub fn in_memory() -> Result<Self> {
        Self::with(Connection::open_in_memory()?)
    }

    fn with(db: Connection) -> Result<Self> {
        db.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS problems (
                 host TEXT PRIMARY KEY,
                 last_seen INTEGER NOT NULL,
                 data TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS problem_ignores (
                 id INTEGER PRIMARY KEY,
                 kind TEXT NOT NULL,
                 value TEXT NOT NULL,
                 created INTEGER NOT NULL,
                 UNIQUE (kind, value)
             );
             CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )?;

        let mut problems = HashMap::new();
        {
            let mut stmt = db.prepare("SELECT data FROM problems")?;
            for data in stmt.query_map([], |r| r.get::<_, String>(0))? {
                // A row from an incompatible older build is dropped, not fatal.
                if let Ok(p) = serde_json::from_str::<Problem>(&data?) {
                    problems.insert(p.host.clone(), p);
                }
            }
        }
        let ignores = {
            let mut stmt = db.prepare("SELECT id, kind, value, created FROM problem_ignores ORDER BY id")?;
            stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?)))?
                .filter_map(|r| r.ok())
                .filter_map(|(id, kind, value, created)| Some(Ignore { id, kind: IgnoreKind::parse(&kind)?, value, created }))
                .collect()
        };
        let seen_mark = db
            .query_row("SELECT value FROM meta WHERE key = 'problems_seen_mark'", [], |r| r.get::<_, String>(0))
            .optional()?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        Ok(Self { db, problems, dirty: HashSet::new(), ignores, seen_mark })
    }

    // ------------------------------------------------------------ ingest

    /// Feed one parsed log line. Returns true when a failure was recorded.
    pub fn ingest(&mut self, line: &Line, now: i64) -> bool {
        match line {
            Line::DialError(e) => {
                if skip_host(&e.dest.host) {
                    return false;
                }
                let kind = classify(&e.error, &e.route.target);
                let app = if e.source.inner { Some("mihomo") } else { e.source.process.as_deref() };
                let p = self.problems.entry(e.dest.host.clone()).or_insert_with(|| Problem {
                    host: e.dest.host.clone(),
                    is_ip: e.dest.host.parse::<IpAddr>().is_ok(),
                    ports: BTreeSet::new(),
                    protos: BTreeSet::new(),
                    apps: BTreeMap::new(),
                    kinds: BTreeMap::new(),
                    last_kind: kind,
                    last_error: String::new(),
                    route: Route::default(),
                    first_seen: now,
                    last_seen: now,
                    count: 0,
                    ok_since_error: 0,
                    last_ok: None,
                    last_ok_chain: None,
                    domain_hint: None,
                });
                if p.is_ip && matches!(e.route.rule.as_str(), "Domain" | "DomainSuffix") && !e.route.payload.is_empty() {
                    p.domain_hint = Some(e.route.payload.to_ascii_lowercase());
                }
                if p.ports.len() < MAX_PORTS {
                    p.ports.insert(e.dest.port);
                }
                p.protos.insert(e.proto);
                if let Some(app) = app
                    && (p.apps.len() < MAX_APPS || p.apps.contains_key(app))
                {
                    *p.apps.entry(app.to_owned()).or_default() += 1;
                }
                *p.kinds.entry(kind).or_default() += 1;
                p.last_kind = kind;
                p.last_error.clone_from(&e.error);
                p.route.clone_from(&e.route);
                p.last_seen = now;
                p.count += 1;
                p.ok_since_error = 0;
                self.dirty.insert(e.dest.host.clone());
                true
            }
            Line::Connected(c) => {
                if let Some(p) = self.problems.get_mut(&c.dest.host) {
                    p.ok_since_error += 1;
                    p.last_ok = Some(now);
                    p.last_ok_chain = Some(c.chain.clone());
                    self.dirty.insert(c.dest.host.clone());
                }
                false
            }
        }
    }

    /// Write changed problems in one transaction.
    pub fn flush(&mut self) -> Result<usize> {
        if self.dirty.is_empty() {
            return Ok(0);
        }
        let tx = self.db.transaction()?;
        let mut n = 0;
        {
            let mut up = tx.prepare_cached("INSERT INTO problems (host, last_seen, data) VALUES (?1, ?2, ?3) ON CONFLICT(host) DO UPDATE SET last_seen = ?2, data = ?3")?;
            for host in self.dirty.drain() {
                if let Some(p) = self.problems.get(&host) {
                    up.execute(params![host, p.last_seen, serde_json::to_string(p).expect("problem serializes")])?;
                    n += 1;
                }
            }
        }
        tx.commit()?;
        Ok(n)
    }

    /// Forget problems not seen for `RETENTION_DAYS`.
    pub fn purge(&mut self, now: i64) -> Result<usize> {
        let cutoff = now - RETENTION_DAYS * 86_400_000;
        self.problems.retain(|_, p| p.last_seen >= cutoff);
        self.db.execute("DELETE FROM problems WHERE last_seen < ?1", [cutoff])
    }

    // ------------------------------------------------------------ queries

    pub fn is_hidden(&self, p: &Problem) -> bool {
        self.ignores.iter().any(|i| i.matches(p))
    }

    pub fn counts(&self) -> Counts {
        let mut c = Counts::default();
        for p in self.problems.values() {
            if self.is_hidden(p) {
                c.hidden += 1;
            } else {
                c.visible += 1;
                if p.last_seen > self.seen_mark {
                    c.new += 1;
                }
            }
        }
        c
    }

    pub fn list(&self, q: &Query) -> Vec<View<'_>> {
        let search = q.search.as_deref().map(str::to_lowercase).filter(|s| !s.is_empty());
        let mut out: Vec<View> = self
            .problems
            .values()
            .map(|p| View { problem: p, new: p.last_seen > self.seen_mark, hidden: self.is_hidden(p) })
            .filter(|v| v.hidden == q.hidden)
            .filter(|v| !q.only_new || v.new)
            .filter(|v| q.kind.is_none_or(|k| v.problem.kinds.contains_key(&k)))
            .filter(|v| q.app.as_ref().is_none_or(|a| v.problem.apps.contains_key(a)))
            .filter(|v| q.target.as_ref().is_none_or(|t| &v.problem.route.target == t))
            .filter(|v| q.since.is_none_or(|s| v.problem.last_seen >= s))
            .filter(|v| {
                search.as_ref().is_none_or(|s| {
                    let p = v.problem;
                    p.host.contains(s.as_str())
                        || p.domain_hint.as_deref().is_some_and(|h| h.contains(s.as_str()))
                        || p.apps.keys().any(|a| a.to_lowercase().contains(s.as_str()))
                })
            })
            .collect();
        out.sort_by(|a, b| b.problem.last_seen.cmp(&a.problem.last_seen).then_with(|| a.problem.host.cmp(&b.problem.host)));
        if let Some(n) = q.limit {
            out.truncate(n);
        }
        out
    }

    pub fn get(&self, host: &str) -> Option<View<'_>> {
        self.problems.get(host).map(|p| View { problem: p, new: p.last_seen > self.seen_mark, hidden: self.is_hidden(p) })
    }

    /// Distinct apps and targets, for filter menus.
    pub fn facets(&self) -> (BTreeSet<&str>, BTreeSet<&str>) {
        let apps = self.problems.values().flat_map(|p| p.apps.keys().map(String::as_str)).collect();
        let targets = self.problems.values().map(|p| p.route.target.as_str()).filter(|t| !t.is_empty()).collect();
        (apps, targets)
    }

    // ------------------------------------------------------------ actions

    /// Everything seen up to `now` is no longer new.
    pub fn mark_seen(&mut self, now: i64) -> Result<()> {
        self.seen_mark = now;
        self.db.execute("INSERT INTO meta (key, value) VALUES ('problems_seen_mark', ?1) ON CONFLICT(key) DO UPDATE SET value = ?1", [now.to_string()])?;
        Ok(())
    }

    pub fn ignores(&self) -> &[Ignore] {
        &self.ignores
    }

    pub fn hide(&mut self, kind: IgnoreKind, value: &str, now: i64) -> Result<Ignore> {
        let value = value.trim().to_owned();
        if let Some(i) = self.ignores.iter().find(|i| i.kind == kind && i.value == value) {
            return Ok(i.clone());
        }
        self.db.execute("INSERT INTO problem_ignores (kind, value, created) VALUES (?1, ?2, ?3)", params![kind.as_str(), value, now])?;
        let ignore = Ignore { id: self.db.last_insert_rowid(), kind, value, created: now };
        self.ignores.push(ignore.clone());
        Ok(ignore)
    }

    pub fn unhide(&mut self, id: i64) -> Result<bool> {
        let n = self.db.execute("DELETE FROM problem_ignores WHERE id = ?1", [id])?;
        self.ignores.retain(|i| i.id != id);
        Ok(n > 0)
    }

    /// Forget one problem, or all of them.
    pub fn clear(&mut self, host: Option<&str>) -> Result<usize> {
        match host {
            Some(h) => {
                self.problems.remove(h);
                self.dirty.remove(h);
                self.db.execute("DELETE FROM problems WHERE host = ?1", [h])
            }
            None => {
                self.problems.clear();
                self.dirty.clear();
                self.db.execute("DELETE FROM problems", [])
            }
        }
    }
}

/// Addresses that say nothing about a real resource: fake-ip leftovers
/// (198.18.0.0/15) and unspecified addresses.
fn skip_host(host: &str) -> bool {
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => {
            let o = v4.octets();
            (o[0] == 198 && (o[1] & 0xfe) == 18) || v4.is_unspecified()
        }
        Ok(IpAddr::V6(v6)) => v6.is_unspecified(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TIMEOUT: &str = "[TCP] dial DIRECT (match Match/) 198.18.0.1:38164(Telegram, uid=1000) --> 142.252.58.41:2137 error: dial tcp 142.252.58.41:2137: i/o timeout";
    const RESET: &str = "[TCP] dial DIRECT (match DomainSuffix/x.com) 198.18.0.1:1(firefox) --> api.x.com:443 error: read: connection reset by peer";

    fn feed(c: &mut Collector, line: &str, now: i64) -> bool {
        c.ingest(&parse::parse(line).expect("parses"), now)
    }

    #[test]
    fn merges_repeats_per_host() {
        let mut c = Collector::in_memory().unwrap();
        assert!(feed(&mut c, TIMEOUT, 1000));
        assert!(feed(&mut c, &TIMEOUT.replace(":2137", ":4927"), 2000));
        assert!(feed(&mut c, RESET, 3000));
        let list = c.list(&Query::default());
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].problem.host, "api.x.com", "newest first");
        let tg = c.get("142.252.58.41").unwrap().problem;
        assert_eq!(tg.count, 2);
        assert_eq!(tg.ports, BTreeSet::from([2137, 4927]));
        assert_eq!(tg.apps["Telegram"], 2);
        assert_eq!(tg.kinds[&ErrorKind::Timeout], 2);
        assert!(tg.is_ip);
        assert_eq!((tg.first_seen, tg.last_seen), (1000, 2000));
        assert_eq!(tg.route.target, "DIRECT");
    }

    #[test]
    fn success_after_failure_is_tracked_but_not_a_problem() {
        let mut c = Collector::in_memory().unwrap();
        let ok = "[TCP] 198.18.0.1:2(firefox) --> api.x.com:443 match DomainSuffix(x.com) using PROXY[FI]";
        assert!(!feed(&mut c, ok, 1), "unknown host: ignored");
        assert!(c.list(&Query::default()).is_empty());
        feed(&mut c, RESET, 10);
        feed(&mut c, ok, 20);
        let p = c.get("api.x.com").unwrap().problem;
        assert_eq!((p.ok_since_error, p.last_ok, p.last_ok_chain.as_deref()), (1, Some(20), Some("PROXY[FI]")));
        feed(&mut c, RESET, 30);
        assert_eq!(c.get("api.x.com").unwrap().problem.ok_since_error, 0);
    }

    #[test]
    fn new_hidden_and_filters() {
        let mut c = Collector::in_memory().unwrap();
        feed(&mut c, TIMEOUT, 100);
        feed(&mut c, RESET, 200);
        assert_eq!(c.counts(), Counts { new: 2, visible: 2, hidden: 0 });
        c.mark_seen(150).unwrap();
        assert_eq!(c.counts().new, 1);
        assert_eq!(c.list(&Query { only_new: true, ..Default::default() })[0].problem.host, "api.x.com");

        let ig = c.hide(IgnoreKind::App, "Telegram", 300).unwrap();
        assert_eq!(c.counts(), Counts { new: 1, visible: 1, hidden: 1 });
        assert_eq!(c.list(&Query { hidden: true, ..Default::default() })[0].problem.host, "142.252.58.41");
        c.hide(IgnoreKind::Suffix, "x.com", 300).unwrap();
        assert_eq!(c.counts().visible, 0);
        assert!(c.unhide(ig.id).unwrap());
        assert_eq!(c.counts().visible, 1);

        let q = |q: Query| c.list(&q).into_iter().map(|v| v.problem.host.clone()).collect::<Vec<_>>();
        assert_eq!(q(Query { kind: Some(ErrorKind::Timeout), ..Default::default() }), ["142.252.58.41"]);
        assert_eq!(q(Query { search: Some("TELE".into()), ..Default::default() }), ["142.252.58.41"]);
        assert!(q(Query { app: Some("firefox".into()), ..Default::default() }).is_empty(), "x.com is hidden");
    }

    #[test]
    fn suffix_ignore_is_label_aware() {
        let p = |h: &str| Problem { host: h.into(), ..serde_json::from_str::<Problem>(&serde_json::to_string(&sample()).unwrap()).unwrap() };
        let i = Ignore { id: 1, kind: IgnoreKind::Suffix, value: "x.com".into(), created: 0 };
        assert!(i.matches(&p("x.com")));
        assert!(i.matches(&p("a.b.x.com")));
        assert!(!i.matches(&p("notx.com")));
    }

    #[test]
    fn ip_entries_keep_the_matched_domain() {
        let mut c = Collector::in_memory().unwrap();
        feed(&mut c, "[TCP] dial AI (match DomainSuffix/ChatGPT.com) 198.18.0.1:1 --> 8.6.112.0:443 error: i/o timeout", 1);
        feed(&mut c, "[TCP] dial DIRECT (match IPCIDR/1.0.0.0/8) 198.18.0.1:1 --> 1.2.3.4:443 error: i/o timeout", 1);
        assert_eq!(c.get("8.6.112.0").unwrap().problem.domain_hint.as_deref(), Some("chatgpt.com"));
        assert_eq!(c.get("1.2.3.4").unwrap().problem.domain_hint, None);
        assert_eq!(c.list(&Query { search: Some("chatgpt".into()), ..Default::default() }).len(), 1);
    }

    #[test]
    fn fake_ip_destinations_are_skipped() {
        let mut c = Collector::in_memory().unwrap();
        assert!(!feed(&mut c, &TIMEOUT.replace("142.252.58.41:2137 error", "198.18.3.4:443 error"), 1));
        assert!(!feed(&mut c, &TIMEOUT.replace("142.252.58.41:2137 error", "198.19.3.4:443 error"), 1));
        assert!(feed(&mut c, &TIMEOUT.replace("142.252.58.41:2137 error", "198.20.3.4:443 error"), 1));
    }

    #[test]
    fn persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.db");
        {
            let mut c = Collector::open(&path).unwrap();
            feed(&mut c, TIMEOUT, 1);
            feed(&mut c, RESET, 2);
            assert_eq!(c.flush().unwrap(), 2);
            assert_eq!(c.flush().unwrap(), 0, "nothing dirty");
            c.mark_seen(1).unwrap();
            c.hide(IgnoreKind::Host, "api.x.com", 3).unwrap();
        }
        let mut c = Collector::open(&path).unwrap();
        assert_eq!(c.counts(), Counts { new: 0, visible: 1, hidden: 1 });
        assert_eq!(c.get("142.252.58.41").unwrap().problem.count, 1);
        let now = 2 + RETENTION_DAYS * 86_400_000;
        assert_eq!(c.purge(now).unwrap(), 1, "the older one expires");
        assert!(c.get("142.252.58.41").is_none());
        assert_eq!(c.clear(None).unwrap(), 1);
    }

    fn sample() -> Problem {
        let mut c = Collector::in_memory().unwrap();
        feed(&mut c, RESET, 1);
        c.get("api.x.com").unwrap().problem.clone()
    }
}
