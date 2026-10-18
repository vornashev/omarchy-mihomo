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
