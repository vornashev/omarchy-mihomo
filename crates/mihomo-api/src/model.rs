//! Typed views of mihomo controller responses.
//!
//! Only the fields the daemon uses are modelled; everything is
//! `#[serde(default)]` so newer cores with extra or missing keys still parse.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Version {
    pub meta: bool,
    pub version: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Rule,
    Global,
    Direct,
}

/// Subset of `GET /configs`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct BaseConfig {
    pub mixed_port: u16,
    pub port: u16,
    pub socks_port: u16,
    pub allow_lan: bool,
    pub mode: Mode,
    pub log_level: String,
    pub ipv6: bool,
    pub find_process_mode: String,
    pub tun: Tun,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Tun {
    pub enable: bool,
    pub device: String,
    pub stack: String,
    pub auto_route: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Proxies {
    pub proxies: BTreeMap<String, Proxy>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Proxy {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    /// Currently selected member, for groups.
    pub now: Option<String>,
    /// Members, for groups.
    pub all: Option<Vec<String>>,
    pub alive: Option<bool>,
    pub udp: bool,
    pub hidden: Option<bool>,
    pub history: Vec<DelayRecord>,
}

impl Proxy {
    /// Group types that can be used as a rule target.
    pub fn is_group(&self) -> bool {
        matches!(
            self.kind.as_str(),
            "Selector" | "URLTest" | "Fallback" | "LoadBalance" | "Relay"
        )
    }

    /// Last measured delay in ms; `0` from mihomo means failure.
    pub fn last_delay(&self) -> Option<u32> {
        self.history.last().map(|h| h.delay)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DelayRecord {
    pub time: String,
    pub delay: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Delay {
    pub delay: u32,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Traffic {
    pub up: u64,
    pub down: u64,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Memory {
    pub inuse: u64,
    pub oslimit: u64,
}

/// One line of the `/logs` stream.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LogLine {
    #[serde(rename = "type")]
    pub level: String,
    pub payload: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Connections {
    pub download_total: u64,
    pub upload_total: u64,
    pub memory: u64,
    /// `null` from the core when there are none.
    #[serde(deserialize_with = "null_as_default")]
    pub connections: Vec<Connection>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Connection {
    pub id: String,
    pub metadata: Metadata,
    pub upload: u64,
    pub download: u64,
    pub start: String,
    /// Leaf first: `chains[0]` is the real outbound, the last is the top group.
    pub chains: Vec<String>,
    pub rule: String,
    pub rule_payload: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Metadata {
    pub network: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "sourceIP")]
    pub source_ip: String,
    #[serde(rename = "destinationIP")]
    pub destination_ip: String,
    pub source_port: String,
    pub destination_port: String,
    pub host: String,
    pub sniff_host: String,
    pub process: String,
    pub process_path: String,
}

impl Metadata {
    /// Best name for the remote side: host, else sniffed host, else IP.
    pub fn target(&self) -> &str {
        [&self.host, &self.sniff_host, &self.destination_ip]
            .into_iter()
            .find(|s| !s.is_empty())
            .map(String::as_str)
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Rules {
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Rule {
    pub index: usize,
    #[serde(rename = "type")]
    pub kind: String,
    pub payload: String,
    pub proxy: String,
    pub extra: RuleExtra,
}

impl Rule {
    pub fn hits(&self) -> u64 {
        self.extra.hit_count
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RuleExtra {
    pub disabled: bool,
    pub hit_count: u64,
    pub miss_count: u64,
}

fn null_as_default<'de, D, T>(de: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(de)?.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_connection_snapshot() {
        let raw = r#"{"downloadTotal":10,"uploadTotal":5,"memory":0,"connections":[{"id":"5bfe","metadata":{"network":"tcp","type":"Tun","sourceIP":"198.18.0.1","destinationIP":"8.47.69.0","sourcePort":"47526","destinationPort":"443","host":"","sniffHost":"chatgpt.com","process":"","processPath":"","uid":0},"upload":2613,"download":5559,"start":"2026-10-14T17:12:59+03:00","chains":["FI","AI"],"providerChains":["",""],"rule":"DomainSuffix","rulePayload":"chatgpt.com"}]}"#;
        let c: Connections = serde_json::from_str(raw).unwrap();
        assert_eq!(c.connections.len(), 1);
        assert_eq!(c.connections[0].metadata.target(), "chatgpt.com");
        assert_eq!(c.connections[0].chains, ["FI", "AI"]);
    }

    #[test]
    fn null_connections_is_empty() {
        let c: Connections = serde_json::from_str(r#"{"connections":null}"#).unwrap();
        assert!(c.connections.is_empty());
    }

    #[test]
    fn group_detection() {
        let p: Proxy = serde_json::from_str(r#"{"name":"AI","type":"Selector","now":"FI","all":["FI","NL"]}"#).unwrap();
        assert!(p.is_group());
        let d: Proxy = serde_json::from_str(r#"{"name":"DIRECT","type":"Direct"}"#).unwrap();
        assert!(!d.is_group());
    }
}
