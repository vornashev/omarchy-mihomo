//! Managed settings: the part of the runtime config that belongs to
//! omarchy-mihomo rather than to a profile. They are applied after every
//! override, so a subscription or merge file cannot, say, open the controller
//! to the network or turn TUN off behind the user's back.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::merge::deep_merge;
use crate::yaml::{Mapping, Value, get_mut_mapping};

/// Controller socket name, relative to the core's home directory.
pub const CONTROLLER_SOCKET: &str = "mihomo.sock";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Settings {
    pub mixed_port: u16,
    pub allow_lan: bool,
    pub ipv6: bool,
    /// `rule`, `global` or `direct`; kept in sync with runtime mode changes.
    pub mode: String,
    pub log_level: String,
    /// `always` so every connection and error carries its process name.
    pub find_process_mode: String,
    pub unified_delay: bool,
    pub tcp_concurrent: bool,
    pub tun: Tun,
    /// Optional TCP controller for web dashboards. Off by default: the unix
    /// socket is enough for us and needs no secret.
    pub external_controller: Option<String>,
    pub secret: Option<String>,
    /// Free-form mihomo keys merged last (e.g. `dns`, `sniffer`, `geox-url`).
    pub extra: Mapping,
    /// Hours between subscription refreshes when a profile sets none.
    pub update_interval_hours: u32,
    /// The core refreshes GeoIP, GeoSite and the MMDBs on its own.
    pub geo_auto_update: bool,
    /// Hours between those refreshes.
    pub geo_update_interval: u32,
    pub dns: Dns,
}

/// DNS on top of the profile's, which is often tuned with care: each `Some`
/// replaces the profile's value, the lists extend it. Left empty, the
/// profile decides.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Dns {
    /// `fake-ip` or `redir-host`.
    pub enhanced_mode: Option<String>,
    pub nameserver: Option<Vec<String>>,
    pub fallback: Option<Vec<String>>,
    /// Plain-IP servers that resolve the DoH hosts above.
    pub default_nameserver: Option<Vec<String>>,
    /// Domains that get real addresses in fake-ip mode, after the profile's.
    pub fake_ip_filter: Vec<String>,
    /// `+.example.org` → server, ahead of the profile's policy.
    pub policy: Vec<Policy>,
    /// Read the host from TLS, HTTP and QUIC.
    pub sniffer: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Policy {
    pub domain: String,
    pub server: String,
}

impl Dns {
    fn touches_dns(&self) -> bool {
        self.enhanced_mode.is_some()
            || self.nameserver.is_some()
            || self.fallback.is_some()
            || self.default_nameserver.is_some()
            || !self.fake_ip_filter.is_empty()
            || !self.policy.is_empty()
    }

    fn enforce(&self, config: &mut Mapping) {
        if self.touches_dns() {
            let dns = get_mut_mapping(config, "dns");
            dns.insert("enable".into(), true.into());
            let list = |v: &[String]| Value::Sequence(v.iter().map(|s| Value::from(s.as_str())).collect());
            if let Some(mode) = &self.enhanced_mode {
                dns.insert("enhanced-mode".into(), mode.as_str().into());
            }
            for (key, value) in [("nameserver", &self.nameserver), ("fallback", &self.fallback), ("default-nameserver", &self.default_nameserver)] {
                if let Some(v) = value {
                    dns.insert(key.into(), list(v));
                }
            }
            if !self.fake_ip_filter.is_empty() {
                let slot = dns.entry("fake-ip-filter".into()).or_insert_with(|| Value::Sequence(Vec::new()));
                if !slot.is_sequence() {
                    *slot = Value::Sequence(Vec::new());
                }
                let filter = slot.as_sequence_mut().expect("just ensured a sequence");
                for d in &self.fake_ip_filter {
                    if !filter.iter().any(|v| v.as_str() == Some(d)) {
                        filter.push(d.as_str().into());
                    }
                }
            }
            if !self.policy.is_empty() {
                // Ours first: mihomo takes the first matching entry.
                let old = match dns.remove("nameserver-policy") {
                    Some(Value::Mapping(m)) => m,
                    _ => Mapping::new(),
                };
                let mut policy = Mapping::new();
                for p in &self.policy {
                    policy.insert(p.domain.as_str().into(), p.server.as_str().into());
                }
                for (k, v) in old {
                    policy.entry(k).or_insert(v);
                }
                dns.insert("nameserver-policy".into(), Value::Mapping(policy));
            }
        }
        if let Some(on) = self.sniffer {
            let sniffer = get_mut_mapping(config, "sniffer");
            sniffer.insert("enable".into(), on.into());
            if on && !sniffer.contains_key("sniff") {
                let ports = |p: &[&str]| Value::Mapping([(Value::from("ports"), Value::Sequence(p.iter().map(|s| Value::from(*s)).collect()))].into_iter().collect());
                let sniff: Mapping = [("HTTP", ports(&["80", "8080-8880"])), ("TLS", ports(&["443", "8443"])), ("QUIC", ports(&["443", "8443"]))]
                    .into_iter()
                    .map(|(k, v)| (Value::from(k), v))
                    .collect();
                sniffer.insert("sniff".into(), Value::Mapping(sniff));
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Tun {
    pub enable: bool,
    pub stack: String,
    pub device: String,
    pub auto_route: bool,
    pub auto_detect_interface: bool,
    pub strict_route: bool,
    pub dns_hijack: Vec<String>,
    pub mtu: Option<u32>,
    /// Any other tun key, merged as-is (`route-exclude-address`, ...).
    pub extra: Mapping,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mixed_port: 7897,
            allow_lan: false,
            ipv6: false,
            mode: "rule".into(),
            log_level: "info".into(),
            find_process_mode: "always".into(),
            unified_delay: true,
            tcp_concurrent: true,
            tun: Tun::default(),
            external_controller: None,
            secret: None,
            extra: Mapping::new(),
            update_interval_hours: 12,
            geo_auto_update: true,
            geo_update_interval: 24,
            dns: Dns::default(),
        }
    }
}

impl Default for Tun {
    fn default() -> Self {
        Self {
            enable: !omm_platform::IS_MACOS,
            stack: "gvisor".into(),
            device: if omm_platform::IS_MACOS { "utun" } else { "Meta" }.into(),
            auto_route: true,
            auto_detect_interface: true,
            strict_route: false,
            dns_hijack: vec!["any:53".into()],
            mtu: None,
            extra: Mapping::new(),
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_yaml_ng::from_str(&text).map_err(|e| Error::yaml(path.display().to_string(), e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(Error::io(path, e)),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let text = serde_yaml_ng::to_string(self).map_err(|e| Error::yaml("settings", e))?;
        crate::paths::write_atomic(path, format!("# omarchy-mihomo managed settings\n{text}").as_bytes())
    }

    /// Force the managed keys onto a generated config.
    pub fn enforce(&self, config: &mut Mapping) {
        // `extra` first: the typed keys below always win.
        crate::merge::apply(config, &self.extra);

        let set = |c: &mut Mapping, k: &str, v: Value| {
            c.insert(Value::from(k), v);
        };
        set(config, "mixed-port", self.mixed_port.into());
        set(config, "allow-lan", self.allow_lan.into());
        set(config, "ipv6", self.ipv6.into());
        set(config, "mode", self.mode.as_str().into());
        set(config, "log-level", self.log_level.as_str().into());
        set(config, "find-process-mode", self.find_process_mode.as_str().into());
        set(config, "unified-delay", self.unified_delay.into());
        set(config, "tcp-concurrent", self.tcp_concurrent.into());
        set(config, "geo-auto-update", self.geo_auto_update.into());
        self.dns.enforce(config);
        set(config, "geo-update-interval", self.geo_update_interval.clamp(1, 24 * 30).into());

        // Listeners other than mixed-port are profile noise; drop them.
        for key in ["port", "socks-port", "redir-port", "tproxy-port", "external-ui", "external-ui-url", "external-ui-name"] {
            config.remove(key);
        }
        set(config, "external-controller-unix", CONTROLLER_SOCKET.into());
        match &self.external_controller {
            Some(addr) => {
                set(config, "external-controller", addr.as_str().into());
                set(config, "secret", self.secret.clone().unwrap_or_default().into());
            }
            None => {
                config.remove("external-controller");
                config.remove("secret");
            }
        }

        // Keep group selections across reloads and restarts.
        get_mut_mapping(config, "profile").insert("store-selected".into(), true.into());

        let tun = get_mut_mapping(config, "tun");
        let mut t = Value::Mapping(std::mem::take(tun));
        deep_merge(&mut t, Value::Mapping(self.tun.extra.clone()));
        let Value::Mapping(mut t) = t else { unreachable!() };
        t.insert("enable".into(), self.tun.enable.into());
        t.insert("stack".into(), self.tun.stack.as_str().into());
        t.insert("device".into(), self.tun.device.as_str().into());
        t.insert("auto-route".into(), self.tun.auto_route.into());
        t.insert("auto-detect-interface".into(), self.tun.auto_detect_interface.into());
        t.insert("strict-route".into(), self.tun.strict_route.into());
        t.insert("dns-hijack".into(), self.tun.dns_hijack.iter().map(|s| Value::from(s.as_str())).collect::<Vec<_>>().into());
        if let Some(mtu) = self.tun.mtu {
            t.insert("mtu".into(), mtu.into());
        }
        *get_mut_mapping(config, "tun") = t;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::yaml::parse_mapping;

    #[test]
    fn managed_keys_win_over_profile() {
        let mut c = parse_mapping(
            "mixed-port: 7890\nport: 7891\nexternal-controller: 0.0.0.0:9090\nsecret: x\nmode: global\ntun: {enable: false, stack: system, route-exclude-address: [10.0.0.0/8]}\n",
            "p",
        )
        .unwrap();
        Settings::default().enforce(&mut c);
        assert_eq!(c["mixed-port"], Value::from(7897));
        assert!(c.get("port").is_none());
        assert!(c.get("external-controller").is_none() && c.get("secret").is_none());
        assert_eq!(c["external-controller-unix"], Value::from(CONTROLLER_SOCKET));
        assert_eq!(c["mode"], Value::from("rule"));
        assert_eq!(c["tun"]["enable"], Value::from(!omm_platform::IS_MACOS));
        assert_eq!(c["tun"]["stack"], Value::from("gvisor"));
        assert!(c["tun"].get("route-exclude-address").is_some(), "profile tun keys kept");
        assert_eq!(c["profile"]["store-selected"], Value::from(true));
        assert_eq!(c["find-process-mode"], Value::from("always"));
        assert_eq!((&c["geo-auto-update"], &c["geo-update-interval"]), (&Value::from(true), &Value::from(24)));
    }

    #[test]
    fn dns_extends_the_profile() {
        let profile = "dns: {enable: true, enhanced-mode: redir-host, nameserver: [https://a/dns-query], fake-ip-filter: [+.lan], nameserver-policy: {+.lan: system, +.x.org: 1.1.1.1}}\n";
        let mut c = parse_mapping(profile, "p").unwrap();
        Settings::default().enforce(&mut c);
        assert_eq!(c["dns"]["nameserver"][0], Value::from("https://a/dns-query"), "nothing set: the profile decides");
        assert!(c.get("sniffer").is_none());

        let mut c = parse_mapping(profile, "p").unwrap();
        let s = Settings {
            dns: Dns {
                enhanced_mode: Some("fake-ip".into()),
                nameserver: Some(vec!["https://b/dns-query".into()]),
                fake_ip_filter: vec!["+.lan".into(), "+.bank.ru".into()],
                policy: vec![Policy { domain: "+.x.org".into(), server: "https://b/dns-query#AI".into() }],
                sniffer: Some(true),
                ..Default::default()
            },
            ..Default::default()
        };
        s.enforce(&mut c);
        let dns = &c["dns"];
        assert_eq!(dns["enhanced-mode"], Value::from("fake-ip"));
        assert_eq!(dns["nameserver"].as_sequence().unwrap().len(), 1);
        assert_eq!(dns["fake-ip-filter"].as_sequence().unwrap().len(), 2, "added once");
        let policy = dns["nameserver-policy"].as_mapping().unwrap();
        assert_eq!(policy.keys().next().unwrap(), &Value::from("+.x.org"), "ours first");
        assert_eq!(policy["+.x.org"], Value::from("https://b/dns-query#AI"));
        assert_eq!(policy["+.lan"], Value::from("system"));
        assert_eq!(c["sniffer"]["enable"], Value::from(true));
        assert!(c["sniffer"]["sniff"]["TLS"].is_mapping());

        // A profile without DNS gets a working section.
        let mut c = parse_mapping("mode: rule\n", "p").unwrap();
        Settings { dns: Dns { nameserver: Some(vec!["1.1.1.1".into()]), ..Default::default() }, ..Default::default() }.enforce(&mut c);
        assert_eq!(c["dns"]["enable"], Value::from(true));
    }

    #[test]
    fn settings_roundtrip_with_defaults() {
        let s: Settings = serde_yaml_ng::from_str("mixed-port: 1080\ntun: {enable: false}\n").unwrap();
        assert_eq!(s.mixed_port, 1080);
        assert!(!s.tun.enable);
        assert_eq!(s.tun.stack, "gvisor", "missing keys use defaults");
        let back: Settings = serde_yaml_ng::from_str(&serde_yaml_ng::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }
}
