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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Check and show the result in the problem list only.
    Show,
    /// Check and notify; change nothing until the user says so.
    Suggest,
    /// Put the best route on trial right away.
    Trial,
}

/// What the notifications cover; the daemon publishes nothing else.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Notify {
    pub core: bool,
    pub dead_group: bool,
    pub subscription: bool,
    pub orphans: bool,
    pub autofix: bool,
    pub saved: bool,
    /// A strict group went down or came back.
    pub strict: bool,
    /// An app moved far more than usual or started using a proxy.
    pub traffic: bool,
    /// A newer core is out.
    pub updates: bool,
}

impl Default for Notify {
    fn default() -> Self {
        Self {
            core: true,
            dead_group: true,
            subscription: true,
            orphans: true,
            autofix: true,
            saved: true,
            strict: true,
            traffic: true,
            updates: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Automation {
    pub autofix: bool,
    pub mode: Mode,
    /// Failures within `window_min` without a single success.
    pub failures: u32,
    pub window_min: u32,
    pub trial_min: u32,
    /// Groups never used for a fix.
    pub exclude_groups: Vec<String>,
    /// Groups whose traffic never moves elsewhere and never receives
    /// other traffic: a work VPN that must not leak into a personal one.
    pub strict_groups: Vec<String>,
    /// What autofix leaves alone: `example.org` (and subdomains), `*.lan`,
    /// `192.168.0.0/16`, `process:slack`.
    pub ignore: Vec<String>,
    pub notify: Notify,
    /// Test nodes the core leaves untested, for their latency history.
    pub probe_nodes: bool,
}

impl Default for Automation {
    fn default() -> Self {
        Self {
            autofix: true,
            mode: Mode::Trial,
            failures: 3,
            window_min: 5,
            trial_min: 60,
            exclude_groups: Vec::new(),
            strict_groups: Vec::new(),
            ignore: Vec::new(),
            notify: Notify::default(),
            probe_nodes: true,
        }
    }
}

impl Automation {
    fn sane(mut self) -> Self {
        self.failures = self.failures.clamp(1, 50);
        self.window_min = self.window_min.clamp(1, 120);
        self.trial_min = self.trial_min.clamp(5, 24 * 60);
        self.ignore.retain(|p| Ignore::parse(p).is_some());
        self
    }

    fn window_ms(&self) -> i64 {
        i64::from(self.window_min) * 60_000
    }

    fn trial_ms(&self) -> i64 {
        i64::from(self.trial_min) * 60_000
    }

    fn ignores(&self, host: &str, process: Option<&str>) -> bool {
        let ip = host.parse::<IpAddr>().ok();
        self.ignore
            .iter()
            .filter_map(|p| Ignore::parse(p))
            .any(|i| match i {
                Ignore::Host(r) => r.matches_host(host, ip),
                Ignore::Process(name) => process.is_some_and(|p| p == name),
            })
    }

    /// Groups a fix may route through.
    fn usable(&self, group: &str) -> bool {
        group != "DIRECT"
            && !self.exclude_groups.iter().any(|g| g == group)
            && !self.strict_groups.iter().any(|g| g == group)
    }
}

enum Ignore {
    Host(UserRule),
    /// The core logs the executable's name, so a path counts by its name.
    Process(String),
}

impl Ignore {
    /// `*.lan` and `lan` both mean the domain and its subdomains.
    fn parse(pattern: &str) -> Option<Self> {
        let p = pattern.trim();
        if let Some(proc) = p.strip_prefix("process:") {
            let name = proc.trim().rsplit('/').next().unwrap_or_default();
            return (!name.is_empty() && !name.contains('*'))
                .then(|| Self::Process(name.to_owned()));
        }
        let p = p.trim_start_matches("*.").trim_start_matches('.');
        if p.is_empty() {
            return None;
        }
        let kind = if p.contains('/') || p.parse::<IpAddr>().is_ok() {
            MatchKind::IpCidr
        } else {
            MatchKind::DomainSuffix
        };
        let value = omm_config::rules::normalize(kind, p).ok()?;
        Some(Self::Host(UserRule {
            id: 0,
            kind,
            value,
            target: String::new(),
            enabled: true,
        }))
    }
}

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

/// Check a host through every group and act on the result.
pub async fn check(state: Arc<State>, host: String) {
    let Some(fix) = state.autofix.lock().unwrap().get(&host) else {
        return;
    };
    let settings = state.autofix.lock().unwrap().settings.clone();
    let rules = state
        .config
        .rules()
        .await
        .map(|(r, _)| r)
        .unwrap_or_default();
    if let Some(protect) = protection(&fix, &settings, &rules) {
        tracing::info!(
            host,
            reason = protect.reason,
            by = protect.by,
            "autofix: explicit route, left alone"
        );
        let t = now();
        let fix = state.autofix.lock().unwrap().update(&host, t, |f| {
            f.state = FixState::Protected;
            f.protect = Some(protect);
            f.until = Some(t + RECHECK_MS);
        });
        if let Some(fix) = fix {
            publish(&state, &fix);
        }
        return;
    }
    publish(&state, &fix);
    let mode = settings.mode;
    let proxies = state.core.proxies().await;
    let groups = match &proxies {
        Ok(p) => crate::methods::targets(p)
            .into_iter()
            .filter(|t| settings.usable(t))
            .collect::<Vec<_>>(),
        Err(e) => {
            tracing::debug!("autofix: listing groups: {e}");
            state.autofix.lock().unwrap().fixes.remove(&host);
            return;
        }
    };
    let mut targets: Vec<String> = groups
        .into_iter()
        .filter(|g| *g != fix.was && !fix.tried.contains(g))
        .take(31)
        .collect();
    if !fix.was.is_empty() {
        targets.push(fix.was.clone());
    }
    let probe = crate::route_check::probe_for(&host, fix.port);
    let mut results = crate::route_check::check(&state, &probe, &targets).await;
    // One probe can be lucky: among working routes, a node that lost fewer
    // tests over the last hours wins before a faster one.
    let loss = proxies.as_ref().map(|p| loss_tiers(&state, p, &targets)).unwrap_or_default();
    results.sort_by_key(|o| (!o.ok, o.target == fix.was, loss.get(&o.target).copied().unwrap_or(0), o.delay.unwrap_or(u32::MAX)));
    let t = now();
    let was_ok = results.iter().any(|o| o.ok && o.target == fix.was);
    let best = results
        .iter()
        .find(|o| o.ok && o.target != fix.was)
        .map(|o| o.target.clone());
    tracing::info!(
        host,
        best = best.as_deref().unwrap_or("-"),
        was_ok,
        "autofix checked"
    );

    if was_ok && best.is_none() {
        // The current route works again: a passing outage, nothing to do.
        state.autofix.lock().unwrap().update(&host, t, |f| {
            f.state = FixState::Dismissed;
            f.until = Some(t + RECHECK_MS);
        });
        return;
    }
    let Some(best) = best.filter(|_| !was_ok) else {
        let fix = state.autofix.lock().unwrap().update(&host, t, |f| {
            f.results = results;
            f.checked_at = Some(t);
            f.state = if was_ok { FixState::Dismissed } else { FixState::Unreachable };
            f.until = Some(t + RECHECK_MS);
        });
        if let Some(fix) = fix.filter(|f| f.state == FixState::Unreachable) {
            publish(&state, &fix);
            if mode != Mode::Show {
                notice(&state, &fix);
            }
        }
        return;
    };
    let fix = state.autofix.lock().unwrap().update(&host, t, |f| {
        f.results = results;
        f.checked_at = Some(t);
        f.state = FixState::Found;
        f.target = Some(best.clone());
    });
    let Some(fix) = fix else { return };
    match mode {
        Mode::Trial => {
            if let Err(e) = start_trial(&state, &host, &best, true).await {
                tracing::warn!(host, "autofix: applying {best}: {}", e.message);
                let fix = state.autofix.lock().unwrap().update(&host, now(), |f| f.error = Some(e.message.clone()));
                if let Some(fix) = fix {
                    publish(&state, &fix);
                    notice(&state, &fix);
                }
            }
        }
        Mode::Suggest => {
            publish(&state, &fix);
            notice(&state, &fix);
        }
        Mode::Show => publish(&state, &fix),
    }
}

/// Why autofix must not reroute this traffic, if it must not. Only
/// traffic nobody directed — the final `Match`, or DIRECT by default — is
/// fixed; an explicit decision stays, even while it fails.
fn protection(
    fix: &Fix,
    settings: &Automation,
    rules: &[crate::user_rules::Stored],
) -> Option<Protect> {
    if settings.strict_groups.contains(&fix.was) {
        return Some(Protect {
            reason: "strict".into(),
            by: fix.was.clone(),
        });
    }
    // The user's own rule, by any match the log lets us check: the rule the
    // core reported, or the host, process and port of the connection.
    let ip = fix.host.parse::<IpAddr>().ok();
    let conn = omm_config::rules::Conn {
        host: &fix.host,
        ip,
        dst_port: fix.port,
        process: fix.apps.first().map(String::as_str).unwrap_or_default(),
        ..Default::default()
    };
    let reported = fix.was_rule.to_ascii_lowercase();
    let own = rules
        .iter()
        .filter(|s| s.rule.enabled && s.source != SOURCE)
        .find(|s| {
            format!("{}({})", s.rule.core_type(), s.rule.value).to_ascii_lowercase() == reported
                || s.rule.matches(&conn) == Some(true)
        });
    if let Some(s) = own {
        return Some(Protect {
            reason: "rule".into(),
            by: s.rule.line(),
        });
    }
    let rule = fix.was_rule.split('(').next().unwrap_or_default();
    if !matches!(rule, "" | "Match") && fix.was != "DIRECT" {
        return Some(Protect {
            reason: "subscription".into(),
            by: format!("{} → {}", fix.was_rule, fix.was),
        });
    }
    None
}

/// The node a group routes through now, following nested groups.
fn node_of(p: &mihomo_api::model::Proxies, target: &str) -> Option<String> {
    let mut name = target.to_owned();
    for _ in 0..8 {
        let proxy = p.proxies.get(&name)?;
        match &proxy.now {
            Some(next) if proxy.is_group() => name = next.clone(),
            _ => return (!proxy.is_group()).then_some(name),
        }
    }
    None
}

/// Each target's share of failed delay tests over the last 6 hours, in
/// 10% steps so that noise does not outweigh speed.
fn loss_tiers(state: &State, p: &mihomo_api::model::Proxies, targets: &[String]) -> HashMap<String, u32> {
    let nodes: Vec<(String, String)> = targets.iter().filter_map(|t| node_of(p, t).map(|n| (t.clone(), n))).collect();
    let q = crate::latency::Query { hours: 6, points: 1, nodes: Some(nodes.iter().map(|(_, n)| n.clone()).collect()) };
    let Ok(views) = state.latency.lock().unwrap().query(&q, now()) else { return HashMap::new() };
    nodes
        .into_iter()
        .filter_map(|(t, n)| {
            let v = views.get(&n).filter(|v| v.tests >= 5)?;
            Some((t, (v.failures * 10 + v.tests / 2) / v.tests))
        })
        .collect()
}

/// Add the rule for `target` with a trial expiry.
/// `announce`: send a notice too (not when the user asked for it).
pub async fn start_trial(
    state: &State,
    host: &str,
    target: &str,
    announce: bool,
) -> Result<Fix, RpcError> {
    let trial = state.autofix.lock().unwrap().settings.trial_ms();
    let fix =
        state.autofix.lock().unwrap().get(host).ok_or_else(|| {
            RpcError::new(ErrorCode::NotFound, format!("nothing known about {host}"))
        })?;
    let t = now();
    let (kind, raw) = rule_for(host);
    let value = omm_config::rules::normalize(kind, &raw)
        .map_err(|e| RpcError::new(ErrorCode::BadRequest, e.to_string()))?;
    let measured = fix.results.iter().find(|o| o.target == target);
    let new = crate::user_rules::NewRule {
        kind,
        value,
        target: target.to_owned(),
        note: None,
        source: SOURCE.into(),
        test: measured.map(|o| crate::user_rules::Check {
            target: o.target.clone(),
            ok: o.ok,
            delay: o.delay,
            error: o.error.clone(),
            at: fix.checked_at.unwrap_or(t),
        }),
        lifetime: crate::user_rules::Lifetime {
            expires: Some(t + trial + GRACE_MS),
            until_reboot: false,
        },
    };
    let (stored, report) = state
        .config
        .change_rules(state, true, |store| {
            let (s, c) = store.upsert(new, t).map_err(crate::config::db_err)?;
            Ok((s, vec![c]))
        })
        .await?;
    if report.reloaded {
        crate::route_check::close_matching(state, &stored.rule).await;
    }
    let fix = state
        .autofix
        .lock()
        .unwrap()
        .update(host, t, |f| {
            f.state = FixState::Trial;
            f.target = Some(target.to_owned());
            f.rule_id = Some(stored.rule.id);
            f.rule = Some(stored.rule.line());
            f.applied_at = Some(t);
            f.ends_at = Some(t + trial);
            f.trial_ok = 0;
            f.trial_failed = 0;
            f.until = None;
            f.error = None;
        })
        .ok_or_else(|| RpcError::new(ErrorCode::NotFound, host.to_owned()))?;
    publish(state, &fix);
    if announce {
        notice(state, &fix);
    }
    Ok(fix)
}

async fn delete_rule(state: &State, id: i64) -> Result<(), RpcError> {
    let t = now();
    let (rule, report) = state
        .config
        .change_rules(state, false, |store| {
            let before = store.get(id).map_err(crate::config::db_err)?;
            let change = store.delete(id, t).map_err(crate::config::db_err)?;
            Ok((before, change.into_iter().collect()))
        })
        .await?;
    if report.reloaded
        && let Some(rule) = rule
    {
        crate::route_check::close_matching(state, &rule.rule).await;
    }
    Ok(())
}

/// The trial route failed too: remove the rule, look for another route.
pub async fn roll_back(state: Arc<State>, host: String) {
    let Some(fix) = state.autofix.lock().unwrap().get(&host) else {
        return;
    };
    if fix.state != FixState::Trial {
        return;
    }
    if let Some(id) = fix.rule_id
        && let Err(e) = delete_rule(&state, id).await
    {
        tracing::warn!(host, "autofix: removing rule: {}", e.message);
    }
    let failed = fix.target.clone().unwrap_or_default();
    tracing::info!(host, failed, "autofix: trial failed, rule removed");
    let settings = state.autofix.lock().unwrap().settings.clone();
    let groups = state
        .core
        .proxies()
        .await
        .map(|p| crate::methods::targets(&p))
        .unwrap_or_default();
    let mut tried = fix.tried.clone();
    tried.push(failed.clone());
    let targets: Vec<String> = groups
        .into_iter()
        .filter(|g| settings.usable(g) && *g != fix.was && !tried.contains(g))
        .take(32)
        .collect();
    let mut results = if targets.is_empty() {
        Vec::new()
    } else {
        crate::route_check::check(
            &state,
            &crate::route_check::probe_for(&host, fix.port),
            &targets,
        )
        .await
    };
    results.sort_by_key(|o| (!o.ok, o.delay.unwrap_or(u32::MAX)));
    let next = results.iter().find(|o| o.ok).map(|o| o.target.clone());
    let t = now();
    let fix = state.autofix.lock().unwrap().update(&host, t, |f| {
        f.state = FixState::RolledBack;
        f.tried = tried;
        f.rule_id = None;
        f.results = results;
        f.checked_at = Some(t);
        f.target = next;
        f.until = Some(t + QUIET_MS);
    });
    if let Some(fix) = fix {
        publish(&state, &fix);
        notice(&state, &fix);
    }
}

async fn pin(state: &State, host: &str) -> Result<Fix, RpcError> {
    let fix = state
        .autofix
        .lock()
        .unwrap()
        .get(host)
        .ok_or_else(|| RpcError::new(ErrorCode::NotFound, host.to_owned()))?;
    let id = fix
        .rule_id
        .ok_or_else(|| RpcError::new(ErrorCode::BadRequest, "no rule to pin"))?;
    let t = now();
    state
        .config
        .change_rules(state, false, |store| {
            match store
                .update(
                    id,
                    crate::user_rules::Patch {
                        lifetime: Some(Default::default()),
                        ..Default::default()
                    },
                    t,
                )
                .map_err(crate::config::db_err)?
            {
                Some((_, c)) => Ok(((), vec![c])),
                None => Err(RpcError::new(ErrorCode::NotFound, format!("no rule #{id}"))),
            }
        })
        .await?;
    let fix = state
        .autofix
        .lock()
        .unwrap()
        .update(host, t, |f| {
            f.state = FixState::Pinned;
            f.ends_at = Some(t);
        })
        .ok_or_else(|| RpcError::new(ErrorCode::NotFound, host.to_owned()))?;
    publish(state, &fix);
    Ok(fix)
}

/// Before keeping a fix for good, try the route it replaced: a short outage
/// must not leave a permanent detour. Returns true when the rule was removed.
async fn revert_if_recovered(state: &State, fix: &Fix) -> bool {
    if fix.was.is_empty() {
        return false;
    }
    let probe = crate::route_check::probe_for(&fix.host, fix.port);
    let was = crate::route_check::check(state, &probe, std::slice::from_ref(&fix.was)).await;
    if !was.first().is_some_and(|o| o.ok) {
        return false;
    }
    if let Some(id) = fix.rule_id
        && let Err(e) = delete_rule(state, id).await
    {
        tracing::warn!(host = fix.host, "autofix: removing rule: {}", e.message);
        return false;
    }
    tracing::info!(
        host = fix.host,
        was = fix.was,
        "autofix: original route works again, rule removed"
    );
    let t = now();
    let mut results = was;
    results.extend(
        fix.results
            .iter()
            .filter(|o| Some(&o.target) == fix.target.as_ref())
            .cloned(),
    );
    let fix = state.autofix.lock().unwrap().update(&fix.host, t, |f| {
        f.state = FixState::Reverted;
        f.rule_id = None;
        f.results = results;
        f.checked_at = Some(t);
        f.until = Some(t + RECHECK_MS);
    });
    if let Some(fix) = fix {
        publish(state, &fix);
        notice(state, &fix);
    }
    true
}

/// Pin trials that passed; drop fixes whose rule the user removed.
pub async fn watch(state: Arc<State>) {
    let mut tick = tokio::time::interval(Duration::from_secs(30));
    loop {
        tick.tick().await;
        // A route tried right after waking up would fail for no reason.
        if state.network_quiet() {
            continue;
        }
        let t = now();
        let trials: Vec<Fix> = {
            let mut a = state.autofix.lock().unwrap();
            a.sweep(t);
            a.fixes
                .values()
                .filter(|f| f.state == FixState::Trial)
                .cloned()
                .collect()
        };
        if trials.is_empty() {
            continue;
        }
        let Ok((rules, _)) = state.config.rules().await else {
            continue;
        };
        for fix in trials {
            let alive = fix
                .rule_id
                .and_then(|id| rules.iter().find(|s| s.rule.id == id))
                .is_some_and(|s| {
                    s.rule.enabled && s.rule.target == fix.target.clone().unwrap_or_default()
                });
            if !alive {
                // Edited or removed by the user: theirs now.
                state.autofix.lock().unwrap().update(&fix.host, t, |f| {
                    f.state = FixState::Dismissed;
                    f.until = Some(t + QUIET_MS);
                });
                continue;
            }
            if fix.ends_at.is_some_and(|e| e <= t) {
                if revert_if_recovered(&state, &fix).await {
                    continue;
                }
                match pin(&state, &fix.host).await {
                    Ok(fix) => {
                        tracing::info!(host = fix.host, "autofix: trial passed, rule pinned");
                        notice(&state, &fix);
                    }
                    Err(e) => tracing::warn!(host = fix.host, "autofix: pinning: {}", e.message),
                }
            }
        }
    }
}

// ------------------------------------------------------------------ RPC

pub async fn rpc(
    state: &Arc<State>,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, RpcError> {
    #[derive(Deserialize)]
    struct HostParam {
        host: String,
        target: Option<String>,
    }
    match method {
        "automation.get" => Ok(json!(state.autofix.lock().unwrap().settings)),
        "automation.set" => {
            let s: Automation = serde_json::from_value(params)
                .map_err(|e| RpcError::new(ErrorCode::BadRequest, e.to_string()))?;
            let saved = state.autofix.lock().unwrap().set_settings(s)?;
            Ok(json!(saved))
        }
        "autofix.list" => Ok(json!(state.autofix.lock().unwrap().list())),
        _ => {
            let p: HostParam = serde_json::from_value(params)
                .map_err(|e| RpcError::new(ErrorCode::BadRequest, e.to_string()))?;
            let host = p.host.trim().to_ascii_lowercase();
            let fix = state.autofix.lock().unwrap().get(&host);
            let t = now();
            let quiet = |state: &State, until: i64| {
                let fix = state.autofix.lock().unwrap().update(&host, t, |f| {
                    f.state = FixState::Dismissed;
                    f.until = Some(until);
                });
                if let Some(f) = &fix {
                    publish(state, f);
                }
                fix
            };
            match method {
                "autofix.get" => Ok(json!(fix)),
                "autofix.apply" => {
                    let fix = fix.ok_or_else(|| {
                        RpcError::new(ErrorCode::NotFound, format!("nothing known about {host}"))
                    })?;
                    let target = p.target.or(fix.target.clone()).ok_or_else(|| {
                        RpcError::new(ErrorCode::BadRequest, "no working route to apply")
                    })?;
                    if fix.state == FixState::Trial
                        && let Some(id) = fix.rule_id
                        && fix.target.as_deref() != Some(target.as_str())
                    {
                        delete_rule(state, id).await?;
                    }
                    Ok(json!(start_trial(state, &host, &target, false).await?))
                }
                "autofix.pin" => Ok(json!(pin(state, &host).await?)),
                // After a revert: keep the detour anyway, for good.
                "autofix.keep" => {
                    let fix =
                        fix.ok_or_else(|| RpcError::new(ErrorCode::NotFound, host.clone()))?;
                    let target = p
                        .target
                        .or(fix.target)
                        .ok_or_else(|| RpcError::new(ErrorCode::BadRequest, "no route to keep"))?;
                    start_trial(state, &host, &target, false).await?;
                    Ok(json!(pin(state, &host).await?))
                }
                "autofix.undo" => {
                    let fix =
                        fix.ok_or_else(|| RpcError::new(ErrorCode::NotFound, host.clone()))?;
                    if let Some(id) = fix.rule_id {
                        delete_rule(state, id).await?;
                    }
                    let fix = state.autofix.lock().unwrap().update(&host, t, |f| {
                        f.rule_id = None;
                        f.state = FixState::Dismissed;
                        f.until = Some(t + QUIET_MS);
                    });
                    if let Some(f) = &fix {
                        publish(state, f);
                    }
                    Ok(json!(fix))
                }
                "autofix.dismiss" => Ok(json!(quiet(state, t + QUIET_MS))),
                "autofix.recheck" => {
                    {
                        let mut a = state.autofix.lock().unwrap();
                        let f = a
                            .fixes
                            .get_mut(&host)
                            .ok_or_else(|| RpcError::new(ErrorCode::NotFound, host.clone()))?;
                        if f.state == FixState::Trial {
                            return Err(RpcError::new(
                                ErrorCode::BadRequest,
                                "a route is on trial",
                            ));
                        }
                        f.state = FixState::Checking;
                        f.tried.clear();
                    }
                    check(state.clone(), host.clone()).await;
                    Ok(json!(state.autofix.lock().unwrap().get(&host)))
                }
                "autofix.exclude" => {
                    if let Some(id) = fix
                        .as_ref()
                        .filter(|f| f.state == FixState::Trial)
                        .and_then(|f| f.rule_id)
                    {
                        delete_rule(state, id).await?;
                    }
                    let mut a = state.autofix.lock().unwrap();
                    let mut s = a.settings.clone();
                    if !s.ignore.contains(&host) {
                        s.ignore.push(host.clone());
                    }
                    a.set_settings(s)?;
                    a.fixes.remove(&host);
                    a.save();
                    Ok(json!({ "ok": true }))
                }
                _ => Err(RpcError::new(
                    ErrorCode::UnknownMethod,
                    format!("unknown method {method}"),
                )),
            }
        }
    }
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
