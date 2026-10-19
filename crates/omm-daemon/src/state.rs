use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use omm_proto::{CoreStatus, Event, PROTOCOL_VERSION, Status, topic};
use serde::Serialize;
use tokio::sync::{Notify, broadcast, watch};

/// An event already serialized once, shared by every subscriber.
#[derive(Debug)]
pub struct Published {
    pub topic: &'static str,
    pub line: String,
}

pub struct State {
    pub session: crate::session::Session,
    pub core: mihomo_api::Client,
    pub system_proxy: tokio::sync::Mutex<crate::system_proxy::Manager>,
    pub config: crate::config::Manager,
    pub problems: crate::problems::Shared,
    pub logs: Mutex<crate::logs::Tail>,
    /// Hosts and processes logged recently, for rule suggestions.
    pub seen: Mutex<crate::seen::Seen>,
    status: watch::Sender<Status>,
    events: broadcast::Sender<Arc<Published>>,
    listeners: Mutex<HashMap<&'static str, usize>>,
    refresh: Notify,
    /// Last TUN failure the core logged; cleared once TUN is up.
    pub tun_error: Mutex<Option<String>>,
    pub core_install: Mutex<crate::health::Install>,
    operation_serial: Mutex<u64>,
    pub autofix: Mutex<crate::autofix::Engine>,
    /// Latest notices, newest last, one per key.
    notices: Mutex<VecDeque<Notice>>,
}

/// Something worth a desktop notification. Clients render the text; the
/// daemon only says what happened.
#[derive(Debug, Clone, Serialize)]
pub struct Notice {
    pub key: String,
    pub kind: String,
    /// `low`, `normal` or `critical`.
    pub urgency: &'static str,
    pub data: serde_json::Value,
    pub at: i64,
}

const MAX_NOTICES: usize = 50;

impl State {
    pub fn new(core: mihomo_api::Client, config: crate::config::Manager, problems: omm_problems::Collector, system_proxy: crate::system_proxy::Manager) -> Self {
        let autofix = crate::autofix::Engine::load(&config.paths().root);
        let status = Status {
            platform: omm_platform::NAME.into(),
            capabilities: vec!["profiles".into(), "rules".into(), "system_proxy".into(), "tun".into(), "process_rules".into()],
            helper: Default::default(),
            daemon_version: env!("CARGO_PKG_VERSION").into(),
            protocol: PROTOCOL_VERSION,
            core: CoreStatus { socket: core.socket().display().to_string(), ..Default::default() },
            core_operation: None,
            config: Default::default(),
            system_proxy: Default::default(),
            issues: Vec::new(),
        };
        Self {
            session: Default::default(),
            core,
            system_proxy: tokio::sync::Mutex::new(system_proxy),
            config,
            problems: std::sync::Mutex::new(problems),
            logs: Mutex::default(),
            seen: Mutex::default(),
            status: watch::channel(status).0,
            // Slow clients lag and skip events rather than stall the daemon.
            events: broadcast::channel(256).0,
            listeners: Mutex::default(),
            refresh: Notify::new(),
            tun_error: Mutex::default(),
            core_install: Mutex::default(),
            operation_serial: Mutex::default(),
            autofix: Mutex::new(autofix),
            notices: Mutex::default(),
        }
    }

    /// Publish a notice unless the user switched its category off.
    pub fn notify(&self, key: &str, kind: &str, urgency: &'static str, data: serde_json::Value) {
        let on = {
            let n = &self.autofix.lock().unwrap().settings.notify;
            match kind {
                "autofix" => n.autofix,
                "core_down" | "core_up" => n.core,
                "dead_group" | "group_alive" => n.dead_group,
                "subscription" => n.subscription,
                "orphans" => n.orphans,
                _ => true,
            }
        };
        if !on {
            return;
        }
        let notice = Notice { key: key.into(), kind: kind.into(), urgency, data, at: crate::problems::now_ms() };
        {
            let mut list = self.notices.lock().unwrap();
            list.retain(|n| n.key != key);
            list.push_back(notice.clone());
            while list.len() > MAX_NOTICES {
                list.pop_front();
            }
        }
        self.publish(topic::NOTICES, &notice);
    }

    pub fn notice(&self, key: &str) -> Option<Notice> {
        self.notices.lock().unwrap().iter().find(|n| n.key == key).cloned()
    }

    pub fn status(&self) -> Status {
        self.status.borrow().clone()
    }

    pub fn begin_operation(&self, kind: &str, detail: &str) -> CoreOperationGuard<'_> {
        let mut serial = self.operation_serial.lock().unwrap();
        *serial += 1;
        self.set_operation(Some(omm_proto::CoreOperation { kind: kind.into(), detail: detail.into(), ..Default::default() }));
        CoreOperationGuard { state: self, serial: *serial }
    }

    fn set_operation(&self, operation: Option<omm_proto::CoreOperation>) {
        if self.status.send_if_modified(|s| {
            if s.core_operation == operation { return false; }
            s.core_operation = operation;
            true
        }) {
            self.publish(topic::STATUS, &self.status());
        }
    }

    #[cfg(target_os = "macos")]
    pub fn set_helper_status(&self, helper: omm_proto::HelperStatus) {
        if self.status.send_if_modified(|s| {
            if s.helper == helper { return false; }
            s.helper = helper; true
        }) { self.publish(topic::STATUS, &self.status()); }
    }

    pub fn core_running(&self) -> bool {
        self.status.borrow().core.running
    }

    pub fn watch_status(&self) -> watch::Receiver<Status> {
        self.status.subscribe()
    }

    /// Store new config state; announce it only when something changed.
    pub fn set_config_status(&self, config: omm_proto::ConfigStatus) {
        let changed = self.status.send_if_modified(|s| {
            if s.config == config {
                return false;
            }
            s.config = config;
            true
        });
        if changed {
            self.publish(topic::STATUS, &self.status());
        }
    }

    /// Store the current setup issues; announce them only when they changed.
    pub fn set_issues(&self, issues: Vec<omm_proto::Issue>) {
        let changed = self.status.send_if_modified(|s| {
            if s.issues == issues {
                return false;
            }
            s.issues = issues;
            true
        });
        if changed {
            self.publish(topic::STATUS, &self.status());
        }
    }

    /// Store new core state; announce it only when something changed.
    pub fn set_core_status(&self, core: CoreStatus) {
        let changed = self.status.send_if_modified(|s| {
            if s.core == core {
                return false;
            }
            s.core = core;
            true
        });
        if changed {
            self.publish(topic::STATUS, &self.status());
        }
    }

    pub fn set_system_proxy_status(&self, proxy: omm_proto::SystemProxyStatus) {
        let changed = self.status.send_if_modified(|s| {
            if s.system_proxy == proxy {
                return false;
            }
            s.system_proxy = proxy;
            true
        });
        if changed {
            self.publish(topic::STATUS, &self.status());
        }
    }

    pub fn publish(&self, topic: &'static str, data: &impl Serialize) {
        if self.listeners(topic) == 0 {
            return;
        }
        let event = Event { event: topic.into(), data: serde_json::to_value(data).unwrap_or_default() };
        let line = serde_json::to_string(&event).expect("event serializes");
        let _ = self.events.send(Arc::new(Published { topic, line }));
    }

    pub fn listeners(&self, topic: &str) -> usize {
        self.listeners.lock().unwrap().get(topic).copied().unwrap_or(0)
    }

    /// Start receiving `topics`; counts are released when the guard drops.
    pub fn subscribe(self: &Arc<Self>, topics: Vec<&'static str>) -> Subscription {
        let mut l = self.listeners.lock().unwrap();
        for t in &topics {
            *l.entry(t).or_default() += 1;
        }
        Subscription { rx: self.events.subscribe(), topics, state: self.clone() }
    }

    /// Ask the core watcher to re-read state now (after a mutation).
    pub fn request_refresh(&self) {
        self.refresh.notify_one();
    }

    pub async fn refresh_requested(&self) {
        self.refresh.notified().await
    }
}

/// Clears transient state on success, error and cancellation. A completed
/// older request must never clear a newer operation's status.
pub struct CoreOperationGuard<'a> {
    state: &'a State,
    serial: u64,
}

impl CoreOperationGuard<'_> {
    pub fn progress(&self, downloaded: u64, total: Option<u64>) {
        let serial = self.state.operation_serial.lock().unwrap();
        if *serial != self.serial { return; }
        if let Some(mut op) = self.state.status().core_operation {
            op.downloaded = downloaded;
            op.total = total;
            self.state.set_operation(Some(op));
        }
    }
}

impl Drop for CoreOperationGuard<'_> {
    fn drop(&mut self) {
        let serial = self.state.operation_serial.lock().unwrap();
        if *serial == self.serial { self.state.set_operation(None); }
    }
}

pub struct Subscription {
    pub rx: broadcast::Receiver<Arc<Published>>,
    pub topics: Vec<&'static str>,
    state: Arc<State>,
}

impl Subscription {
    pub fn wants(&self, topic: &str) -> bool {
        self.topics.contains(&topic)
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        let mut l = self.state.listeners.lock().unwrap();
        for t in &self.topics {
            if let Some(n) = l.get_mut(t) {
                *n = n.saturating_sub(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_progress_survives_probes_and_old_completion() {
        let dir = tempfile::tempdir().unwrap();
        let paths = omm_config::paths::Paths::new(dir.path());
        paths.ensure().unwrap();
        let state = State::new(
            mihomo_api::Client::new(dir.path().join("core.sock")),
            crate::config::Manager::open(paths, None).unwrap(),
            omm_problems::Collector::open(&dir.path().join("state.db")).unwrap(),
            crate::system_proxy::Manager::open(dir.path().join("proxy.json")).unwrap(),
        );
        let older = state.begin_operation("starting", "");
        let download = state.begin_operation("downloading", "v1.19.32");
        download.progress(14, Some(22));
        drop(older);
        state.set_core_status(CoreStatus { running: true, ..Default::default() });
        let current = state.status().core_operation.unwrap();
        assert_eq!(current.kind, "downloading");
        assert_eq!(current.downloaded, 14);
        assert_eq!(current.total, Some(22));
        drop(download);
        assert!(state.status().core_operation.is_none());
        {
            let _failed = state.begin_operation("applying", "Config.yml");
        }
        assert!(state.status().core_operation.is_none());
    }
}
