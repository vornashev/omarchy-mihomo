//! Autofix: when a host keeps failing, check it through the profile's groups
//! and route it through one that works.
//!
//! The collector only records failures; this module is the one place that
//! changes routing on its own, and only within limits the user set: a fix is
//! a rule "on trial" that pins itself when the host keeps working and
//! removes itself when it does not. Hosts with the user's own rules, local
//! addresses and DNS failures (which a route cannot cure) are left alone.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use omm_config::rules::{MatchKind, UserRule};
use omm_problems::{ErrorKind, Line, classify};
use omm_proto::{ErrorCode, RpcError};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::route_check::Outcome;
use crate::state::State;

/// Failures closer together than this are one failure: a page load retries
/// a dead host several times in a second.
const MERGE_MS: i64 = 10_000;
/// At most one check per host in this time.
const RECHECK_MS: i64 = 30 * 60_000;
/// "Not now", "undo": leave the host alone this long.
const QUIET_MS: i64 = 24 * 3_600_000;
/// The rule's own expiry trails the trial so the trial, not the expiry
/// sweep, decides what happens to it. If the daemon is down at the end of
/// the trial, the rule still switches itself off.
const GRACE_MS: i64 = 10 * 60_000;
/// Finished fixes stay visible this long.
const KEEP_MS: i64 = 24 * 3_600_000;
/// Hosts tracked at once; the oldest are dropped beyond this.
const MAX_TRACKED: usize = 2000;
pub const SOURCE: &str = "autofix";

mod check;
mod rpc;
mod settings;
mod trial;

pub use check::*;
pub use rpc::*;
pub use settings::*;
pub use trial::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixState {
    Checking,
    /// A working route was found and waits for the user.
    Found,
    /// A rule is on trial.
    Trial,
    /// The trial passed; the rule is permanent.
    Pinned,
    /// The trial failed and the rule was removed; `target` is the next
    /// working route, if any.
    RolledBack,
    /// Nothing works, the direct route included: not a routing problem.
    Unreachable,
    /// Routed by an explicit decision autofix does not override: a strict
    /// group, the user's rule, a subscription rule naming a group.
    Protected,
    /// The original route worked again at the end of the trial; the rule
    /// was removed instead of kept.
    Reverted,
    /// The user said "not now" or undid the fix.
    Dismissed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fix {
    pub host: String,
    pub port: Option<u16>,
    pub apps: Vec<String>,
    /// Kind of the latest failure.
    pub kind: ErrorKind,
    /// Failures that triggered the check.
    pub failures: u32,
    /// Route at the time, e.g. `DIRECT` via `Match`.
    pub was: String,
    pub was_rule: String,
    pub state: FixState,
    /// Latest check, best route first.
    pub results: Vec<Outcome>,
    pub checked_at: Option<i64>,
    /// Route applied (trial, pinned) or proposed (found, rolled back).
    pub target: Option<String>,
    pub rule_id: Option<i64>,
    pub rule: Option<String>,
    /// Why a protected fix is left alone.
    #[serde(default)]
    pub protect: Option<Protect>,
    pub applied_at: Option<i64>,
    pub ends_at: Option<i64>,
    pub trial_ok: u32,
    pub trial_failed: u32,
    #[serde(default)]
    trial_last_fail: i64,
    /// Routes that failed their trial.
    pub tried: Vec<String>,
    /// Quiet (dismissed, unreachable) until this time.
    pub until: Option<i64>,
    pub error: Option<String>,
    pub updated: i64,
}

impl Fix {
    fn active(&self) -> bool {
        matches!(self.state, FixState::Checking | FixState::Found | FixState::Trial)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Protect {
    /// `strict`, `rule` or `subscription`.
    pub reason: String,
    /// The group, or the rule as a line.
    pub by: String,
}

#[derive(Debug, Default)]
struct Track {
    failures: VecDeque<i64>,
    last_ok: Option<i64>,
    port: Option<u16>,
    apps: BTreeSet<String>,
    kind: Option<ErrorKind>,
    target: String,
    rule: String,
}

/// What [`Engine::observe`] asks the caller to do.
#[derive(Debug, PartialEq)]
pub enum Next {
    Check(String),
    RollBack(String),
}

pub struct Engine {
    path: PathBuf,
    settings_path: PathBuf,
    pub settings: Automation,
    fixes: BTreeMap<String, Fix>,
    tracks: HashMap<String, Track>,
}

impl Engine {
    pub fn load(root: &Path) -> Self {
        let path = root.join("autofix.json");
        let settings_path = root.join("automation.json");
        let read = |p: &Path| std::fs::read_to_string(p).ok();
        let settings = read(&settings_path).and_then(|t| serde_json::from_str::<Automation>(&t).ok()).unwrap_or_default().sane();
        let fixes = read(&path).and_then(|t| serde_json::from_str::<BTreeMap<String, Fix>>(&t).ok()).unwrap_or_default();
        let mut fixes = fixes;
        // A check cut short by a restart starts over on the next failures.
        fixes.retain(|_, f| f.state != FixState::Checking);
        Self { path, settings_path, settings, fixes, tracks: HashMap::new() }
    }

    #[cfg(test)]
    fn in_memory() -> Self {
        Self { path: PathBuf::new(), settings_path: PathBuf::new(), settings: Automation::default(), fixes: BTreeMap::new(), tracks: HashMap::new() }
    }

    fn save(&self) {
        if self.path.as_os_str().is_empty() {
            return;
        }
        let text = serde_json::to_vec_pretty(&self.fixes).expect("fixes serialize");
        if let Err(e) = omm_config::paths::write_atomic(&self.path, &text) {
            tracing::warn!("saving autofix state: {e}");
        }
    }

    pub fn set_settings(&mut self, s: Automation) -> Result<Automation, RpcError> {
        let s = s.sane();
        if !self.settings_path.as_os_str().is_empty() {
            let text = serde_json::to_vec_pretty(&s).expect("settings serialize");
            omm_config::paths::write_atomic(&self.settings_path, &text).map_err(|e| RpcError::new(ErrorCode::Internal, e.to_string()))?;
        }
        self.settings = s.clone();
        Ok(s)
    }

    pub fn list(&self) -> Vec<Fix> {
        let mut out: Vec<Fix> = self.fixes.values().filter(|f| f.state != FixState::Dismissed).cloned().collect();
        out.sort_by_key(|f| std::cmp::Reverse(f.updated));
        out
    }

    pub fn get(&self, host: &str) -> Option<Fix> {
        self.fixes.get(host).cloned()
    }

    fn update(&mut self, host: &str, now: i64, f: impl FnOnce(&mut Fix)) -> Option<Fix> {
        let fix = self.fixes.get_mut(host)?;
        f(fix);
        fix.updated = now;
        let out = fix.clone();
        self.save();
        Some(out)
    }

    /// Feed one log line. Returns what should happen next, if anything.
    pub fn observe(&mut self, line: &Line, now: i64) -> Option<Next> {
        if !self.settings.autofix {
            return None;
        }
        match line {
            Line::Connected(c) => {
                let host = c.dest.host.to_ascii_lowercase();
                if let Some(t) = self.tracks.get_mut(&host) {
                    t.last_ok = Some(now);
                    t.failures.clear();
                }
                let fix = self.fixes.get_mut(&host)?;
                if fix.state == FixState::Trial
                    && fix.target.as_deref().is_some_and(|t| via(&c.chain, t))
                {
                    fix.trial_ok += 1;
                }
                None
            }
            Line::DialError(e) => {
                let host = e.dest.host.to_ascii_lowercase();
                if e.source.inner || host.is_empty() || local(&host) {
                    return None;
                }
                let kind = classify(&e.error, &e.route.target);
                if matches!(
                    kind,
                    ErrorKind::DnsFail | ErrorKind::DnsStub | ErrorKind::Other
                ) || self.settings.ignores(&host, e.source.process.as_deref())
                {
                    return None;
                }
                let (failures, window) = (self.settings.failures, self.settings.window_ms());
                if let Some(fix) = self.fixes.get_mut(&host) {
                    if fix.state == FixState::Trial {
                        if fix.target.as_deref() == Some(e.route.target.as_str())
                            && now - fix.trial_last_fail >= MERGE_MS
                        {
                            fix.trial_last_fail = now;
                            fix.trial_failed += 1;
                            if fix.trial_failed >= failures {
                                return Some(Next::RollBack(host));
                            }
                        }
                        return None;
                    }
                    let quiet = match fix.state {
                        FixState::Checking | FixState::Found | FixState::Pinned => true,
                        _ => fix.until.is_some_and(|u| u > now),
                    };
                    if quiet {
                        return None;
                    }
                }
                if self.tracks.len() >= MAX_TRACKED && !self.tracks.contains_key(&host) {
                    self.prune(now);
                }
                let t = self.tracks.entry(host.clone()).or_default();
                t.port = Some(e.dest.port);
                if let Some(p) = &e.source.process {
                    t.apps.insert(p.clone());
                }
                t.kind = Some(kind);
                t.target.clone_from(&e.route.target);
                t.rule = if e.route.payload.is_empty() {
                    e.route.rule.clone()
                } else {
                    format!("{}({})", e.route.rule, e.route.payload)
                };
                if t.failures.back().is_none_or(|last| now - last >= MERGE_MS) {
                    t.failures.push_back(now);
                }
                while t.failures.front().is_some_and(|f| now - f > window) {
                    t.failures.pop_front();
                }
                if t.failures.len() < failures as usize
                    || t.last_ok.is_some_and(|ok| now - ok < window)
                {
                    return None;
                }
                let fix = Fix {
                    host: host.clone(),
                    port: t.port,
                    apps: t.apps.iter().cloned().collect(),
                    kind,
                    failures: t.failures.len() as u32,
                    was: t.target.clone(),
                    was_rule: t.rule.clone(),
                    state: FixState::Checking,
                    results: Vec::new(),
                    checked_at: None,
                    target: None,
                    rule_id: None,
                    rule: None,
                    protect: None,
                    applied_at: None,
                    ends_at: None,
                    trial_ok: 0,
                    trial_failed: 0,
                    trial_last_fail: 0,
                    tried: self
                        .fixes
                        .get(&host)
                        .map(|f| f.tried.clone())
                        .unwrap_or_default(),
                    until: None,
                    error: None,
                    updated: now,
                };
                self.tracks.remove(&host);
                self.fixes.insert(host.clone(), fix);
                Some(Next::Check(host))
            }
        }
    }

    /// Failures around a sleep or a network change say nothing about
    /// routes: start counting afresh.
    pub fn forget_failures(&mut self) {
        self.tracks.clear();
        for fix in self.fixes.values_mut().filter(|f| f.state == FixState::Trial) {
            fix.trial_failed = 0;
            fix.trial_last_fail = 0;
        }
    }

    /// The machine slept `ms`: a trial measures time awake.
    pub fn postpone_trials(&mut self, ms: i64) {
        let mut moved = false;
        for fix in self.fixes.values_mut().filter(|f| f.state == FixState::Trial) {
            if let Some(end) = fix.ends_at.as_mut() {
                *end += ms;
                moved = true;
            }
        }
        if moved {
            self.save();
        }
    }

    /// Rules on trial: the trial, not their expiry, decides their fate.
    pub fn trial_rules(&self) -> Vec<i64> {
        self.fixes.values().filter(|f| f.state == FixState::Trial).filter_map(|f| f.rule_id).collect()
    }

    fn prune(&mut self, now: i64) {
        let window = self.settings.window_ms();
        self.tracks.retain(|_, t| t.failures.back().is_some_and(|f| now - f <= window));
        if self.tracks.len() >= MAX_TRACKED {
            self.tracks.clear();
        }
    }

    /// Drop finished fixes after a day.
    fn sweep(&mut self, now: i64) {
        let before = self.fixes.len();
        self.fixes.retain(|_, f| {
            f.active() || now - f.updated < KEEP_MS || f.until.is_some_and(|u| u > now)
        });
        if self.fixes.len() != before {
            self.save();
        }
    }
}

/// `chain` is `Group[Node]`, `DIRECT`...
fn via(chain: &str, target: &str) -> bool {
    chain == target || chain.strip_prefix(target).is_some_and(|r| r.starts_with('['))
}

/// Loopback, LAN and link-local addresses, `.local` and bare names.
fn local(host: &str) -> bool {
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => {
            ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified()
        }
        Ok(IpAddr::V6(ip)) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || (ip.segments()[0] & 0xfe00) == 0xfc00
                || (ip.segments()[0] & 0xffc0) == 0xfe80
        }
        Err(_) => !host.contains('.') || host.ends_with(".local") || host.ends_with(".lan"),
    }
}

/// The rule a fix adds: the host and its subdomains, or the address.
fn rule_for(host: &str) -> (MatchKind, String) {
    match host.parse::<IpAddr>() {
        Ok(_) => (MatchKind::IpCidr, host.to_owned()),
        Err(_) => (MatchKind::DomainSuffix, host.to_owned()),
    }
}

// ------------------------------------------------------------------ actions

fn now() -> i64 {
    crate::problems::now_ms()
}

fn publish(state: &State, fix: &Fix) {
    state.publish(omm_proto::topic::AUTOFIX, fix);
}

fn notice(state: &State, fix: &Fix) {
    let urgency = if matches!(fix.state, FixState::Pinned | FixState::Reverted) {
        "low"
    } else {
        "normal"
    };
    state.notify(
        &format!("autofix:{}", fix.host),
        "autofix",
        urgency,
        json!(fix),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use omm_problems::parse;

    fn err(host: &str, target: &str, error: &str) -> Line {
        parse::parse(&format!("[TCP] dial {target} (match Match/) 198.18.0.1:38164(firefox, uid=1000) --> {host}:443 error: {error}")).unwrap()
    }

    fn ok(host: &str, chain: &str) -> Line {
        parse::parse(&format!("[TCP] 198.18.0.1:48082(firefox, uid=1000) --> {host}:443 match DomainSuffix({host}) using {chain}")).unwrap()
    }

    const TIMEOUT: &str = "dial tcp 1.2.3.4:443: i/o timeout";

    #[test]
    fn three_spread_failures_without_success_trigger_a_check() {
        let mut e = Engine::in_memory();
        let line = err("chatgpt.com", "DIRECT", TIMEOUT);
        assert_eq!(e.observe(&line, 0), None);
        assert_eq!(e.observe(&line, 2_000), None, "retries within 10 s are one failure");
        assert_eq!(e.observe(&line, 20_000), None);
        assert_eq!(e.observe(&line, 40_000), Some(Next::Check("chatgpt.com".into())));
        let fix = e.get("chatgpt.com").unwrap();
        assert_eq!((fix.state, fix.was.as_str(), fix.failures), (FixState::Checking, "DIRECT", 3));
        assert_eq!(e.observe(&line, 60_000), None, "one check at a time");
    }

    #[test]
    fn successes_dns_and_local_hosts_do_not_trigger() {
        let mut e = Engine::in_memory();
        for t in [0, 20_000] {
            e.observe(&err("x.org", "DIRECT", TIMEOUT), t);
        }
        e.observe(&ok("x.org", "DIRECT"), 30_000);
        assert_eq!(e.observe(&err("x.org", "DIRECT", TIMEOUT), 40_000), None, "a success resets the count");
        for t in [0, 20_000, 40_000, 60_000] {
            assert_eq!(e.observe(&err("y.org", "DIRECT", "dns resolve failed: couldn't find ip"), t), None);
            assert_eq!(e.observe(&err("192.168.1.5", "DIRECT", TIMEOUT), t), None);
        }
        e.settings.ignore = vec!["*.bank.ru".into()];
        for t in [0, 20_000, 40_000, 60_000] {
            assert_eq!(e.observe(&err("online.bank.ru", "DIRECT", TIMEOUT), t), None);
        }
    }

    #[test]
    fn old_failures_fall_out_of_the_window() {
        let mut e = Engine::in_memory();
        let line = err("z.org", "DIRECT", TIMEOUT);
        e.observe(&line, 0);
        e.observe(&line, 20_000);
        assert_eq!(e.observe(&line, 6 * 60_000), None);
        assert_eq!(e.observe(&line, 6 * 60_000 + 20_000), None, "only two inside the window");
        assert_eq!(e.observe(&line, 6 * 60_000 + 40_000), Some(Next::Check("z.org".into())));
    }

    #[test]
    fn groups_resolve_to_their_node() {
        let p: mihomo_api::model::Proxies = serde_json::from_value(json!({ "proxies": {
            "AI": { "name": "AI", "type": "Selector", "now": "AUTO" },
            "AUTO": { "name": "AUTO", "type": "URLTest", "now": "FI" },
            "FI": { "name": "FI", "type": "Vless" },
            "LOOP": { "name": "LOOP", "type": "Selector", "now": "LOOP" }
        }}))
        .unwrap();
        assert_eq!(node_of(&p, "AI").as_deref(), Some("FI"));
        assert_eq!(node_of(&p, "FI").as_deref(), Some("FI"));
        assert_eq!(node_of(&p, "LOOP"), None);
        assert_eq!(node_of(&p, "GONE"), None);
    }

    #[test]
    fn network_changes_reset_counts_and_sleep_extends_trials() {
        let mut e = Engine::in_memory();
        let line = err("x.org", "DIRECT", TIMEOUT);
        e.observe(&line, 0);
        e.observe(&line, 20_000);
        e.forget_failures();
        assert_eq!(e.observe(&line, 40_000), None, "failures before the change are forgotten");
        for t in [0, 20_000, 40_000] {
            e.observe(&err("chatgpt.com", "DIRECT", TIMEOUT), t);
        }
        e.update("chatgpt.com", 50_000, |f| {
            f.state = FixState::Trial;
            f.target = Some("AI".into());
            f.rule_id = Some(7);
            f.ends_at = Some(100_000);
            f.trial_failed = 2;
        });
        e.postpone_trials(3_600_000);
        e.forget_failures();
        let fix = e.get("chatgpt.com").unwrap();
        assert_eq!((fix.ends_at, fix.trial_failed), (Some(3_700_000), 0));
        assert_eq!(e.trial_rules(), vec![7]);
    }

    #[test]
    fn trial_counts_route_failures_and_successes() {
        let mut e = Engine::in_memory();
        let line = err("chatgpt.com", "DIRECT", TIMEOUT);
        for t in [0, 20_000, 40_000] {
            e.observe(&line, t);
        }
        e.update("chatgpt.com", 50_000, |f| {
            f.state = FixState::Trial;
            f.target = Some("AI".into());
        });
        e.observe(&ok("chatgpt.com", "AI[Finland 1]"), 60_000);
        e.observe(&ok("chatgpt.com", "AIR[x]"), 61_000);
        assert_eq!(e.get("chatgpt.com").unwrap().trial_ok, 1);
        let via_ai = err("chatgpt.com", "AI", TIMEOUT);
        assert_eq!(e.observe(&line, 70_000), None, "the old route does not count");
        assert_eq!(e.observe(&via_ai, 80_000), None);
        assert_eq!(e.observe(&via_ai, 100_000), None);
        assert_eq!(e.observe(&via_ai, 120_000), Some(Next::RollBack("chatgpt.com".into())));
    }

    fn stored(
        kind: MatchKind,
        value: &str,
        target: &str,
        source: &str,
    ) -> crate::user_rules::Stored {
        crate::user_rules::Stored {
            rule: UserRule {
                id: 1,
                kind,
                value: value.into(),
                target: target.into(),
                enabled: true,
            },
            note: None,
            source: source.into(),
            created: 0,
            updated: 0,
            test: None,
            expires: None,
            boot: None,
        }
    }

    fn failing(host: &str, was: &str, was_rule: &str, app: &str) -> Fix {
        let mut e = Engine::in_memory();
        let line = parse::parse(&format!("[TCP] dial {was} (match {was_rule}) 198.18.0.1:38164({app}, uid=1000) --> {host}:443 error: {TIMEOUT}")).unwrap();
        for t in [0, 20_000, 40_000] {
            e.observe(&line, t);
        }
        e.get(host).expect("triggered")
    }

    #[test]
    fn explicit_decisions_are_protected() {
        let s = Automation {
            strict_groups: vec!["WORK".into()],
            ..Default::default()
        };
        let by = |f: &Fix, rules: &[crate::user_rules::Stored]| {
            protection(f, &s, rules).map(|p| (p.reason, p.by))
        };

        let work = failing(
            "jira.corp.com",
            "WORK",
            "DomainSuffix/corp.com",
            "slack-work",
        );
        assert_eq!(by(&work, &[]).unwrap().0, "strict");

        // A process rule must win over the host: the bug a domain-only check had.
        let slack = failing("edgeapi.slack.com", "PROXY", "ProcessName/slack", "slack");
        let rules = [stored(MatchKind::ProcessName, "slack", "PROXY", "user")];
        assert_eq!(
            by(&slack, &rules),
            Some(("rule".into(), "PROCESS-NAME,slack,PROXY".into()))
        );

        let sub = failing("x.com", "PROXY", "DomainSuffix/x.com", "firefox");
        assert_eq!(by(&sub, &[]).unwrap().0, "subscription");

        let plain = failing("chatgpt.com", "DIRECT", "Match/", "firefox");
        assert_eq!(by(&plain, &[]), None);
        assert_eq!(
            by(
                &plain,
                &[stored(MatchKind::DomainSuffix, "chatgpt.com", "AI", SOURCE)]
            ),
            None,
            "autofix's own rules do not protect"
        );
        assert_eq!(
            by(
                &failing("ya.ru", "DIRECT", "GeoSite/category-ru", "firefox"),
                &[]
            ),
            None,
            "DIRECT by a subscription is fixable"
        );
    }

    #[test]
    fn strict_and_excluded_groups_are_not_routes() {
        let s = Automation {
            strict_groups: vec!["WORK".into()],
            exclude_groups: vec!["TikTok".into()],
            ..Default::default()
        };
        assert!(s.usable("AI"));
        assert!(!s.usable("WORK") && !s.usable("TikTok") && !s.usable("DIRECT"));
    }

    #[test]
    fn ignored_processes() {
        let mut e = Engine::in_memory();
        e.settings.ignore = vec![
            "process:/opt/corp/slack-work".into(),
            "process:".into(),
            "process:/opt/*".into(),
        ];
        e.settings = e.settings.clone().sane();
        assert_eq!(e.settings.ignore.len(), 1);
        let line = parse::parse(&format!("[TCP] dial DIRECT (match Match/) 198.18.0.1:1(slack-work, uid=1000) --> a.corp.com:443 error: {TIMEOUT}")).unwrap();
        for t in [0, 20_000, 40_000, 60_000] {
            assert_eq!(e.observe(&line, t), None);
        }
    }

    #[test]
    fn ignore_patterns() {
        let s = Automation {
            ignore: vec![
                "*.lan".into(),
                "bank.ru".into(),
                "10.0.0.0/8".into(),
                "".into(),
            ],
            ..Default::default()
        }
        .sane();
        assert_eq!(s.ignore.len(), 3);
        assert!(
            s.ignores("nas.lan", None)
                && s.ignores("bank.ru", None)
                && s.ignores("my.bank.ru", None)
                && s.ignores("10.1.2.3", None)
        );
        assert!(!s.ignores("notbank.ru", None) && !s.ignores("11.0.0.1", None));
    }
}
