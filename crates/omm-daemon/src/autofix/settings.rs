//! What the user lets autofix do, and what it notifies about.

use super::*;

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
    pub(super) fn sane(mut self) -> Self {
        self.failures = self.failures.clamp(1, 50);
        self.window_min = self.window_min.clamp(1, 120);
        self.trial_min = self.trial_min.clamp(5, 24 * 60);
        self.ignore.retain(|p| Ignore::parse(p).is_some());
        self
    }

    pub(super) fn window_ms(&self) -> i64 {
        i64::from(self.window_min) * 60_000
    }

    pub(super) fn trial_ms(&self) -> i64 {
        i64::from(self.trial_min) * 60_000
    }

    pub(super) fn ignores(&self, host: &str, process: Option<&str>) -> bool {
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
    pub(super) fn usable(&self, group: &str) -> bool {
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
    pub(super) fn parse(pattern: &str) -> Option<Self> {
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
