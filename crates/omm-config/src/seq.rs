//! Sequence overrides: `{prepend: [...], append: [...], delete: [...]}` for
//! `rules`, `proxies` and `proxy-groups` (Clash Verge Rev's "rules/proxies/
//! groups" profile items).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::yaml::{Mapping, Sequence, Value, entry_name};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeqField {
    Rules,
    Proxies,
    ProxyGroups,
}

impl SeqField {
    pub const ALL: [SeqField; 3] = [SeqField::Rules, SeqField::Proxies, SeqField::ProxyGroups];

    pub fn key(self) -> &'static str {
        match self {
            SeqField::Rules => "rules",
            SeqField::Proxies => "proxies",
            SeqField::ProxyGroups => "proxy-groups",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SeqOverride {
    pub prepend: Sequence,
    pub append: Sequence,
    /// Rule strings, or proxy/group names, to drop from the profile.
    pub delete: Vec<String>,
}

impl SeqOverride {
    pub fn is_empty(&self) -> bool {
        self.prepend.is_empty() && self.append.is_empty() && self.delete.is_empty()
    }

    /// Result is `prepend + (profile − delete) + append`.
    ///
    /// For `proxies`, deleted names also disappear from every group, and added
    /// proxies are offered in the first `select` group so they are reachable.
    pub fn apply(&self, config: &mut Mapping, field: SeqField) {
        if self.is_empty() {
            return;
        }
        let deleted: HashSet<&str> = self.delete.iter().map(|s| s.trim()).collect();
        let existing = match config.remove(field.key()) {
            Some(Value::Sequence(s)) => s,
            _ => Sequence::new(),
        };

        let mut out = self.prepend.clone();
        out.extend(existing.into_iter().filter(|item| !entry_name(item).is_some_and(|n| deleted.contains(n.trim()))));
        out.extend(self.append.iter().cloned());
        config.insert(Value::from(field.key()), Value::Sequence(out));

        if field == SeqField::Proxies {
            let mut seen = HashSet::new();
            let added: Vec<String> = self
                .prepend
                .iter()
                .chain(&self.append)
                .filter_map(entry_name)
                .filter(|n| seen.insert(*n))
                .map(str::to_owned)
                .collect();
            fix_group_members(config, &deleted, &added);
        }
    }
}

fn fix_group_members(config: &mut Mapping, deleted: &HashSet<&str>, added: &[String]) {
    let Some(Value::Sequence(groups)) = config.get_mut("proxy-groups") else {
        return;
    };
    let mut offered = added.is_empty();
    for group in groups.iter_mut().filter_map(Value::as_mapping_mut) {
        let is_select = group
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|t| t.eq_ignore_ascii_case("select") || t.eq_ignore_ascii_case("selector"));
        let Some(Value::Sequence(members)) = group.get_mut("proxies") else {
            if !offered && is_select {
                let list = added.iter().map(|n| Value::from(n.as_str())).collect();
                group.insert(Value::from("proxies"), Value::Sequence(list));
                offered = true;
            }
            continue;
        };
        members.retain(|m| !m.as_str().is_some_and(|n| deleted.contains(n)));
        if !offered && is_select {
            let present: HashSet<String> = members.iter().filter_map(Value::as_str).map(str::to_owned).collect();
            let fresh = added.iter().filter(|n| !present.contains(*n)).map(|n| Value::from(n.as_str()));
            let mut merged: Sequence = fresh.collect();
            merged.append(members);
            *members = merged;
            offered = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::yaml::parse_mapping;

    const PROFILE: &str = "
proxies:
  - {name: A, type: ss}
  - {name: B, type: vless}
proxy-groups:
  - {name: Auto, type: url-test, proxies: [A, B]}
  - {name: Pick, type: select, proxies: [Auto, A, B, DIRECT]}
rules:
  - DOMAIN-SUFFIX,a.com,Pick
  - MATCH,DIRECT
";

    fn seq(y: &str) -> SeqOverride {
        serde_yaml_ng::from_str(y).unwrap()
    }

    #[test]
    fn rules_prepend_delete_append() {
        let mut c = parse_mapping(PROFILE, "p").unwrap();
        seq("prepend: ['DOMAIN,x.com,DIRECT']\nappend: ['IP-CIDR,1.1.1.1/32,DIRECT']\ndelete: [' DOMAIN-SUFFIX,a.com,Pick ']").apply(&mut c, SeqField::Rules);
        let rules: Vec<_> = c["rules"].as_sequence().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(rules, ["DOMAIN,x.com,DIRECT", "MATCH,DIRECT", "IP-CIDR,1.1.1.1/32,DIRECT"]);
    }

    #[test]
    fn deleting_a_proxy_removes_it_from_groups() {
        let mut c = parse_mapping(PROFILE, "p").unwrap();
        seq("delete: [A]").apply(&mut c, SeqField::Proxies);
        assert_eq!(names(&c, "proxies"), ["B"]);
        assert_eq!(members(&c, 0), ["B"]);
        assert_eq!(members(&c, 1), ["Auto", "B", "DIRECT"]);
    }

    #[test]
    fn added_proxies_go_into_first_selector_once() {
        let mut c = parse_mapping(PROFILE, "p").unwrap();
        seq("prepend: [{name: C, type: ss}]\nappend: [{name: A, type: ss}]").apply(&mut c, SeqField::Proxies);
        assert_eq!(names(&c, "proxies"), ["C", "A", "B", "A"]);
        assert_eq!(members(&c, 0), ["A", "B"], "url-test untouched");
        assert_eq!(members(&c, 1), ["C", "Auto", "A", "B", "DIRECT"]);
    }

    #[test]
    fn groups_override_by_name() {
        let mut c = parse_mapping(PROFILE, "p").unwrap();
        seq("prepend: [{name: AI, type: select, proxies: [A]}]\ndelete: [Auto]").apply(&mut c, SeqField::ProxyGroups);
        assert_eq!(names(&c, "proxy-groups"), ["AI", "Pick"]);
    }

    #[test]
    fn empty_override_is_noop() {
        let mut c = parse_mapping(PROFILE, "p").unwrap();
        let before = c.clone();
        SeqOverride::default().apply(&mut c, SeqField::Rules);
        assert_eq!(c, before);
    }

    fn names(c: &Mapping, key: &str) -> Vec<String> {
        crate::yaml::names(c, key).map(str::to_owned).collect()
    }

    fn members(c: &Mapping, group: usize) -> Vec<String> {
        c["proxy-groups"][group]["proxies"].as_sequence().unwrap().iter().map(|v| v.as_str().unwrap().to_owned()).collect()
    }
}
