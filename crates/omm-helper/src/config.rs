//! Root core configs never select executable files or paths outside its home.
use anyhow::{Context, Result, bail};
use serde_yaml_ng::Value;
use std::path::{Component, Path};

pub fn safe_relative(s: &str) -> bool {
    !s.is_empty()
        && Path::new(s)
            .components()
            .all(|p| matches!(p, Component::Normal(_) | Component::CurDir))
}
pub fn prepare(text: &str) -> Result<String> {
    let mut value: Value = serde_yaml_ng::from_str(text)?;
    let root = value.as_mapping_mut().context("config must be a mapping")?;
    // Controllers and downloaded UI archives must never escape the private runtime.
    for key in [
        "external-controller",
        "external-controller-tls",
        "external-controller-pipe",
        "external-ui",
        "external-ui-url",
        "external-ui-name",
        "secret",
        "interface-name",
        "routing-mark",
    ] {
        root.remove(key);
    }
    root.insert("external-controller-unix".into(), "controller.sock".into());
    // Restrict file references in all protocol types, including certificates.
    validate_files(&value, "")?;
    let root = value.as_mapping_mut().unwrap();
    if let Some(tun) = root.get_mut("tun").and_then(Value::as_mapping_mut) {
        let name = tun.get("device").and_then(Value::as_str).unwrap_or("utun");
        if !name.starts_with("utun") || !name[4..].bytes().all(|b| b.is_ascii_digit()) {
            bail!("macOS TUN device must be utun or utun followed by a number");
        }
        for key in [
            "auto-redirect",
            "include-uid",
            "exclude-uid",
            "include-uid-range",
            "exclude-uid-range",
            "include-android-user",
            "include-package",
            "exclude-package",
            "iproute2-table-index",
            "iproute2-rule-index",
            "gso",
        ] {
            if tun.contains_key(key) {
                bail!("TUN option {key} is not supported on macOS");
            }
        }
    }
    Ok(serde_yaml_ng::to_string(&value)?)
}
fn validate_files(value: &Value, parent: &str) -> Result<()> {
    match value {
        Value::Mapping(m) => {
            for (key, value) in m {
                let key = key.as_str().context("config keys must be strings")?;
                if matches!(
                    key,
                    "script" | "command" | "exec" | "external-ui" | "external-ui-url"
                ) {
                    bail!("{key} is not allowed in the privileged core");
                }
                if key == "plugin"
                    && !matches!(
                        value.as_str(),
                        Some("obfs" | "v2ray-plugin" | "shadow-tls" | "restls")
                    )
                {
                    bail!("external proxy plugins are not supported");
                }
                let file = matches!(
                    key,
                    "certificate"
                        | "private-key"
                        | "ca"
                        | "ca-file"
                        | "client-cert"
                        | "client-key"
                        | "certificate-path"
                        | "private-key-path"
                ) || (key == "path"
                    && !matches!(
                        parent,
                        "ws-opts" | "http-opts" | "h2-opts" | "grpc-opts" | "xhttp-opts"
                    ));
                if file
                    && let Some(s) = value.as_str()
                    && !s.contains("-----BEGIN ")
                    && !safe_relative(s)
                {
                    bail!(
                        "{key} must reference a file within the managed runtime (or contain a PEM value)"
                    );
                }
                validate_files(value, key)?;
            }
        }
        Value::Sequence(s) => {
            for v in s {
                validate_files(v, parent)?;
            }
        }
        _ => {}
    }
    Ok(())
}
pub fn allowed_patch(value: &serde_json::Value) -> bool {
    let Some(m) = value.as_object() else {
        return false;
    };
    !m.is_empty()
        && m.iter().all(|(key, v)| match key.as_str() {
            "mode" => matches!(v.as_str(), Some("rule" | "global" | "direct")),
            "log-level" => matches!(
                v.as_str(),
                Some("debug" | "info" | "warning" | "error" | "silent")
            ),
            "tun" => v.as_object().is_some_and(|m| {
                m.len() == 1 && m.get("enable").is_some_and(serde_json::Value::is_boolean)
            }),
            _ => false,
        })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn confines_files_but_allows_websocket_paths() {
        assert!(prepare("proxies: [{name: a, type: vless, ws-opts: {path: /socket}}]").is_ok());
        for path in ["/etc/passwd", "../escape", "a/../../escape"] {
            assert!(prepare(&format!("rule-providers: {{a: {{path: '{path}'}}}}")).is_err());
        }
        assert!(prepare("rule-providers: {a: {path: providers/a.yaml}}").is_ok());
        assert!(prepare("proxies: [{plugin: /bin/sh}]").is_err());
    }
    #[test]
    fn controller_and_tun_are_owned() {
        let text = prepare("external-controller: 0.0.0.0:9090\nexternal-controller-unix: /tmp/a\ntun: {device: utun12}").unwrap();
        assert!(!text.contains("9090") && text.contains("controller.sock"));
        assert!(prepare("tun: {device: Meta}").is_err());
        assert!(prepare("tun: {auto-redirect: true}").is_err());
    }
    #[test]
    fn runtime_patch_cannot_change_files_or_listeners() {
        assert!(allowed_patch(&serde_json::json!({"tun":{"enable":true}})));
        for v in [
            serde_json::json!({"external-controller":"0.0.0.0:9090"}),
            serde_json::json!({"tun":{"enable":true,"device":"other"}}),
            serde_json::json!({"mixed-port":1}),
        ] {
            assert!(!allowed_patch(&v));
        }
    }
}
