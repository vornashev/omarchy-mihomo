use std::collections::HashMap;
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
    pub core: mihomo_api::Client,
    pub config: crate::config::Manager,
    pub problems: crate::problems::Shared,
    status: watch::Sender<Status>,
    events: broadcast::Sender<Arc<Published>>,
    listeners: Mutex<HashMap<&'static str, usize>>,
    refresh: Notify,
    /// Last TUN failure the core logged; cleared once TUN is up.
    pub tun_error: Mutex<Option<String>>,
    pub core_install: Mutex<crate::health::Install>,
}

impl State {
    pub fn new(core: mihomo_api::Client, config: crate::config::Manager, problems: omm_problems::Collector) -> Self {
        let status = Status {
            daemon_version: env!("CARGO_PKG_VERSION").into(),
            protocol: PROTOCOL_VERSION,
            core: CoreStatus { socket: core.socket().display().to_string(), ..Default::default() },
            config: Default::default(),
            issues: Vec::new(),
        };
        Self {
            core,
            config,
            problems: std::sync::Mutex::new(problems),
            status: watch::channel(status).0,
            // Slow clients lag and skip events rather than stall the daemon.
            events: broadcast::channel(256).0,
            listeners: Mutex::default(),
            refresh: Notify::new(),
            tun_error: Mutex::default(),
            core_install: Mutex::default(),
        }
    }

    pub fn status(&self) -> Status {
        self.status.borrow().clone()
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
