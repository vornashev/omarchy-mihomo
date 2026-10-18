//! Offline share-link / YAML / WireGuard import. Never executes config hooks.
use crate::{
    Error, Result,
    yaml::{Mapping, Value as Yaml},
};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use url::Url;

#[derive(Debug, Serialize)]
pub struct Item {
    pub index: usize,
    pub definition: Option<Mapping>,
    pub warnings: Vec<String>,
    pub error: Option<String>,
}
fn invalid(s: impl Into<String>) -> Error {
    Error::Invalid(s.into())
}
fn decode(s: &str) -> Result<String> {
    let s = s.trim();
    for engine in [&STANDARD, &STANDARD_NO_PAD, &URL_SAFE, &URL_SAFE_NO_PAD] {
        if let Ok(b) = engine.decode(s)
            && let Ok(s) = String::from_utf8(b)
        {
            return Ok(s);
        }
    }
    Err(invalid("Invalid base64 text"))
}
fn unescape(s: &str) -> String {
    percent_encoding::percent_decode_str(s)
        .decode_utf8_lossy()
        .into_owned()
}
fn mapping(v: Value) -> Result<Mapping> {
    serde_yaml_ng::to_value(v)
        .map_err(|e| Error::yaml("import", e))?
        .as_mapping()
        .cloned()
        .ok_or_else(|| invalid("Expected a node object"))
}
fn number(s: &str) -> Result<Value> {
    s.parse::<u16>()
        .ok()
        .filter(|p| *p > 0)
        .map(|p| json!(p))
        .ok_or_else(|| invalid("Invalid port"))
}
fn boolean(s: &str) -> Result<Value> {
    match s.to_ascii_lowercase().as_str() {
        "1" | "true" => Ok(json!(true)),
        "0" | "false" => Ok(json!(false)),
        _ => Err(invalid("Expected true/false or 1/0")),
    }
}

pub fn preview(text: &str) -> Result<Vec<Item>> {
    if text.len() > 1024 * 1024 {
        return Err(invalid("Import limit is 1 MiB"));
    }
    let text = text.trim().trim_start_matches('\u{feff}');
    if text.is_empty() {
        return Err(invalid("Paste links, YAML or a WireGuard configuration"));
    }
    let decoded;
    let text = if !text.contains("://") && !text.contains(':') && !text.contains("[Interface]") {
        decoded = decode(&text.split_whitespace().collect::<String>())?;
        decoded.as_str()
    } else {
        text
    };
    if text.contains("[Interface]") {
        let (def, warnings) = wireguard(text)?;
        return Ok(vec![Item {
            index: 1,
            definition: Some(def),
            warnings,
            error: None,
        }]);
    }
    if text
        .lines()
        .find(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
        .is_some_and(|l| l.contains("://") && !l.trim().starts_with('-') && !l.contains(": "))
    {
        let mut out = Vec::new();
        for (i, line) in text
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty() && !l.trim().starts_with('#'))
        {
            match link(line.trim()) {
                Ok(nodes) => {
                    for (definition, warnings) in nodes {
                        out.push(Item {
                            index: i + 1,
                            definition: Some(definition),
                            warnings,
                            error: None,
                        });
                    }
                }
                Err(e) => out.push(Item {
                    index: i + 1,
                    definition: None,
                    warnings: vec![],
                    error: Some(e.to_string()),
                }),
            }
        }
        if out.len() > 1000 {
            return Err(invalid("Import limit is 1000 nodes"));
        }
        return Ok(out);
    }
    let mut value: Yaml = serde_yaml_ng::from_str(text).map_err(|_| invalid("Invalid YAML"))?;
    value
        .apply_merge()
        .map_err(|_| invalid("Unresolved YAML anchor or merge"))?;
    let values = if let Some(v) = value.get("proxies") {
        v.as_sequence()
            .cloned()
            .ok_or_else(|| invalid("proxies must be a list"))?
    } else if let Some(a) = value.as_sequence() {
        a.clone()
    } else {
        vec![value]
    };
    if values.len() > 1000 {
        return Err(invalid("Import limit is 1000 nodes"));
    }
    Ok(values
        .into_iter()
        .enumerate()
        .map(|(i, v)| {
            let m = v.as_mapping().cloned();
            let valid = m.as_ref().is_some_and(|m| {
                m.get("type").and_then(Yaml::as_str).is_some()
                    && m.get("name").and_then(Yaml::as_str).is_some()
            });
            Item {
                index: i + 1,
                definition: if valid { m } else { None },
                warnings: vec![],
                error: if valid {
                    None
                } else {
                    Some("Node requires name and type".into())
                },
            }
        })
        .collect())
}

fn link(line: &str) -> Result<Vec<(Mapping, Vec<String>)>> {
    let (scheme, body) = line
        .split_once("://")
        .ok_or_else(|| invalid("Missing URI scheme"))?;
    if scheme == "vmess" && !body.contains('@') {
        let v: Value =
            serde_json::from_str(&decode(body)?).map_err(|_| invalid("Invalid VMess JSON"))?;
        let text = |key: &str| {
            v.get(key)
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| v.to_string())
                })
                .unwrap_or_default()
        };
        let server = text("add");
        if server.is_empty() {
            return Err(invalid("VMess server is missing"));
        }
        let mut m = json!({"name":if text("ps").is_empty(){server.clone()}else{text("ps")},"type":"vmess","server":server,"port":number(&text("port"))?,"uuid":text("id"),"alterId":text("aid").parse::<u32>().unwrap_or(0),"cipher":if text("scy").is_empty(){"auto".into()}else{text("scy")},"udp":true});
        if text("tls") == "tls" {
            m["tls"] = json!(true);
        }
        if !text("sni").is_empty() {
            m["servername"] = json!(text("sni"));
        }
        if !text("fp").is_empty() {
            m["client-fingerprint"] = json!(text("fp"));
        }
        if !text("alpn").is_empty() {
            m["alpn"] = json!(text("alpn").split(',').collect::<Vec<_>>());
        }
        let mut warnings = Vec::new();
        transport(
            &mut m,
            &text("net"),
            &text("path"),
            &text("host"),
            &text("path"),
            &mut warnings,
        );
        let known = [
            "v", "ps", "add", "port", "id", "aid", "scy", "net", "type", "host", "path", "tls",
            "sni", "fp", "alpn",
        ];
        if let Some(o) = v.as_object() {
            for k in o.keys().filter(|k| !known.contains(&k.as_str())) {
                warnings.push(format!("Unsupported VMess field: {k}"));
            }
        }
        if !text("type").is_empty() && text("type") != "none" {
            warnings.push("VMess header type needs review".into());
        }
        return Ok(vec![(mapping(m)?, warnings)]);
    }
    if scheme == "ssr" {
        let decoded = decode(body)?;
        let (base, query) = decoded.split_once("/?").unwrap_or((&decoded, ""));
        let fields: Vec<_> = base.rsplitn(6, ':').collect();
        if fields.len() != 6 {
            return Err(invalid("Invalid SSR link"));
        }
        let q: BTreeMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect();
        let mut m = json!({"name":q.get("remarks").map(|s|decode(s)).transpose()?.unwrap_or_else(||fields[5].into()),"type":"ssr","server":fields[5],"port":number(fields[4])?,"protocol":fields[3],"cipher":fields[2],"obfs":fields[1],"password":decode(fields[0])?,"udp":true});
        for (a, b) in [
            ("obfsparam", "obfs-param"),
            ("protoparam", "protocol-param"),
        ] {
            if let Some(v) = q.get(a) {
                m[b] = json!(decode(v)?);
            }
        }
        let warnings = q
            .keys()
            .filter(|k| !["remarks", "obfsparam", "protoparam", "group"].contains(&k.as_str()))
            .map(|k| format!("Unsupported SSR parameter: {k}"))
            .collect();
        return Ok(vec![(mapping(m)?, warnings)]);
    }
    let normalized;
    let line = if scheme == "ss" && !body.contains('@') {
        let (b, fragment) = body.split_once('#').unwrap_or((body, ""));
        normalized = format!("ss://{}#{}", decode(b)?, fragment);
        normalized.as_str()
    } else {
        line
    };
    let mut hopping_line = line.to_owned();
    let mut hopping_ports = None;
    if ["hy2", "hysteria2"].contains(&scheme) {
        let start = line.find("://").unwrap() + 3;
        let end = line[start..]
            .find(['/', '?', '#'])
            .map_or(line.len(), |i| start + i);
        if let Some(colon) = line[start..end].rfind(':').map(|i| start + i) {
            let ports = &line[colon + 1..end];
            if ports.contains([',', '-'])
                && ports
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == ',' || c == '-')
            {
                let first = ports.split([',', '-']).next().unwrap();
                number(first)?;
                hopping_ports = Some(ports.to_owned());
                hopping_line.replace_range(colon + 1..end, first);
            }
        }
    }
    let u = Url::parse(&hopping_line).map_err(|_| invalid("Invalid proxy URI or port"))?;
    let server = u
        .host_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid("Missing server"))?
        .trim_start_matches('[')
        .trim_end_matches(']');
    let kind = match scheme {
        "vless" | "vmess" | "trojan" | "hysteria" | "tuic" | "anytls" | "ss" => scheme,
        "hy2" | "hysteria2" | "hy2+realm" | "hysteria2+realm" => "hysteria2",
        "socks" | "socks5" | "socks5h" => "socks5",
        "http" | "https" => "http",
        "mierus" => "mieru",
        _ => return Err(invalid(format!("Unsupported share-link scheme: {scheme}"))),
    };
    let mut q: BTreeMap<String, String> = BTreeMap::new();
    let mut repeated: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (k, v) in u.query_pairs() {
        q.insert(k.to_string(), v.to_string());
        repeated
            .entry(k.to_string())
            .or_default()
            .push(v.to_string());
    }
    let mut used = BTreeSet::new();
    let mut m = json!({"name":u.fragment().map(unescape).filter(|s|!s.is_empty()).unwrap_or_else(||format!("{server} · {kind}")),"type":kind,"server":server,"port":u.port().or_else(||u.port_or_known_default()).unwrap_or(443)});
    if let Some(ports) = hopping_ports {
        m["ports"] = json!(ports);
    }
    let username = unescape(u.username());
    let password = u.password().map(unescape);
    match kind {
        "vless" => {
            m["uuid"] = json!(username);
            m["udp"] = json!(true);
        }
        "vmess" => {
            let auth = if let Some(p) = password {
                format!("{username}:{p}")
            } else {
                decode(&username).unwrap_or(username)
            };
            let (cipher, uuid) = auth.split_once(':').unwrap_or(("auto", &auth));
            m["uuid"] = json!(uuid);
            m["udp"] = json!(true);
            m["cipher"] = json!(cipher);
            m["alterId"] = json!(0);
        }
        "trojan" | "anytls" => {
            m["password"] = json!(username);
            m["udp"] = json!(true);
        }
        "hysteria2" => {
            m["password"] = json!(
                password
                    .as_ref()
                    .map(|p| format!("{username}:{p}"))
                    .unwrap_or(username.clone())
            );
        }
        "tuic" => {
            m["uuid"] = json!(username);
            m["password"] = json!(password.unwrap_or_default());
        }
        "ss" => {
            let auth = if let Some(p) = password {
                format!("{username}:{p}")
            } else {
                decode(&username)?
            };
            let (cipher, secret) = auth
                .split_once(':')
                .ok_or_else(|| invalid("SS link needs cipher and password"))?;
            m["cipher"] = json!(cipher);
            m["password"] = json!(secret);
            m["udp"] = json!(true);
        }
        "socks5" | "http" => {
            if let Some(p) = password {
                m["username"] = json!(username);
                m["password"] = json!(p);
            } else if !username.is_empty() {
                if let Ok(decoded) = decode(&username) {
                    let (user, pass) = decoded.split_once(':').unwrap_or((&decoded, ""));
                    m["username"] = json!(user);
                    m["password"] = json!(pass);
                } else {
                    m["username"] = json!(username);
                }
            }
            if scheme == "https" {
                m["tls"] = json!(true);
            }
        }
        "mieru" => {
            m["username"] = json!(username);
            m["password"] = json!(password.unwrap_or_default());
            m["udp"] = json!(true);
        }
        _ => (),
    }
    let mut warnings = Vec::new();
    if let Some(a) = q.get("alterId").or_else(|| q.get("alterid"))
        && kind == "vmess"
    {
        m["alterId"] = json!(
            a.parse::<u32>()
                .map_err(|_| invalid("Invalid VMess alterId"))?
        );
        used.insert("alterId".into());
        used.insert("alterid".into());
    }
    for (key, value) in &q {
        let dest = match key.as_str() {
            "sni" | "peer" => {
                if ["vless", "vmess"].contains(&kind) {
                    "servername"
                } else {
                    "sni"
                }
            }
            "fp" => "client-fingerprint",
            "pbk" => "reality-opts.public-key",
            "sid" => "reality-opts.short-id",
            "allowInsecure" | "insecure" | "skip-cert-verify" => "skip-cert-verify",
            "pinSHA256" => "fingerprint",
            "congestion_control" => "congestion-controller",
            "udp_relay_mode" => "udp-relay-mode",
            "auth" => {
                if kind == "hysteria" {
                    "auth-str"
                } else {
                    "password"
                }
            }
            "upmbps" => "up",
            "downmbps" => "down",
            "mport" => "ports",
            "flow" | "encryption" | "obfs" | "obfs-password" | "up" | "down" | "alpn" | "ports"
            | "hop-interval" | "multiplexing" | "handshake-mode" | "traffic-pattern"
            | "disable-sni" | "reduce-rtt" | "token" => key.as_str(),
            _ => continue,
        };
        used.insert(key.clone());
        let v = if ["skip-cert-verify", "disable-sni", "reduce-rtt"].contains(&dest) {
            boolean(value)?
        } else if dest == "alpn" {
            json!(value.split(',').collect::<Vec<_>>())
        } else {
            json!(value)
        };
        if let Some((a, b)) = dest.split_once('.') {
            if m.get(a).is_none() {
                m[a] = json!({});
            }
            m[a][b] = v;
        } else {
            m[dest] = v;
        }
    }
    if let Some(security) = q.get("security") {
        used.insert("security".into());
        match security.as_str() {
            "tls" | "reality" => m["tls"] = json!(true),
            "none" | "" => m["tls"] = json!(false),
            _ => warnings.push("Unsupported security mode".into()),
        }
    }
    if ["vless", "vmess", "trojan"].contains(&kind) {
        for k in ["type", "network", "path", "host", "serviceName"] {
            used.insert(k.into());
        }
        transport(
            &mut m,
            q.get("type")
                .or_else(|| q.get("network"))
                .map(String::as_str)
                .unwrap_or("tcp"),
            q.get("path").map(String::as_str).unwrap_or(""),
            q.get("host").map(String::as_str).unwrap_or(""),
            q.get("serviceName").map(String::as_str).unwrap_or(""),
            &mut warnings,
        );
    }
    if kind == "ss"
        && let Some(plugin) = q.get("plugin")
    {
        used.insert("plugin".into());
        let mut pieces = plugin.split(';');
        let p = pieces.next().unwrap_or("");
        m["plugin"] = json!(if p == "obfs-local" { "obfs" } else { p });
        m["plugin-opts"] = json!({});
        for part in pieces {
            let (k, v) = part.split_once('=').unwrap_or((part, "true"));
            let k = k.strip_prefix("obfs-").unwrap_or(k);
            m["plugin-opts"][k] = if v == "true" { json!(true) } else { json!(v) };
        }
    }
    if scheme.ends_with("+realm") {
        used.insert("stun".into());
        m["realm-opts"] = json!({"enable":true,"server-url":format!("https://{}",&u[url::Position::BeforeHost..url::Position::AfterPort]),"token":m["password"],"realm-id":unescape(u.path().trim_start_matches('/')),"stun-servers":repeated.get("stun").cloned().unwrap_or_default()});
        m["password"] = json!(q.get("auth").cloned().unwrap_or_default());
    }
    if kind == "mieru" {
        for k in ["port", "protocol", "profile"] {
            used.insert(k.into());
        }
        let ports = repeated
            .get("port")
            .ok_or_else(|| invalid("Mieru link needs port query parameters"))?;
        let protocols = repeated
            .get("protocol")
            .ok_or_else(|| invalid("Mieru link needs protocol query parameters"))?;
        if ports.len() != protocols.len() {
            return Err(invalid("Mieru ports/protocols have different lengths"));
        }
        warnings.extend(
            q.keys()
                .filter(|k| !used.contains(*k))
                .map(|k| format!("Unsupported parameter: {k}")),
        );
        let mut out = Vec::new();
        for (port, protocol) in ports.iter().zip(protocols) {
            let mut n = m.clone();
            n["name"] = json!(format!(
                "{}:{port}/{protocol}",
                q.get("profile")
                    .map(String::as_str)
                    .unwrap_or(m["name"].as_str().unwrap_or(server))
            ));
            n["transport"] = json!(protocol);
            if port.contains('-') {
                n.as_object_mut().unwrap().remove("port");
                n["port-range"] = json!(port);
            } else {
                n["port"] = number(port)?;
            }
            out.push((mapping(n)?, warnings.clone()));
        }
        return Ok(out);
    }
    warnings.extend(
        q.keys()
            .filter(|k| !used.contains(*k))
            .map(|k| format!("Unsupported parameter: {k}")),
    );
    // Fields that don't exist for this protocol are explicit warnings, never silent drops.
    let catalog = crate::nodes::catalog();
    let schema = catalog["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["type"] == kind)
        .unwrap();
    let allowed: BTreeSet<_> = schema["fields"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|f| f["key"].as_str())
        .chain(["type"])
        .collect();
    for key in m.as_object().unwrap().keys() {
        if !allowed.contains(key.as_str()) {
            warnings.push(format!("Field not supported by {kind}: {key}"));
        }
    }
    Ok(vec![(mapping(m)?, warnings)])
}

fn transport(
    m: &mut Value,
    network: &str,
    path: &str,
    host: &str,
    service: &str,
    warnings: &mut Vec<String>,
) {
    let network = if network.is_empty() { "tcp" } else { network };
    m["network"] = json!(if network == "httpupgrade" {
        "ws"
    } else {
        network
    });
    match network {
        "tcp" => (),
        "ws" | "httpupgrade" => {
            m["ws-opts"] = json!({"path":path});
            if !host.is_empty() {
                m["ws-opts"]["headers"] = json!({"Host":host});
            }
            if network == "httpupgrade" {
                m["ws-opts"]["v2ray-http-upgrade"] = json!(true);
            }
        }
        "grpc" => m["grpc-opts"] = json!({"grpc-service-name":service}),
        "h2" => {
            m["h2-opts"] = json!({"host":host.split(',').filter(|s|!s.is_empty()).collect::<Vec<_>>(),"path":path})
        }
        "http" => {
            m["http-opts"] =
                json!({"path":[path],"headers":{"Host":host.split(',').collect::<Vec<_>>()}})
        }
        "xhttp" => m["xhttp-opts"] = json!({"path":path,"host":host}),
        _ => warnings.push(format!("Transport requires review: {network}")),
    }
}

fn wireguard(text: &str) -> Result<(Mapping, Vec<String>)> {
    let mut m = json!({"name":"WireGuard","type":"wireguard","udp":true,"peers":[]});
    let mut section = "";
    let mut peer = None;
    let mut warnings = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') {
            section = line;
            if section == "[Peer]" {
                m["peers"].as_array_mut().unwrap().push(json!({}));
                peer = Some(m["peers"].as_array().unwrap().len() - 1);
            }
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            return Err(invalid(format!("Invalid WireGuard line {}", i + 1)));
        };
        let k = k.trim();
        let v = v.trim();
        if section == "[Interface]" {
            match k {
                "PrivateKey" => m["private-key"] = json!(v),
                "Address" => {
                    for addr in v.split(',').map(str::trim) {
                        let address = addr.split('/').next().unwrap_or(addr);
                        m[if address.contains(':') { "ipv6" } else { "ip" }] = json!(address);
                    }
                }
                "DNS" => {
                    m["dns"] = json!(v.split(',').map(str::trim).collect::<Vec<_>>());
                    m["remote-dns-resolve"] = json!(true);
                }
                "MTU" => m["mtu"] = json!(v.parse::<u32>().map_err(|_| invalid("Invalid MTU"))?),
                "Jc" | "Jmin" | "Jmax" | "S1" | "S2" | "S3" | "S4" | "H1" | "H2" | "H3" | "H4"
                | "I1" | "I2" | "I3" | "I4" | "I5" | "J1" | "J2" | "J3" | "Itime" | "Version" => {
                    if m.get("amnezia-wg-option").is_none() {
                        m["amnezia-wg-option"] = json!({});
                    }
                    let key = k.to_ascii_lowercase();
                    m["amnezia-wg-option"][&key] = if matches!(
                        k,
                        "Jc" | "Jmin" | "Jmax" | "S1" | "S2" | "S3" | "S4" | "Itime" | "Version"
                    ) {
                        json!(
                            v.parse::<u32>()
                                .map_err(|_| invalid(format!("Invalid {k}")))?
                        )
                    } else {
                        json!(v)
                    };
                }
                _ => warnings.push(format!("Interface field not transferred: {k}")),
            }
        } else if section == "[Peer]" {
            let p = &mut m["peers"][peer.unwrap()];
            match k {
                "PublicKey" => p["public-key"] = json!(v),
                "PresharedKey" => p["pre-shared-key"] = json!(v),
                "AllowedIPs" => {
                    p["allowed-ips"] = json!(v.split(',').map(str::trim).collect::<Vec<_>>())
                }
                "Endpoint" => {
                    let u = Url::parse(&format!("wg://{v}"))
                        .map_err(|_| invalid("Invalid WireGuard endpoint"))?;
                    p["server"] = json!(
                        u.host_str()
                            .ok_or_else(|| invalid("Missing endpoint host"))?
                            .trim_matches(['[', ']'])
                    );
                    p["port"] = json!(u.port().ok_or_else(|| invalid("Missing endpoint port"))?);
                }
                "PersistentKeepalive" => {
                    let n = v.parse::<u32>().map_err(|_| invalid("Invalid keepalive"))?;
                    if m.get("persistent-keepalive")
                        .is_some_and(|old| old != &json!(n))
                    {
                        warnings.push(
                            "Different peer keepalive values: Mihomo has one shared keepalive"
                                .into(),
                        );
                    }
                    m["persistent-keepalive"] = json!(n);
                }
                _ => warnings.push(format!("Peer field not transferred: {k}")),
            }
        } else {
            warnings.push(format!("Unknown section at line {}", i + 1));
        }
    }
    if m.get("private-key").is_none() || m["peers"].as_array().unwrap().is_empty() {
        return Err(invalid("WireGuard needs PrivateKey and at least one Peer"));
    }
    Ok((mapping(m)?, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vless_reality_unicode_ipv6() {
        let r=preview("vless://00000000-0000-4000-8000-000000000001@[2001:db8::1]:443?security=reality&sni=example.net&pbk=abc&sid=1234&type=grpc&serviceName=tunnel#%D0%A2%D0%B5%D1%81%D1%82").unwrap();
        let m = r[0].definition.as_ref().unwrap();
        assert_eq!(m["name"].as_str(), Some("Тест"));
        assert_eq!(m["server"].as_str(), Some("2001:db8::1"));
        assert_eq!(m["reality-opts"]["public-key"].as_str(), Some("abc"));
        assert!(r[0].warnings.is_empty());
    }
    #[test]
    fn batch_reports_errors_and_unknown_parameters() {
        let r = preview("vless://id@host:443?unknown=value\nnot-a-link\nhy2://p%40ss@host:443#H")
            .unwrap();
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].warnings.len(), 1);
        assert!(r[1].error.is_some());
        assert_eq!(
            r[2].definition.as_ref().unwrap()["password"].as_str(),
            Some("p@ss")
        );
    }
    #[test]
    fn yaml_anchors() {
        let r = preview(
            "base: &w {type: wireguard, private-key: fake}\nproxies: [{name: WARP, <<: *w}]",
        )
        .unwrap();
        assert_eq!(
            r[0].definition.as_ref().unwrap()["type"].as_str(),
            Some("wireguard")
        );
    }
    #[test]
    fn wg_import_never_executes_hooks() {
        let r=preview("[Interface]\nAddress = 172.16.0.2/32, fd00::2/128\nPrivateKey = fake\nPostUp = touch /tmp/never\nI1 = <b 0x0000><r 10>\n[Peer]\nPublicKey = public\nEndpoint = [2001:db8::1]:4500\nAllowedIPs = 0.0.0.0/0").unwrap();
        assert_eq!(r[0].warnings.len(), 1);
        let m = r[0].definition.as_ref().unwrap();
        assert_eq!(m["ipv6"].as_str(), Some("fd00::2"));
        assert_eq!(
            m["amnezia-wg-option"]["i1"].as_str(),
            Some("<b 0x0000><r 10>")
        );
    }
    #[test]
    fn ss_and_encoded_subscription() {
        let auth = STANDARD_NO_PAD.encode("aes-128-gcm:p@ss:word");
        let link = format!("ss://{auth}@example.net:443#EU");
        let r = preview(&STANDARD.encode(link)).unwrap();
        assert_eq!(
            r[0].definition.as_ref().unwrap()["password"].as_str(),
            Some("p@ss:word")
        );
    }
    #[test]
    fn vmess_uri_and_hysteria_port_hopping() {
        let r=preview("vmess://auto:00000000-0000-4000-8000-000000000001@example.net:443?alterId=0\nhy2://p@example.net:443,500-600?insecure=0").unwrap();
        assert_eq!(
            r[0].definition.as_ref().unwrap()["uuid"].as_str(),
            Some("00000000-0000-4000-8000-000000000001")
        );
        assert!(r[0].warnings.is_empty());
        assert_eq!(
            r[1].definition.as_ref().unwrap()["ports"].as_str(),
            Some("443,500-600")
        );
        assert_eq!(
            r[1].definition.as_ref().unwrap()["skip-cert-verify"].as_bool(),
            Some(false)
        );
    }
}
