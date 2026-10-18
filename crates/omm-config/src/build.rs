//! The generation pipeline: profile + overrides + user rules + settings.

use std::collections::HashSet;

use crate::error::Result;
use crate::profiles::Overrides;
use crate::rules::{self, UserRule};
use crate::seq::SeqField;
use crate::settings::Settings;
use crate::yaml::{self, Mapping, Sequence, Value};

pub struct Inputs<'a> {
    /// Raw profile YAML.
    pub profile: &'a str,
    pub profile_name: &'a str,
    pub global: &'a Overrides,
    pub overrides: &'a Overrides,
    pub user_rules: &'a [UserRule],
    pub settings: &'a Settings,
}

#[derive(Debug)]
pub struct Output {
    pub config: Mapping,
    /// User rules skipped because their target is missing.
    pub orphans: Vec<i64>,
    /// Valid rule targets in this config: `DIRECT` and every group.
    pub groups: Vec<String>,
    /// Every name a rule may point at: groups, proxies, built-in policies.
    pub targets: Vec<String>,
}

impl Output {
    pub fn yaml(&self) -> Result<String> {
        yaml::to_string(&self.config)
    }
}

pub fn build(i: &Inputs) -> Result<Output> {
    build_inner(i, true)
}

/// Read an editable document even when a new subscription breaks managed refs.
pub fn inspect(i: &Inputs) -> Result<Output> {
    build_inner(i, false)
}

fn build_inner(i: &Inputs, validate_managed: bool) -> Result<Output> {
    let parsed = yaml::parse_mapping(i.profile, i.profile_name)?;
    // Keys are case-insensitive for mihomo but not for merging.
    let mut config: Mapping = parsed
        .into_iter()
        .map(|(k, v)| match k {
            Value::String(s) => (Value::from(s.to_ascii_lowercase()), v),
            k => (k, v),
        })
        .collect();

    for field in SeqField::ALL {
        i.overrides.seq(field).apply(&mut config, field);
    }
    for merge in [&i.global.merge, &i.overrides.merge].into_iter().flatten() {
        crate::merge::apply(&mut config, merge);
    }

    i.overrides.nodes.apply(&mut config);

    let groups: Vec<String> = yaml::names(&config, "proxy-groups").map(str::to_owned).collect();
    let targets: HashSet<&str> = groups.iter().map(String::as_str).chain(yaml::names(&config, "proxies")).collect();
    let user_rules: Vec<_> = i.user_rules.iter().cloned().map(|mut r| { r.target = i.overrides.nodes.target(&r.target); r }).collect();
    let rendered = rules::render(&user_rules, &targets);
    let mut all_targets: Vec<String> = rules::BUILTIN_TARGETS.iter().map(|s| (*s).to_owned()).chain(targets.iter().map(|s| (*s).to_owned())).collect();
    all_targets.sort();
    all_targets.dedup();
    if !rendered.lines.is_empty() {
        let mut list: Sequence = rendered.lines.into_iter().map(Value::from).collect();
        if let Some(Value::Sequence(existing)) = config.remove("rules") {
            list.extend(existing);
        }
        config.insert("rules".into(), Value::Sequence(list));
    }

    i.settings.enforce(&mut config);
    // Managed edits must fail on dangling references, never silently prune them.
    if i.overrides.nodes.nodes.is_empty() && i.overrides.nodes.groups.is_empty() && i.overrides.nodes.redirects.is_empty() {
        prune_group_members(&mut config);
    } else if validate_managed {
        crate::nodes::validate(&config)?;
    }

    Ok(Output {
        config: sorted(config),
        orphans: rendered.orphans,
        targets: all_targets,
        groups: std::iter::once("DIRECT".to_owned()).chain(groups.into_iter().filter(|g| g != "GLOBAL")).collect(),
    })
}

/// Drop group members that name nothing (left behind by deletes or merges);
/// mihomo refuses to start on them.
fn prune_group_members(config: &mut Mapping) {
    let mut known: HashSet<String> = rules::BUILTIN_TARGETS.iter().map(|s| (*s).to_owned()).collect();
    known.insert("COMPATIBLE".into());
    known.extend(yaml::names(config, "proxies").map(str::to_owned));
    known.extend(yaml::names(config, "proxy-groups").map(str::to_owned));

    let Some(Value::Sequence(groups)) = config.get_mut("proxy-groups") else { return };
    for group in groups.iter_mut().filter_map(Value::as_mapping_mut) {
        // Groups fed by providers or filters resolve members at runtime.
        let dynamic = ["use", "include-all", "include-all-proxies", "include-all-providers"].iter().any(|k| group.contains_key(*k));
        if dynamic {
            continue;
        }
        if let Some(Value::Sequence(members)) = group.get_mut("proxies") {
            members.retain(|m| m.as_str().is_none_or(|n| known.contains(n)));
        }
    }
}

/// Control keys first, bulky lists last, so the file reads well.
fn sorted(mut config: Mapping) -> Mapping {
    const HEAD: &[&str] = &[
        "mixed-port", "allow-lan", "mode", "log-level", "ipv6", "find-process-mode", "unified-delay", "tcp-concurrent",
        "external-controller-unix", "external-controller", "secret", "profile", "tun", "dns", "sniffer",
    ];
    const TAIL: &[&str] = &["proxies", "proxy-providers", "proxy-groups", "rule-providers", "sub-rules", "rules"];
    let mut out = Mapping::new();
    for k in HEAD {
        if let Some(v) = config.remove(*k) {
            out.insert(Value::from(*k), v);
        }
    }
    let tail: Vec<_> = TAIL.iter().filter_map(|k| config.remove(*k).map(|v| (Value::from(*k), v))).collect();
    out.extend(config);
    out.extend(tail);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::MatchKind;

    const PROFILE: &str = "
Mode: global
mixed-port: 7890
external-controller: 0.0.0.0:9090
base: &b {type: ss, server: 1.2.3.4, port: 443, cipher: aes-128-gcm, password: x}
proxies:
  - {name: FI, <<: *b}
  - {name: NL, <<: *b}
proxy-groups:
  - {name: AI, type: select, proxies: [FI, NL]}
  - {name: PROXY, type: fallback, proxies: [FI, NL, Gone], url: 'https://cp.cloudflare.com', interval: 300}
  - {name: Sub, type: select, use: [prov], proxies: [Whatever]}
rules:
  - DOMAIN-SUFFIX,google.com,PROXY
  - MATCH,DIRECT
";

    fn rule(id: i64, kind: MatchKind, value: &str, target: &str) -> UserRule {
        UserRule { id, kind, value: value.into(), target: target.into(), enabled: true }
    }

    #[test]
    fn full_pipeline() {
        let global = Overrides { merge: Some(yaml::parse_mapping("dns: {enable: true}", "g").unwrap()), ..Default::default() };
        let overrides = Overrides {
            rules: serde_yaml_ng::from_str("prepend: ['DOMAIN,profile.example,AI']").unwrap(),
            proxies: serde_yaml_ng::from_str("delete: [NL]").unwrap(),
            ..Default::default()
        };
        let user = [
            rule(1, MatchKind::DomainSuffix, "anthropic.com", "AI"),
            rule(2, MatchKind::Domain, "tiktok.com", "TikTok"),
        ];
        let settings = Settings::default();
        let out = build(&Inputs { profile: PROFILE, profile_name: "t", global: &global, overrides: &overrides, user_rules: &user, settings: &settings }).unwrap();
        let c = &out.config;

        let rules: Vec<_> = c["rules"].as_sequence().unwrap().iter().filter_map(Value::as_str).collect();
        assert_eq!(rules, ["DOMAIN-SUFFIX,anthropic.com,AI", "DOMAIN,profile.example,AI", "DOMAIN-SUFFIX,google.com,PROXY", "MATCH,DIRECT"]);
        assert_eq!(out.orphans, [2]);
        assert_eq!(out.groups, ["DIRECT", "AI", "PROXY", "Sub"]);

        assert_eq!(c["mode"], Value::from("rule"), "profile key `Mode` lowercased, then settings win");
        assert!(c.get("Mode").is_none());
        assert_eq!(c["proxies"][0]["server"], Value::from("1.2.3.4"), "merge keys resolved");
        assert_eq!(c["proxy-groups"][0]["proxies"], serde_yaml_ng::from_str::<Value>("[FI]").unwrap());
        assert_eq!(c["proxy-groups"][1]["proxies"], serde_yaml_ng::from_str::<Value>("[FI]").unwrap(), "deleted and dangling members pruned");
        assert_eq!(c["proxy-groups"][2]["proxies"], serde_yaml_ng::from_str::<Value>("[Whatever]").unwrap(), "provider groups untouched");
        assert_eq!(c["dns"]["enable"], Value::from(true));

        let keys: Vec<_> = c.keys().filter_map(Value::as_str).collect();
        assert_eq!(keys.first(), Some(&"mixed-port"));
        assert_eq!(keys.last(), Some(&"rules"));
    }
}
