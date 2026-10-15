//! Reading Clash Verge Rev's profiles for import.
//!
//! `profiles.yaml` lists items by `uid`. `remote` and `local` items are
//! configs; their `option` points at per-profile enhancement items by uid:
//! `merge` (a mapping deep-merged over the config), `rules`, `proxies` and
//! `groups` (`{prepend, append, delete}`, the same format as our sequence
//! overrides) and `script` (JavaScript, which we cannot run). The items
//! with uid `Merge` and `Script` apply to every profile.

use std::path::Path;

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::paths::read_optional;
use crate::profiles::Source;
use crate::rules::{self, MatchKind};
use crate::seq::SeqOverride;
use crate::yaml::parse_mapping;

/// site-autotune marks the rules file it owns with this comment.
const AUTOTUNE_MARKER: &str = "site-autotune";

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Index {
    current: Option<String>,
    items: Vec<Item>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Item {
    uid: String,
    #[serde(rename = "type")]
    kind: String,
    name: Option<String>,
    file: Option<String>,
    url: Option<String>,
    option: Option<ItemOption>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ItemOption {
    user_agent: Option<String>,
    /// Minutes.
    update_interval: Option<u64>,
    merge: Option<String>,
    script: Option<String>,
    rules: Option<String>,
    proxies: Option<String>,
    groups: Option<String>,
}

/// One Clash Verge profile, ready to become ours.
#[derive(Debug, Clone)]
pub struct Import {
    pub uid: String,
    pub name: String,
    pub source: Source,
    pub interval_hours: Option<u32>,
    pub body: String,
    /// Override files by our name (`merge.yaml`, `rules.yaml`, ...), only
    /// those with content.
    pub overrides: Vec<(&'static str, String)>,
    /// Rules site-autotune wrote into the profile's rules override; they
    /// become the user's own rules so they can be seen, changed and undone.
    pub autotune_rules: Vec<AutoRule>,
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AutoRule {
    pub kind: MatchKind,
    pub value: String,
    pub target: String,
}

/// Move the rules site-autotune prepended (`DOMAIN[-SUFFIX]` or `IP-CIDR`
/// lines to an `AUTO-*` group) out of a rules override. Returns what is left
/// of the override (`None` when nothing) and the moved rules.
pub fn split_autotune(text: &str) -> Result<(Option<String>, Vec<AutoRule>)> {
    if !text.contains(AUTOTUNE_MARKER) {
        return Ok((Some(text.to_owned()), Vec::new()));
    }
    let mut seq: SeqOverride = serde_yaml_ng::from_str::<Option<SeqOverride>>(text).map_err(|e| Error::yaml("site-autotune rules", e))?.unwrap_or_default();
    let mut moved = Vec::new();
    seq.prepend.retain(|item| {
        let Some(line) = item.as_str() else { return true };
        let parts: Vec<&str> = line.split(',').map(str::trim).collect();
        let kind = match parts.first().copied() {
            Some("DOMAIN") => MatchKind::Domain,
            Some("DOMAIN-SUFFIX") => MatchKind::DomainSuffix,
            Some("IP-CIDR") | Some("IP-CIDR6") => MatchKind::IpCidr,
            _ => return true,
        };
        let (Some(value), Some(target)) = (parts.get(1), parts.get(2)) else { return true };
        if !target.starts_with("AUTO-") {
            return true;
        }
        match rules::normalize(kind, value) {
            Ok(value) => {
                moved.push(AutoRule { kind, value, target: (*target).to_owned() });
                false
            }
            Err(_) => true,
        }
    });
    let rest = if seq.is_empty() { None } else { Some(serde_yaml_ng::to_string(&seq).map_err(|e| Error::yaml("rules override", e))?) };
    Ok((rest, moved))
}

#[derive(Debug, Default)]
pub struct Found {
    pub profiles: Vec<Import>,
    /// Contents of the global `Merge` item, if it does anything.
    pub global_merge: Option<String>,
    /// What could not be carried over.
    pub warnings: Vec<String>,
}

/// The JavaScript template Clash Verge creates; anything else is real code.
fn trivial_script(text: &str) -> bool {
    let code: String = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("//")).collect();
    code == "function main(config, profileName) {return config;}" || code == "function main(config) {return config;}"
}

/// Text of an enhancement item, `None` when it has no effect.
fn enhancement(dir: &Path, items: &[Item], uid: Option<&str>) -> Result<Option<(String, String)>> {
    let Some(item) = uid.and_then(|u| items.iter().find(|i| i.uid == u)) else { return Ok(None) };
    let Some(file) = &item.file else { return Ok(None) };
    let Some(text) = read_optional(&dir.join("profiles").join(file))? else { return Ok(None) };
    let empty = match item.kind.as_str() {
        "script" => trivial_script(&text),
        _ => match serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&text) {
            Ok(serde_yaml_ng::Value::Null) => true,
            Ok(serde_yaml_ng::Value::Mapping(m)) => m.values().all(|v| v.is_null() || v.as_sequence().is_some_and(|s| s.is_empty())),
            _ => false,
        },
    };
    Ok((!empty).then(|| (item.kind.clone(), text)))
}

/// Read Clash Verge's profiles from its data directory.
pub fn read(dir: &Path) -> Result<Found> {
    let index_text = read_optional(&dir.join("profiles.yaml"))?.ok_or_else(|| Error::Invalid(format!("no profiles.yaml in {}", dir.display())))?;
    let index: Index = serde_yaml_ng::from_str(&index_text).map_err(|e| Error::yaml("Clash Verge profiles.yaml", e))?;
    let mut found = Found { global_merge: enhancement(dir, &index.items, Some("Merge"))?.map(|(_, t)| t), ..Found::default() };
    if enhancement(dir, &index.items, Some("Script"))?.is_some() {
        found.warnings.push("the global Script was not imported: JavaScript enhancements are not supported".into());
    }

    for item in index.items.iter().filter(|i| i.kind == "remote" || i.kind == "local") {
        let name = item.name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| item.uid.clone());
        let Some(body) = item.file.as_ref().map(|f| dir.join("profiles").join(f)).map(|p| read_optional(&p)).transpose()?.flatten() else {
            found.warnings.push(format!("{name}: its file is missing, skipped"));
            continue;
        };
        if let Err(e) = parse_mapping(&body, &name) {
            found.warnings.push(format!("{name}: {e}, skipped"));
            continue;
        }
        let opt = item.option.as_ref();
        let source = match (item.kind.as_str(), item.url.as_deref().filter(|u| !u.is_empty())) {
            ("remote", Some(url)) => Source::Remote { url: url.into(), user_agent: opt.and_then(|o| o.user_agent.clone()) },
            _ => Source::Local,
        };
        let mut overrides = Vec::new();
        let mut autotune_rules = Vec::new();
        if let Some(opt) = opt {
            for (ours, uid) in [("merge.yaml", &opt.merge), ("rules.yaml", &opt.rules), ("proxies.yaml", &opt.proxies), ("groups.yaml", &opt.groups)] {
                if let Some((_, text)) = enhancement(dir, &index.items, uid.as_deref())? {
                    if ours == "rules.yaml" {
                        let (rest, moved) = split_autotune(&text)?;
                        autotune_rules = moved;
                        if let Some(rest) = rest {
                            overrides.push((ours, rest));
                        }
                    } else {
                        overrides.push((ours, text));
                    }
                }
            }
            if enhancement(dir, &index.items, opt.script.as_deref())?.is_some() {
                found.warnings.push(format!("{name}: its Script was not imported: JavaScript enhancements are not supported"));
            }
        }
        let interval_hours = opt.and_then(|o| o.update_interval).filter(|m| *m > 0).map(|m| m.div_ceil(60).min(u64::from(u32::MAX)) as u32);
        found.profiles.push(Import {
            uid: item.uid.clone(),
            name,
            source,
            interval_hours,
            body,
            overrides,
            autotune_rules,
            current: index.current.as_deref() == Some(item.uid.as_str()),
        });
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::yaml::Value;

    #[test]
    fn autotune_rules_move_out_of_the_override() {
        let text = "# Заполняется автоматически site-autotune/tuner.py — правки будут перезаписаны.\nprepend:\n- DOMAIN-SUFFIX,herdr.dev,AUTO-herdr-dev\n- DOMAIN-SUFFIX,Wttr.in,AUTO-wttr-in\n- DOMAIN-SUFFIX,keep.me,PROXY\nappend: []\ndelete: []\n";
        let (rest, moved) = split_autotune(text).unwrap();
        assert_eq!(
            moved,
            [
                AutoRule { kind: MatchKind::DomainSuffix, value: "herdr.dev".into(), target: "AUTO-herdr-dev".into() },
                AutoRule { kind: MatchKind::DomainSuffix, value: "wttr.in".into(), target: "AUTO-wttr-in".into() },
            ]
        );
        let rest: SeqOverride = serde_yaml_ng::from_str(&rest.unwrap()).unwrap();
        assert_eq!(rest.prepend, vec![Value::from("DOMAIN-SUFFIX,keep.me,PROXY")]);

        let only_auto = "# site-autotune\nprepend:\n- DOMAIN-SUFFIX,a.com,AUTO-a-com\n";
        assert_eq!(split_autotune(only_auto).unwrap().0, None, "nothing left, no override");

        let manual = "prepend:\n- DOMAIN-SUFFIX,a.com,AUTO-a-com\n";
        assert_eq!(split_autotune(manual).unwrap(), (Some(manual.to_owned()), vec![]), "unmarked files are the user's own");
    }

    #[test]
    fn reads_profiles_with_enhancements() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("profiles");
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(
            dir.path().join("profiles.yaml"),
            "current: L1\nitems:\n\
             - {uid: Merge, type: merge, file: Merge.yaml}\n\
             - {uid: Script, type: script, file: Script.js}\n\
             - {uid: R1, type: rules, file: R1.yaml}\n\
             - {uid: G1, type: groups, file: G1.yaml}\n\
             - {uid: S1, type: script, file: S1.js}\n\
             - {uid: L1, type: local, name: Config.yml, file: L1.yaml, option: {rules: R1, groups: G1, script: S1}}\n\
             - {uid: W1, type: remote, name: Sub, file: W1.yaml, url: 'https://x/sub', option: {user_agent: ua, update_interval: 90}}\n",
        )
        .unwrap();
        std::fs::write(p.join("Merge.yaml"), "# template\nprofile:\n  store-selected: true\n").unwrap();
        std::fs::write(p.join("Script.js"), "// Define main function (script entry)\n\nfunction main(config, profileName) {\n  return config;\n}\n").unwrap();
        std::fs::write(p.join("R1.yaml"), "prepend:\n  - MATCH,DIRECT\n").unwrap();
        std::fs::write(p.join("G1.yaml"), "prepend: []\nappend: []\ndelete: []\n").unwrap();
        std::fs::write(p.join("S1.js"), "function main(c) { c.mode = 'global'; return c }\n").unwrap();
        std::fs::write(p.join("L1.yaml"), "proxies: []\n").unwrap();
        std::fs::write(p.join("W1.yaml"), "proxies: []\n").unwrap();

        let f = read(dir.path()).unwrap();
        assert!(f.global_merge.is_some());
        assert_eq!(f.profiles.len(), 2);
        let l = &f.profiles[0];
        assert!(l.current && l.name == "Config.yml" && l.source == Source::Local);
        assert_eq!(l.overrides.iter().map(|(n, _)| *n).collect::<Vec<_>>(), ["rules.yaml"], "empty groups skipped");
        let w = &f.profiles[1];
        assert_eq!(w.source, Source::Remote { url: "https://x/sub".into(), user_agent: Some("ua".into()) });
        assert_eq!(w.interval_hours, Some(2));
        assert_eq!(f.warnings.len(), 1, "only the real per-profile script: {:?}", f.warnings);
    }
}
