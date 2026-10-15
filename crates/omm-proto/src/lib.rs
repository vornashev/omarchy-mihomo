//! Wire protocol of `omarchy-mihomod`.
//!
//! Newline-delimited JSON over a unix socket. Clients send [`Request`]s and
//! receive [`Response`]s with the same `id`; after `events.subscribe` the
//! daemon also pushes [`Event`]s on the same connection. Every frame is one
//! line, which keeps the QML side (`Quickshell.Io.Socket` + `SplitParser`)
//! trivial.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Bumped on incompatible changes; reported by `hello`.
pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub method: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    pub fn ok(id: u64, result: Value) -> Self {
        Self { id, result: Some(result), error: None }
    }

    pub fn err(id: u64, error: RpcError) -> Self {
        Self { id, result: None, error: Some(error) }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: ErrorCode,
    pub message: String,
}

impl RpcError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for RpcError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Malformed frame or params.
    BadRequest,
    UnknownMethod,
    /// The mihomo core is not running or not reachable.
    CoreUnavailable,
    /// The core answered with an error.
    Core,
    /// Requested object does not exist.
    NotFound,
    /// A profile or override produced a config mihomo rejects.
    Config,
    /// A subscription could not be downloaded.
    Fetch,
    Internal,
}

/// Unsolicited message pushed to subscribed clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub event: String,
    pub data: Value,
}

/// Any frame the daemon may send.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ServerFrame {
    Response(Response),
    Event(Event),
}

/// Event topics.
pub mod topic {
    /// [`crate::Status`], on every change of core state.
    pub const STATUS: &str = "status";
    /// `{up, down}` bytes per second, once a second.
    pub const TRAFFIC: &str = "traffic";
    /// `{counts: {new, visible, hidden}, recorded}` whenever failures were
    /// recorded or counts changed; drives the bar badge.
    pub const PROBLEMS: &str = "problems";
    /// `{type, payload}` core log lines (level info and above).
    pub const LOGS: &str = "logs";
    /// `{id, name, draft, error}` after a local profile's file was saved
    /// and checked.
    pub const PROFILES: &str = "profiles";

    pub const ALL: &[&str] = &[STATUS, TRAFFIC, PROBLEMS, LOGS, PROFILES];

    /// The canonical `'static` name of a known topic.
    pub fn resolve(name: &str) -> Option<&'static str> {
        ALL.iter().copied().find(|t| *t == name)
    }
}

// ---------------------------------------------------------------- payloads

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Status {
    pub daemon_version: String,
    pub protocol: u32,
    pub core: CoreStatus,
    #[serde(default)]
    pub config: ConfigStatus,
    /// What stands in the way, each with its fix when there is one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<Issue>,
}

/// A problem with the setup. Clients word it from `id` (and `detail`);
/// `omm fix` lists them in English.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Issue {
    /// `core_missing`, `core_installing`, `core_install_failed`,
    /// `other_core`, `core_no_caps`, `core_stopped`, `tun_failed`,
    /// `no_profile`, `autotune`.
    pub id: String,
    pub level: IssueLevel,
    /// The variable part of the message: an app name, an error text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<Fix>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueLevel {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fix {
    /// Passed to `health.fix`, or to `omm fix` when privileged.
    pub id: String,
    /// Needs sudo: run `omm fix <id>` in a terminal instead of `health.fix`.
    pub privileged: bool,
    /// Changes other programs or the system: ask before running.
    pub confirm: bool,
}

/// State of the generated runtime config.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConfigStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<ProfileRef>,
    /// Unix seconds of the last successful apply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_at: Option<u64>,
    /// User rules skipped because their target group is gone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub orphans: Vec<i64>,
    /// Last failed apply or profile update.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CoreStatus {
    pub running: bool,
    pub socket: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    pub tun: bool,
    pub mixed_port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Set while the settings ask for TUN but the core runs without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tun_issue: Option<TunIssue>,
}

/// Why TUN is off although the settings turn it on.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TunIssue {
    /// The core binary lacks the capabilities TUN needs (`omm core setcap`).
    pub no_capabilities: bool,
    /// Another mihomo core that may hold the TUN device and routes
    /// (Clash Verge's `verge-mihomo`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other_core: Option<String>,
    /// The core's own complaint, from its log.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyGroup {
    pub name: String,
    pub kind: String,
    pub now: Option<String>,
    pub members: Vec<ProxyMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyMember {
    pub name: String,
    pub kind: String,
    /// Last delay in ms; `None` when never tested, `Some(0)` when it failed.
    pub delay: Option<u32>,
    pub alive: Option<bool>,
}

// ------------------------------------------------------------------ paths

/// `$XDG_RUNTIME_DIR/omarchy-mihomo/daemon.sock`.
pub fn default_socket_path() -> PathBuf {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    runtime.join("omarchy-mihomo").join("daemon.sock")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn frames_are_distinguishable() {
        let r: ServerFrame = serde_json::from_value(json!({"id": 3, "result": {"ok": true}})).unwrap();
        assert!(matches!(r, ServerFrame::Response(Response { id: 3, .. })));
        let e: ServerFrame = serde_json::from_value(json!({"event": "traffic", "data": {"up": 1}})).unwrap();
        assert!(matches!(e, ServerFrame::Event(Event { ref event, .. }) if event == "traffic"));
    }

    #[test]
    fn request_params_optional() {
        let r: Request = serde_json::from_str(r#"{"id":1,"method":"status"}"#).unwrap();
        assert!(r.params.is_null());
        assert_eq!(serde_json::to_string(&r).unwrap(), r#"{"id":1,"method":"status"}"#);
    }

    #[test]
    fn error_codes_are_snake_case() {
        let e = Response::err(7, RpcError::new(ErrorCode::CoreUnavailable, "down"));
        assert_eq!(
            serde_json::to_value(e).unwrap(),
            json!({"id": 7, "error": {"code": "core_unavailable", "message": "down"}})
        );
    }
}
