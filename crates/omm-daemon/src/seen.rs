//! Hosts and processes the core has logged recently, for rule suggestions.
//!
//! The log tail keeps only a few hundred lines; this keeps one entry per
//! host for the last day, bounded so a busy machine cannot grow it forever.

use std::collections::HashMap;

use omm_problems::parse::Line;
use serde::Serialize;

const MAX_HOSTS: usize = 4000;
const MAX_PROCESSES: usize = 600;
const KEEP_MS: i64 = 24 * 3600 * 1000;

#[derive(Debug, Clone, Serialize)]
pub struct Host {
    pub host: String,
    pub last: i64,
    pub count: u32,
    pub process: Option<String>,
    /// Where it went: `Group[Node]`, `DIRECT`...
    pub route: String,
    pub failed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Process {
    pub name: String,
    pub last: i64,
    pub count: u32,
}

#[derive(Default)]
pub struct Seen {
    hosts: HashMap<String, Host>,
    processes: HashMap<String, Process>,
}

impl Seen {
    pub fn record(&mut self, line: &Line, now: i64) {
        let (dest, source, route, failed) = match line {
            Line::Connected(c) => (&c.dest, &c.source, c.chain.clone(), false),
            Line::DialError(e) => (&e.dest, &e.source, e.route.target.clone(), true),
        };
        if source.inner || dest.host.is_empty() {
            return;
        }
        let host = dest.host.to_ascii_lowercase();
        let entry = self.hosts.entry(host.clone()).or_insert_with(|| Host { host, last: now, count: 0, process: None, route: String::new(), failed: false });
        entry.last = now;
        entry.count = entry.count.saturating_add(1);
        entry.route = route;
        entry.failed = failed;
        if source.process.is_some() {
            entry.process.clone_from(&source.process);
        }
        if let Some(name) = source.process.as_deref().filter(|n| !n.is_empty()) {
            let p = self.processes.entry(name.to_owned()).or_insert_with(|| Process { name: name.to_owned(), last: now, count: 0 });
            p.last = now;
            p.count = p.count.saturating_add(1);
        }
        if self.hosts.len() > MAX_HOSTS || self.processes.len() > MAX_PROCESSES {
            self.prune(now);
        }
    }

    /// Drop what is older than a day, then the oldest tenth if still full.
    fn prune(&mut self, now: i64) {
        self.hosts.retain(|_, h| now - h.last < KEEP_MS);
        self.processes.retain(|_, p| now - p.last < KEEP_MS);
        if self.hosts.len() > MAX_HOSTS {
            let mut lasts: Vec<i64> = self.hosts.values().map(|h| h.last).collect();
            lasts.sort_unstable();
            let cut = lasts[lasts.len() / 10];
            self.hosts.retain(|_, h| h.last > cut);
        }
        if self.processes.len() > MAX_PROCESSES {
            let mut lasts: Vec<i64> = self.processes.values().map(|p| p.last).collect();
            lasts.sort_unstable();
            let cut = lasts[lasts.len() / 10];
            self.processes.retain(|_, p| p.last > cut);
        }
    }

    /// Hosts of the last day, most recent first.
    pub fn hosts(&self, now: i64) -> Vec<Host> {
        let mut out: Vec<Host> = self.hosts.values().filter(|h| now - h.last < KEEP_MS).cloned().collect();
        out.sort_by_key(|x| std::cmp::Reverse(x.last));
        out
    }

    pub fn processes(&self, now: i64) -> Vec<Process> {
        let mut out: Vec<Process> = self.processes.values().filter(|p| now - p.last < KEEP_MS).cloned().collect();
        out.sort_by_key(|x| std::cmp::Reverse(x.last));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omm_problems::parse::parse;

    #[test]
    fn records_hosts_and_processes_and_stays_bounded() {
        let mut s = Seen::default();
        let ok = parse("[TCP] 198.18.0.1:48082(chromium, uid=1000) --> www.google.com:443 match DomainSuffix(google.com) using WARP[Стандартный 2]").unwrap();
        let err = parse("[TCP] dial DIRECT (match Match/) 198.18.0.1:38164(Telegram, uid=1000) --> 142.252.58.41:2137 error: dial tcp: i/o timeout").unwrap();
        s.record(&ok, 1);
        s.record(&ok, 2);
        s.record(&err, 3);
        let hosts = s.hosts(10);
        assert_eq!(hosts[0].host, "142.252.58.41");
        assert!(hosts[0].failed);
        assert_eq!((hosts[1].count, hosts[1].process.as_deref()), (2, Some("chromium")));
        assert_eq!(s.processes(10).len(), 2);
        assert!(s.hosts(KEEP_MS + 5).is_empty(), "a day later");

        for i in 0..(MAX_HOSTS as i64 + 50) {
            let line = parse(&format!("[TCP] 198.18.0.1:1 --> h{i}.example:443 using DIRECT")).unwrap();
            s.record(&line, 100 + i);
        }
        assert!(s.hosts.len() <= MAX_HOSTS);
        assert!(s.hosts.contains_key(&format!("h{}.example", MAX_HOSTS + 49)), "newest kept");
    }
}
