//! Profile-local, named edits. Legacy sequence overrides remain untouched.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    Error, Result,
    paths::{Paths, read_optional, write_atomic},
    yaml::{self, Mapping, Value},
};

pub const BUILTINS: &[&str] = &[
    "DIRECT",
    "REJECT",
    "REJECT-DROP",
    "PASS",
    "COMPATIBLE",
    "GLOBAL",
];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Managed {
    pub nodes: BTreeMap<String, Option<Mapping>>,
    pub groups: BTreeMap<String, Option<Mapping>>,
    pub providers: BTreeMap<String, Mapping>,
    /// Scoped rewrites, including the targets of global user rules.
    pub redirects: BTreeMap<String, String>,
    /// User-chosen order of proxy groups; unlisted groups keep their place after these.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub group_order: Vec<String>,
}

impl Managed {
    pub fn load(paths: &Paths, id: &str) -> Result<Self> {
        let text = read_optional(&paths.overrides(id).join("nodes.yaml"))?;
        match text {
            Some(t) => serde_yaml_ng::from_str(&t).map_err(|e| Error::yaml("nodes.yaml", e)),
            None => Ok(Self::default()),
        }
    }
    pub fn save(&self, paths: &Paths, id: &str) -> Result<()> {
        let text = serde_yaml_ng::to_string(self).map_err(|e| Error::yaml("nodes.yaml", e))?;
        write_atomic(&paths.overrides(id).join("nodes.yaml"), text.as_bytes())
    }
    pub fn entries(&self, group: bool) -> &BTreeMap<String, Option<Mapping>> {
        if group { &self.groups } else { &self.nodes }
    }
    pub fn entries_mut(&mut self, group: bool) -> &mut BTreeMap<String, Option<Mapping>> {
        if group {
            &mut self.groups
        } else {
            &mut self.nodes
        }
    }
    pub fn original(&self, group: bool, name: &str) -> String {
        self.entries(group)
            .iter()
            .find(|(_, v)| {
                v.as_ref()
                    .and_then(|v| v.get("name"))
                    .and_then(Value::as_str)
                    == Some(name)
            })
            .map(|(k, _)| k.clone())
            .unwrap_or_else(|| name.to_owned())
    }
    pub fn target(&self, name: &str) -> String {
        let mut name = name.to_owned();
        let mut seen = BTreeSet::new();
        while seen.insert(name.clone()) {
            match self.redirects.get(&name) {
                Some(n) => name = n.clone(),
                None => break,
            }
        }
        name
    }
    pub fn apply(&self, config: &mut Mapping) {
        for (key, edits) in [("proxies", &self.nodes), ("proxy-groups", &self.groups)] {
            let mut remaining = edits.clone();
            let mut items = Vec::new();
            if let Some(Value::Sequence(base)) = config.remove(key) {
                for item in base {
                    match yaml::entry_name(&item).and_then(|n| remaining.remove(n)) {
                        Some(Some(m)) => items.push(Value::Mapping(m)),
                        Some(None) => (),
                        None => items.push(item),
                    }
                }
            }
            items.extend(remaining.into_values().flatten().map(Value::Mapping));
            if key == "proxy-groups" && !self.group_order.is_empty() {
                let rank = |v: &Value| {
                    yaml::entry_name(v)
                        .and_then(|n| self.group_order.iter().position(|o| o == n))
                        .unwrap_or(usize::MAX)
                };
                items.sort_by_key(rank);
            }
            config.insert(key.into(), Value::Sequence(items));
        }
        if !self.providers.is_empty() {
            let providers = config.entry("proxy-providers".into()).or_insert_with(|| Value::Mapping(Mapping::new()));
            if let Some(providers) = providers.as_mapping_mut() {
                for (name, definition) in &self.providers {
                    providers.insert(name.clone().into(), Value::Mapping(definition.clone()));
                }
            }
        }
        rewrite(config, &|n| self.target(n));
    }
}

/// Only rewrite rule targets, never domains, provider names or arbitrary strings.
pub fn rule_target(line: &str) -> Option<(usize, usize)> {
    let mut depth = 0;
    let mut parts = Vec::new();
    let mut start = 0;
    for (i, c) in line.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push((start, i));
                start = i + 1;
            }
            _ => (),
        }
    }
    parts.push((start, line.len()));
    if parts.len() < 2 {
        return None;
    }
    let mut i = parts.len() - 1;
    while i > 0 && ["no-resolve", "src"].contains(&line[parts[i].0..parts[i].1].trim()) {
        i -= 1;
    }
    Some(parts[i])
}

pub fn rewrite(config: &mut Mapping, f: &impl Fn(&str) -> String) {
    fn visit(value: &mut Value, key: &str, f: &impl Fn(&str) -> String) {
        match value {
            Value::Mapping(m) => {
                for (k, v) in m {
                    visit(v, k.as_str().unwrap_or(""), f);
                }
            }
            Value::Sequence(s) => {
                for v in s {
                    visit(v, key, f);
                }
            }
            Value::String(s)
                if [
                    "proxies",
                    "dialer-proxy",
                    "default-selected",
                    "empty-fallback",
                    "proxy",
                ]
                .contains(&key) =>
            {
                *s = f(s)
            }
            Value::String(s) if key == "rules" || key == "sub-rules" => {
                if let Some((a, b)) = rule_target(s) {
                    let replacement = f(s[a..b].trim());
                    s.replace_range(a..b, &replacement);
                }
            }
            _ => (),
        }
    }
    // sub-rules has named lists; handle their rule strings explicitly.
    if let Some(Value::Mapping(sub)) = config.get_mut("sub-rules") {
        for value in sub.values_mut() {
            visit(value, "rules", f);
        }
    }
    for (k, v) in config {
        if k.as_str() != Some("sub-rules") {
            visit(v, k.as_str().unwrap_or(""), f);
        }
    }
}

pub fn definitions(config: &Mapping, group: bool) -> Vec<Mapping> {
    config
        .get(if group { "proxy-groups" } else { "proxies" })
        .and_then(Value::as_sequence)
        .into_iter()
        .flatten()
        .filter_map(Value::as_mapping)
        .cloned()
        .collect()
}

pub fn name(m: &Mapping) -> &str {
    m.get("name").and_then(Value::as_str).unwrap_or("")
}

#[derive(Debug, Serialize)]
pub struct Dependency {
    pub owner: String,
    pub field: String,
}

pub fn dependencies(config: &Mapping, target: &str) -> Vec<Dependency> {
    let mut out = Vec::new();
    fn visit(v: &Value, path: &str, key: &str, target: &str, out: &mut Vec<Dependency>) {
        match v {
            Value::Mapping(m) => {
                for (k, v) in m {
                    let k = k.as_str().unwrap_or("");
                    visit(v, &format!("{path}.{k}"), k, target, out);
                }
            }
            Value::Sequence(s) => {
                for (i, v) in s.iter().enumerate() {
                    visit(v, &format!("{path}[{i}]"), key, target, out);
                }
            }
            Value::String(s) => {
                let is_ref = [
                    "proxies",
                    "dialer-proxy",
                    "empty-fallback",
                    "default-selected",
                    "proxy",
                ]
                .contains(&key)
                    && s == target;
                let is_rule = (key == "rules" || path.starts_with("sub-rules."))
                    && rule_target(s).is_some_and(|(a, b)| s[a..b].trim() == target);
                if is_ref || is_rule {
                    out.push(Dependency {
                        owner: path.to_owned(),
                        field: key.to_owned(),
                    });
                }
            }
            _ => (),
        }
    }
    for (k, v) in config {
        let k = k.as_str().unwrap_or("");
        visit(v, k, k, target, &mut out);
    }
    out
}

pub fn validate(config: &Mapping) -> Result<()> {
    let mut known: BTreeSet<String> = BUILTINS.iter().map(|s| s.to_string()).collect();
    let groups = definitions(config, true);
    let nodes = definitions(config, false);
    for m in nodes.iter().chain(&groups) {
        let n = name(m);
        if n.trim().is_empty()
            || n != n.trim()
            || n.contains([',', '\n', '\r'])
            || !known.insert(n.into())
        {
            return Err(Error::Invalid(format!(
                "invalid or duplicate proxy/group name: {n}"
            )));
        }
        if m.get("type").and_then(Value::as_str).is_none() {
            return Err(Error::Invalid(format!("{n}: missing type")));
        }
    }
    let providers: BTreeSet<&str> = config
        .get("proxy-providers")
        .and_then(Value::as_mapping)
        .into_iter()
        .flat_map(|m| m.keys())
        .filter_map(Value::as_str)
        .collect();
    let mut edges: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for m in nodes.iter().chain(&groups) {
        let n = name(m);
        let refs = edges.entry(n.into()).or_default();
        for key in [
            "proxies",
            "dialer-proxy",
            "empty-fallback",
            "default-selected",
        ] {
            if let Some(v) = m.get(key) {
                let values: Vec<&str> = if let Some(a) = v.as_sequence() {
                    a.iter().filter_map(Value::as_str).collect()
                } else {
                    v.as_str().into_iter().filter(|s| !s.is_empty()).collect()
                };
                for r in values {
                    if !known.contains(r) {
                        return Err(Error::Invalid(format!("{n}.{key}: missing target {r}")));
                    }
                    if key == "empty-fallback" && groups.iter().any(|m| name(m) == r) {
                        return Err(Error::Invalid(format!(
                            "{n}: empty-fallback must be a node or built-in policy"
                        )));
                    }
                    refs.push(r.into());
                }
            }
        }
        if let Some(Value::Sequence(a)) = m.get("use") {
            for p in a.iter().filter_map(Value::as_str) {
                if !providers.contains(p) {
                    return Err(Error::Invalid(format!("{n}: missing provider {p}")));
                }
            }
        }
    }
    fn dfs(
        n: &str,
        edges: &BTreeMap<String, Vec<String>>,
        stack: &mut BTreeSet<String>,
        done: &mut BTreeSet<String>,
    ) -> Result<()> {
        if done.contains(n) {
            return Ok(());
        }
        if !stack.insert(n.into()) {
            return Err(Error::Invalid(format!(
                "proxy/group dependency cycle at {n}"
            )));
        }
        if let Some(next) = edges.get(n) {
            for r in next {
                dfs(r, edges, stack, done)?;
            }
        }
        stack.remove(n);
        done.insert(n.into());
        Ok(())
    }
    let mut done = BTreeSet::new();
    for n in edges.keys() {
        dfs(n, &edges, &mut BTreeSet::new(), &mut done)?;
    }
    Ok(())
}

pub fn catalog() -> serde_json::Value {
    serde_json::from_str(include_str!("proxy_catalog.json")).expect("bundled proxy catalogue")
}

pub fn supports_catalog(version: &str) -> bool {
    let parsed = version.split_whitespace().find_map(|word| {
        let numbers: Vec<u32> = word
            .trim_start_matches('v')
            .split('.')
            .map(str::parse)
            .collect::<std::result::Result<_, _>>()
            .ok()?;
        (numbers.len() == 3).then_some(numbers)
    });
    parsed.is_none_or(|v| v.as_slice() >= [1, 19, 32].as_slice())
}

pub fn fingerprint(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cfg(s: &str) -> Mapping {
        yaml::parse_mapping(s, "test").unwrap()
    }
    #[test]
    fn rename_is_scoped_and_preserves_unknown_fields() {
        let mut c = cfg(
            "proxies: [{name: A, type: direct, future: {x: 1}}]\nproxy-groups: [{name: G, type: select, proxies: [A]}]\nrules: ['DOMAIN,A,A', 'AND,((DOMAIN,x),(NETWORK,TCP)),A,no-resolve']",
        );
        let mut m = Managed::default();
        m.nodes.insert(
            "A".into(),
            Some(cfg("name: B\ntype: direct\nfuture: {x: 2}")),
        );
        m.redirects.insert("A".into(), "B".into());
        m.apply(&mut c);
        validate(&c).unwrap();
        assert_eq!(c["rules"][0].as_str(), Some("DOMAIN,A,B"));
        assert_eq!(c["proxy-groups"][0]["proxies"][0].as_str(), Some("B"));
        assert_eq!(c["proxies"][0]["future"]["x"].as_i64(), Some(2));
    }
    #[test]
    fn group_order_puts_listed_groups_first() {
        let mut c = cfg(
            "proxy-groups: [{name: A, type: select, proxies: [DIRECT]}, {name: B, type: select, proxies: [DIRECT]}, {name: C, type: select, proxies: [DIRECT]}]",
        );
        let m = Managed {
            group_order: vec!["C".into(), "Gone".into(), "A".into()],
            ..Default::default()
        };
        m.apply(&mut c);
        let names: Vec<_> = yaml::names(&c, "proxy-groups").collect();
        assert_eq!(names, ["C", "A", "B"]);
    }
    #[test]
    fn graph_rejects_collisions_missing_refs_and_cycles() {
        for s in [
            "proxies: [{name: DIRECT, type: direct}]",
            "proxy-groups: [{name: G, type: select, proxies: [G]}]",
            "proxies: [{name: A, type: direct, dialer-proxy: B}, {name: B, type: direct, dialer-proxy: A}]",
            "proxy-groups: [{name: G, type: select, proxies: [Missing]}]",
        ] {
            assert!(validate(&cfg(s)).is_err(), "{s}");
        }
    }
    #[test]
    fn catalog_covers_pinned_core() {
        let c = catalog();
        assert_eq!(c["nodes"].as_array().unwrap().len(), 28);
        assert_eq!(c["groups"].as_array().unwrap().len(), 4);
    }

    /// Parse every catalogue type with the real core, in an isolated home.
    /// The certificate and MASQUE key in the fixture are generated test data.
    #[tokio::test]
    async fn all_catalog_types_validate_with_real_core() {
        use serde_json::json;
        let Some(binary) = crate::runtime::test_binary() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        paths.ensure().unwrap();
        let runtime = crate::runtime::Runtime::new(paths, binary);
        let uuid = "00000000-0000-4000-8000-000000000001";
        let key = "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=";
        let extras: serde_json::Value =
            serde_json::from_str(include_str!("../tests/protocol-fixtures.json")).unwrap();
        let common = json!({
            "ss":{"cipher":"aes-128-gcm","password":"test"},"ssr":{"cipher":"aes-128-cfb","password":"test","protocol":"origin","obfs":"plain"},
            "vmess":{"uuid":uuid,"alterId":0,"cipher":"auto"},"vless":{"uuid":uuid},"snell":{"psk":"test","version":3},"trojan":{"password":"test"},
            "hysteria":{"auth-str":"test","up":"10","down":"100"},"hysteria2":{"password":"test"},"wireguard":{"private-key":key,"public-key":key,"ip":"172.16.0.2"},
            "tuic":{"uuid":uuid,"password":"test"},"shadowquic":{"password":"test"},"rematch":{"target-rematch-name":"DIRECT"},"ssh":{"username":"test","password":"test"},
            "mieru":{"username":"test","password":"test","transport":"TCP"},"anytls":{"password":"test"},"sudoku":{"key":"test"},"trusttunnel":{"username":"test","password":"test"}
        });
        for spec in catalog()["nodes"].as_array().unwrap() {
            let kind = spec["type"].as_str().unwrap();
            let mut node = json!({"name":"Test","type":kind});
            if ![
                "direct",
                "dns",
                "reject",
                "rematch",
                "tailscale",
                "zerotier",
                "easytier",
            ]
            .contains(&kind)
            {
                node["server"] = json!("example.net");
                node["port"] = json!(443);
            }
            for extra in [common.get(kind), extras.get(kind)].into_iter().flatten() {
                node.as_object_mut()
                    .unwrap()
                    .extend(extra.as_object().unwrap().clone());
            }
            let config = json!({"proxies":[node],"rules":["MATCH,DIRECT"]});
            runtime
                .validate(&serde_yaml_ng::to_string(&config).unwrap())
                .await
                .unwrap_or_else(|e| panic!("{kind}: {e}"));
        }
        for kind in ["select", "url-test", "fallback", "load-balance"] {
            let config = json!({"proxy-groups":[{"name":"G","type":kind,"proxies":["DIRECT"],"url":"https://example.net","interval":300}],"rules":["MATCH,G"]});
            runtime
                .validate(&serde_yaml_ng::to_string(&config).unwrap())
                .await
                .unwrap_or_else(|e| panic!("{kind}: {e}"));
        }
    }
}
