//! What to offer while a rule is typed, and what a rule would cover.
//!
//! Sources: live connections, hosts and processes the core logged in the
//! last day, open problems, running programs, the GeoSite database and the
//! profile's rule providers.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::IpAddr;
use std::path::Path;

use mihomo_api::model::{Connection, Metadata};
use omm_config::rules::{Conn, MatchKind, UserRule, normalize, registrable};
use serde::{Deserialize, Serialize};

use crate::state::State;

const PER_SECTION: usize = 6;

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Item {
    /// What goes into the field.
    pub value: String,
    /// `common`, `conn`, `log`, `problem`, `running`, `window`, `user`,
    /// `known`, `db`, `provider`.
    pub source: &'static str,
    /// Secondary line: process, path, example domains...
    #[serde(skip_serializing_if = "String::is_empty")]
    pub detail: String,
    pub count: u64,
    /// Route it takes now.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub route: String,
    /// Unix ms of the last sighting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last: Option<i64>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub failed: bool,
    /// The GeoSite category is already in the profile's rules.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub used_by: String,
}

#[derive(Debug, Default, Serialize)]
pub struct Suggestions {
    pub items: Vec<Item>,
    /// `no-geosite` when the database has not been downloaded yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<&'static str>,
}

/// A connection as the rule matcher sees it.
pub fn conn(m: &Metadata) -> Conn<'_> {
    Conn {
        host: if m.host.is_empty() { &m.sniff_host } else { &m.host },
        ip: m.destination_ip.parse().ok(),
        src_ip: m.source_ip.parse().ok(),
        dst_port: m.destination_port.parse().ok(),
        src_port: m.source_port.parse().ok(),
        network: &m.network,
        process: &m.process,
        process_path: &m.process_path,
        uid: (!m.process.is_empty()).then_some(m.uid),
    }
}

/// `Group → Node`, top group first.
fn route_of(c: &Connection) -> String {
    c.chains.iter().rev().cloned().collect::<Vec<_>>().join(" → ")
}

fn host_of(m: &Metadata) -> &str {
    if m.host.is_empty() { &m.sniff_host } else { &m.host }
}

fn is_ip(s: &str) -> bool {
    s.parse::<IpAddr>().is_ok()
}

/// The query as it would be typed into a domain field.
fn domain_query(q: &str) -> String {
    let q = q.trim().to_lowercase();
    let q = q.split_once("://").map_or(q.as_str(), |(_, r)| r);
    let q = q.split(['/', '?', '#']).next().unwrap_or(q);
    q.trim_start_matches("*.").trim_start_matches('.').trim_end_matches('.').to_owned()
}

pub async fn suggest(state: &State, kind: MatchKind, query: &str) -> Suggestions {
    let conns = state.core.connections().await.map(|c| c.connections).unwrap_or_default();
    let now = crate::problems::now_ms();
    let seen_hosts = state.seen.lock().unwrap().hosts(now);
    let seen_procs = state.seen.lock().unwrap().processes(now);
    use MatchKind::*;
    match kind {
        Domain | DomainSuffix | DomainKeyword | DomainWildcard | DomainRegex => {
            let problems: Vec<(String, u64, String)> = {
                let c = state.problems.lock().unwrap();
                c.list(&omm_problems::Query::default()).into_iter().map(|v| (v.problem.host.clone(), v.problem.count, v.problem.route.target.clone())).collect()
            };
            Suggestions { items: domains(&conns, &seen_hosts, &problems, &domain_query(query)), note: None }
        }
        IpCidr | IpSuffix | SrcIpCidr => Suggestions { items: addresses(&conns, &seen_hosts, kind == SrcIpCidr, query.trim()), note: None },
        ProcessName | ProcessPath | ProcessNameRegex | ProcessPathRegex => {
            let mut programs = tokio::task::spawn_blocking(|| crate::procs::programs(Path::new("/proc"))).await.unwrap_or_default();
            let windows = crate::procs::windows().await;
            crate::procs::attach_windows(&mut programs, &windows, Path::new("/proc"));
            let by_path = matches!(kind, ProcessPath | ProcessPathRegex);
            Suggestions { items: processes(&conns, &programs, &seen_procs, by_path, query.trim()), note: None }
        }
        Uid => {
            let passwd = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
            let q = query.trim().to_lowercase();
            let items = crate::procs::users(&passwd)
                .into_iter()
                .filter(|(uid, name)| q.is_empty() || uid.to_string().starts_with(&q) || name.to_lowercase().contains(&q))
                .map(|(uid, name)| {
                    let count = conns.iter().filter(|c| !c.metadata.process.is_empty() && c.metadata.uid == uid).count() as u64;
                    Item { value: uid.to_string(), source: "user", detail: name, count, ..Default::default() }
                })
                .collect();
            Suggestions { items, note: None }
        }
        DstPort | SrcPort => Suggestions { items: ports(&conns, kind == SrcPort, query.trim()), note: None },
        Network => Suggestions {
            items: ["tcp", "udp"]
                .into_iter()
                .filter(|n| n.starts_with(&query.trim().to_lowercase()))
                .map(|n| Item { value: n.into(), source: "known", count: conns.iter().filter(|c| c.metadata.network == n).count() as u64, ..Default::default() })
                .collect(),
            note: None,
        },
        Geosite => {
            let home = state.config.paths().runtime();
            let used = profile_geosites(state).await;
            let q = query.trim().to_lowercase();
            let q = q.strip_prefix("geosite:").unwrap_or(&q).to_owned();
            let found = tokio::task::spawn_blocking(move || {
                let path = crate::geosite::find(&home)?;
                let data = std::fs::read(path).ok()?;
                Some(crate::geosite::search(&data, &q, 12))
            })
            .await
            .ok()
            .flatten();
            match found {
                None => Suggestions { items: Vec::new(), note: Some("no-geosite") },
                Some(cats) => Suggestions {
                    items: cats
                        .into_iter()
                        .map(|c| Item {
                            used_by: used.get(&c.name).cloned().unwrap_or_default(),
                            value: c.name,
                            source: "db",
                            detail: c.samples.join(" · "),
                            count: c.size as u64,
                            ..Default::default()
                        })
                        .collect(),
                    note: None,
                },
            }
        }
        Geoip => Suggestions { items: known(GEOIP, query), note: None },
        IpAsn => Suggestions { items: known(ASNS, query.trim().trim_start_matches(['A', 'a']).trim_start_matches(['S', 's'])), note: None },
        RuleSet => {
            let q = query.trim().to_lowercase();
            let items = state
                .core
                .rule_providers()
                .await
                .map(|p| p.providers)
                .unwrap_or_default()
                .into_values()
                // `geosite:openai` finds the set added from it as `geosite-openai`.
                .filter(|p| q.is_empty() || p.name.to_lowercase().contains(&q) || p.name.to_lowercase().contains(&q.replace(':', "-")))
                .map(|p| Item { detail: p.behavior.to_lowercase(), count: p.rule_count, value: p.name, source: "provider", ..Default::default() })
                .collect::<Vec<_>>();
            // Ready-made lists the user can add with one pick; typing
            // `geosite:<anything>` offers that category too.
            let mut specs: Vec<String> = omm_config::rule_sets::POPULAR.iter().map(|s| (*s).to_owned()).collect();
            if q.contains(':') && !specs.contains(&q) {
                specs.insert(0, q.clone());
            }
            let catalog: Vec<Item> = specs
                .into_iter()
                .filter_map(|spec| omm_config::rule_sets::catalog(&spec).map(|set| (spec, set)))
                .filter(|(spec, set)| (q.is_empty() || spec.contains(&q) || set.name.contains(&q)) && !items.iter().any(|i| i.value == set.name))
                .take(12)
                .map(|(spec, set)| Item { value: set.name, detail: spec, source: "catalog", ..Default::default() })
                .collect();
            let mut items = items;
            items.extend(catalog);
            Suggestions { items, note: None }
        }
        And | Or | Not => Suggestions::default(),
    }
}

fn domains(conns: &[Connection], seen: &[crate::seen::Host], problems: &[(String, u64, String)], q: &str) -> Vec<Item> {
    let wanted = |h: &str| !h.is_empty() && !is_ip(h) && (q.is_empty() || h.contains(q));
    let mut live: BTreeMap<String, Item> = BTreeMap::new();
    for c in conns {
        let h = host_of(&c.metadata).to_ascii_lowercase();
        if !wanted(&h) {
            continue;
        }
        let it = live.entry(h.clone()).or_insert_with(|| Item { value: h, source: "conn", detail: c.metadata.process.clone(), route: route_of(c), ..Default::default() });
        it.count += 1;
    }
    let mut live: Vec<Item> = live.into_values().collect();
    live.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.value.cmp(&b.value)));
    let logged: Vec<Item> = seen
        .iter()
        .filter(|h| wanted(&h.host) && !live.iter().any(|l| l.value == h.host))
        .map(|h| Item { value: h.host.clone(), source: "log", detail: h.process.clone().unwrap_or_default(), count: h.count as u64, route: h.route.clone(), last: Some(h.last), failed: h.failed, ..Default::default() })
        .collect();
    let failing: Vec<Item> = problems
        .iter()
        .filter(|(h, ..)| wanted(h))
        .map(|(h, count, target)| Item { value: h.clone(), source: "problem", count: *count, route: target.clone(), failed: true, ..Default::default() })
        .collect();

    // A domain shared by several of the hosts found: offer the suffix.
    let mut common = Vec::new();
    if !q.is_empty() {
        let mut groups: BTreeMap<String, HashSet<&str>> = BTreeMap::new();
        for it in live.iter().chain(&logged).chain(&failing) {
            groups.entry(registrable(&it.value)).or_default().insert(it.value.as_str());
        }
        for (domain, hosts) in groups {
            if hosts.len() >= 2 || (hosts.len() == 1 && !hosts.contains(domain.as_str()) && domain.contains(q)) {
                let mut examples: Vec<&str> = hosts.iter().copied().filter(|h| *h != domain).collect();
                examples.sort();
                examples.truncate(3);
                common.push(Item { value: domain, source: "common", detail: examples.join(" · "), count: hosts.len() as u64, ..Default::default() });
            }
        }
        common.sort_by_key(|x| std::cmp::Reverse(x.count));
        common.truncate(2);
    }
    [common, live, logged, failing].into_iter().flat_map(|s| s.into_iter().take(PER_SECTION)).collect()
}

fn addresses(conns: &[Connection], seen: &[crate::seen::Host], source: bool, q: &str) -> Vec<Item> {
    let mut live: BTreeMap<String, Item> = BTreeMap::new();
    for c in conns {
        let m = &c.metadata;
        let ip = if source { &m.source_ip } else { &m.destination_ip };
        if ip.is_empty() || !(q.is_empty() || ip.starts_with(q)) {
            continue;
        }
        let detail = if source { m.process.clone() } else { host_of(m).to_owned() };
        let it = live.entry(ip.clone()).or_insert_with(|| Item { value: ip.clone(), source: "conn", detail, route: route_of(c), ..Default::default() });
        it.count += 1;
    }
    let mut live: Vec<Item> = live.into_values().collect();
    live.sort_by_key(|x| std::cmp::Reverse(x.count));
    let logged: Vec<Item> = if source {
        Vec::new()
    } else {
        seen.iter()
            .filter(|h| is_ip(&h.host) && (q.is_empty() || h.host.starts_with(q)) && !live.iter().any(|l| l.value == h.host))
            .map(|h| Item { value: h.host.clone(), source: "log", detail: h.process.clone().unwrap_or_default(), count: h.count as u64, route: h.route.clone(), last: Some(h.last), failed: h.failed, ..Default::default() })
            .collect()
    };
    [live, logged].into_iter().flat_map(|s| s.into_iter().take(PER_SECTION)).collect()
}

fn processes(conns: &[Connection], programs: &[crate::procs::Program], seen: &[crate::seen::Process], by_path: bool, q: &str) -> Vec<Item> {
    let q = q.to_lowercase();
    let wanted = |name: &str, path: &str| q.is_empty() || name.to_lowercase().contains(&q) || (by_path && path.to_lowercase().contains(&q));
    let mut per_name: HashMap<&str, (u64, String)> = HashMap::new();
    for c in conns.iter().filter(|c| !c.metadata.process.is_empty()) {
        let e = per_name.entry(c.metadata.process.as_str()).or_insert((0, route_of(c)));
        e.0 += 1;
    }
    let mut net = Vec::new();
    let mut windowed = Vec::new();
    let mut other = Vec::new();
    let mut running: HashSet<&str> = HashSet::new();
    // By name, copies of one program at several paths are one choice.
    let mut copies: HashMap<&str, usize> = HashMap::new();
    for p in programs.iter().filter(|p| wanted(&p.name, &p.path)) {
        if !by_path {
            let n = copies.entry(p.name.as_str()).or_default();
            *n += 1;
            if *n > 1 {
                continue;
            }
        }
        running.insert(&p.name);
        let (count, route) = per_name.get(p.name.as_str()).cloned().unwrap_or_default();
        let detail = match &p.window {
            Some(w) => format!("{} · «{w}»", p.path),
            None => p.path.clone(),
        };
        let value = if by_path { p.path.clone() } else { p.name.clone() };
        let it = Item { value, source: if count > 0 { "conn" } else if p.window.is_some() { "window" } else { "running" }, detail, count, route, ..Default::default() };
        match it.source {
            "conn" => net.push(it),
            "window" => windowed.push(it),
            _ => other.push(it),
        }
    }
    for it in net.iter_mut().chain(windowed.iter_mut()).chain(other.iter_mut()) {
        if let Some(n) = copies.get(it.value.as_str()).filter(|n| **n > 1) {
            it.detail = format!("{} · +{}", it.detail, n - 1);
        }
    }
    net.sort_by_key(|x| std::cmp::Reverse(x.count));
    // Without a query, background daemons are noise.
    if q.is_empty() {
        other.clear();
    }
    let logged: Vec<Item> = if by_path {
        Vec::new()
    } else {
        seen.iter()
            .filter(|p| wanted(&p.name, "") && !running.contains(p.name.as_str()))
            .map(|p| Item { value: p.name.clone(), source: "log", count: p.count as u64, last: Some(p.last), ..Default::default() })
            .collect()
    };
    [net, windowed, other, logged].into_iter().flat_map(|s| s.into_iter().take(PER_SECTION)).collect()
}

const WELL_KNOWN_PORTS: &[(u16, &str)] =
    &[(443, "https"), (80, "http"), (53, "dns"), (853, "dns-over-tls"), (22, "ssh"), (123, "ntp"), (3478, "stun"), (5222, "xmpp"), (6881, "bittorrent"), (51820, "wireguard"), (1194, "openvpn"), (8080, "http-alt"), (993, "imaps"), (587, "smtp")];

fn ports(conns: &[Connection], source: bool, q: &str) -> Vec<Item> {
    let mut counts: BTreeMap<u16, (u64, String)> = BTreeMap::new();
    for c in conns {
        let p = if source { &c.metadata.source_port } else { &c.metadata.destination_port };
        if let Ok(p) = p.parse::<u16>() {
            let e = counts.entry(p).or_insert((0, host_of(&c.metadata).to_owned()));
            e.0 += 1;
        }
    }
    let name = |p: u16| WELL_KNOWN_PORTS.iter().find(|(n, _)| *n == p).map(|(_, s)| (*s).to_owned()).unwrap_or_default();
    let mut live: Vec<Item> = counts
        .into_iter()
        .filter(|(p, _)| q.is_empty() || p.to_string().starts_with(q))
        .map(|(p, (count, host))| Item { value: p.to_string(), source: "conn", detail: [name(p), host].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · "), count, ..Default::default() })
        .collect();
    live.sort_by_key(|x| std::cmp::Reverse(x.count));
    live.truncate(PER_SECTION);
    let known: Vec<Item> = if source {
        Vec::new()
    } else {
        WELL_KNOWN_PORTS
            .iter()
            .filter(|(p, n)| (q.is_empty() || p.to_string().starts_with(q) || n.contains(q)) && !live.iter().any(|l| l.value == p.to_string()))
            .take(PER_SECTION)
            .map(|(p, n)| Item { value: p.to_string(), source: "known", detail: (*n).into(), ..Default::default() })
            .collect()
    };
    live.into_iter().chain(known).collect()
}

fn known(list: &[(&str, &str)], q: &str) -> Vec<Item> {
    let q = q.trim().to_lowercase();
    list.iter()
        .filter(|(v, name)| q.is_empty() || v.to_lowercase().starts_with(&q) || name.to_lowercase().contains(&q))
        .take(12)
        .map(|(v, name)| Item { value: (*v).into(), source: "known", detail: (*name).into(), ..Default::default() })
        .collect()
}

/// GeoSite categories the generated config already routes, and where.
async fn profile_geosites(state: &State) -> HashMap<String, String> {
    let Ok(rules) = state.core.rules().await else { return HashMap::new() };
    rules.rules.into_iter().filter(|r| r.kind == "GeoSite").map(|r| (r.payload.to_lowercase(), r.proxy)).collect()
}

const GEOIP: &[(&str, &str)] = &[
    ("private", "локальные сети / LAN"),
    ("ru", "Россия · Russia"),
    ("us", "США · United States"),
    ("de", "Германия · Germany"),
    ("nl", "Нидерланды · Netherlands"),
    ("fi", "Финляндия · Finland"),
    ("gb", "Великобритания · United Kingdom"),
    ("fr", "Франция · France"),
    ("se", "Швеция · Sweden"),
    ("ch", "Швейцария · Switzerland"),
    ("pl", "Польша · Poland"),
    ("ee", "Эстония · Estonia"),
    ("lv", "Латвия · Latvia"),
    ("lt", "Литва · Lithuania"),
    ("kz", "Казахстан · Kazakhstan"),
    ("by", "Беларусь · Belarus"),
    ("ua", "Украина · Ukraine"),
    ("ge", "Грузия · Georgia"),
    ("am", "Армения · Armenia"),
    ("tr", "Турция · Turkey"),
    ("ae", "ОАЭ · UAE"),
    ("il", "Израиль · Israel"),
    ("ir", "Иран · Iran"),
    ("cn", "Китай · China"),
    ("hk", "Гонконг · Hong Kong"),
    ("tw", "Тайвань · Taiwan"),
    ("jp", "Япония · Japan"),
    ("kr", "Корея · South Korea"),
    ("sg", "Сингапур · Singapore"),
    ("in", "Индия · India"),
    ("ca", "Канада · Canada"),
    ("br", "Бразилия · Brazil"),
    ("au", "Австралия · Australia"),
    ("it", "Италия · Italy"),
    ("es", "Испания · Spain"),
];

const ASNS: &[(&str, &str)] = &[
    ("62041", "Telegram Messenger"),
    ("59930", "Telegram Messenger"),
    ("44907", "Telegram Messenger"),
    ("211157", "Telegram Messenger"),
    ("13335", "Cloudflare"),
    ("15169", "Google"),
    ("396982", "Google Cloud"),
    ("32934", "Meta (Facebook, Instagram, WhatsApp)"),
    ("16509", "Amazon AWS"),
    ("14618", "Amazon AWS"),
    ("8075", "Microsoft"),
    ("714", "Apple"),
    ("2906", "Netflix"),
    ("54113", "Fastly"),
    ("20940", "Akamai"),
    ("13238", "Yandex"),
    ("47541", "VK"),
    ("24940", "Hetzner"),
    ("16276", "OVH"),
    ("14061", "DigitalOcean"),
    ("20473", "Vultr"),
    ("36459", "GitHub"),
    ("46489", "Twitch"),
    ("138699", "TikTok (ByteDance)"),
    ("396986", "TikTok (ByteDance)"),
];

#[derive(Debug, Default, Serialize, PartialEq)]
pub struct Route {
    pub rule: String,
    pub payload: String,
    pub target: String,
    pub count: u64,
}

/// What a rule covers right now.
#[derive(Debug, Default, Serialize, PartialEq)]
pub struct Coverage {
    /// Live connections it matches.
    pub connections: u64,
    /// Hosts logged in the last day it matches.
    pub seen: u64,
    /// Some connections could not be judged (GeoSite, GeoIP, rule sets).
    pub unknown: bool,
    /// How the matched connections are routed now, busiest first.
    pub routes: Vec<Route>,
}

pub fn coverage(rule: &UserRule, conns: &[Connection], seen: &[crate::seen::Host]) -> Coverage {
    let mut out = Coverage::default();
    let mut routes: BTreeMap<(String, String, String), u64> = BTreeMap::new();
    for c in conns {
        match rule.matches(&conn(&c.metadata)) {
            Some(true) => {
                out.connections += 1;
                let target = c.chains.last().cloned().unwrap_or_default();
                *routes.entry((c.rule.clone(), c.rule_payload.clone(), target)).or_default() += 1;
            }
            None => out.unknown = true,
            Some(false) => {}
        }
    }
    for h in seen {
        let ip = h.host.parse().ok();
        let c = Conn { host: if ip.is_some() { "" } else { &h.host }, ip, process: h.process.as_deref().unwrap_or_default(), ..Default::default() };
        if rule.matches(&c) == Some(true) {
            out.seen += 1;
        }
    }
    out.routes = routes.into_iter().map(|((rule, payload, target), count)| Route { rule, payload, target, count }).collect();
    out.routes.sort_by_key(|x| std::cmp::Reverse(x.count));
    out.routes.truncate(4);
    out
}

/// A connection, as the UI knows it, to derive rule scopes from.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Origin {
    pub host: String,
    pub ip: String,
    pub process: String,
    pub process_path: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Scope {
    pub kind: MatchKind,
    pub value: String,
    #[serde(flatten)]
    pub coverage: Coverage,
}

/// Rule scopes a connection suggests, narrowest first: exact host, its
/// registrable domain, the program, the address.
pub fn scopes(o: &Origin, conns: &[Connection], seen: &[crate::seen::Host]) -> Vec<Scope> {
    let mut candidates = Vec::new();
    let host = o.host.trim();
    if !host.is_empty() && !is_ip(host) {
        candidates.push((MatchKind::Domain, host.to_owned()));
        candidates.push((MatchKind::DomainSuffix, registrable(host)));
    }
    if !o.process.trim().is_empty() {
        candidates.push((MatchKind::ProcessName, o.process.trim().to_owned()));
    }
    // Without a host (a window, not a connection) the executable is the
    // other natural scope.
    if host.is_empty() && o.ip.trim().is_empty() && !o.process_path.trim().is_empty() {
        candidates.push((MatchKind::ProcessPath, o.process_path.trim().to_owned()));
    }
    let ip = if o.ip.trim().is_empty() && is_ip(host) { host } else { o.ip.trim() };
    if !ip.is_empty() {
        candidates.push((MatchKind::IpCidr, ip.to_owned()));
    }
    candidates
        .into_iter()
        .filter_map(|(kind, raw)| {
            let value = normalize(kind, &raw).ok()?;
            let rule = UserRule { id: 0, kind, value: value.clone(), target: String::new(), enabled: true };
            Some(Scope { kind, value, coverage: coverage(&rule, conns, seen) })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(host: &str, ip: &str, process: &str, port: &str, chains: &[&str]) -> Connection {
        Connection {
            id: host.into(),
            metadata: Metadata { host: host.into(), destination_ip: ip.into(), process: process.into(), destination_port: port.into(), network: "tcp".into(), uid: 1000, ..Default::default() },
            chains: chains.iter().map(|s| (*s).to_owned()).collect(),
            rule: "DomainSuffix".into(),
            rule_payload: "anthropic.com".into(),
            ..Default::default()
        }
    }

    fn seen(host: &str, process: &str) -> crate::seen::Host {
        crate::seen::Host { host: host.into(), last: 5, count: 2, process: Some(process.into()), route: "AI".into(), failed: false }
    }

    #[test]
    fn domain_suggestions_offer_the_shared_domain_first() {
        let conns = [c("api.anthropic.com", "160.79.104.10", "claude", "443", &["NY", "AI"]), c("api.anthropic.com", "160.79.104.10", "claude", "443", &["NY", "AI"]), c("statsig.anthropic.com", "1.1.1.1", "claude", "443", &["AI"])];
        let logged = [seen("console.anthropic.com", "firefox"), seen("api.anthropic.com", "claude"), seen("example.org", "x")];
        let problems = [("api.anthropic.com".to_owned(), 41, "DIRECT".to_owned())];
        let items = domains(&conns, &logged, &problems, &domain_query("https://ANTHR"));
        let short: Vec<(&str, &str, u64)> = items.iter().map(|i| (i.source, i.value.as_str(), i.count)).collect();
        assert_eq!(
            short,
            [
                ("common", "anthropic.com", 3),
                ("conn", "api.anthropic.com", 2),
                ("conn", "statsig.anthropic.com", 1),
                ("log", "console.anthropic.com", 2),
                ("problem", "api.anthropic.com", 41)
            ]
        );
        assert_eq!(items[1].route, "AI → NY");
        assert_eq!(items[1].detail, "claude");
    }

    #[test]
    fn process_suggestions_put_networked_programs_first() {
        let programs = [
            crate::procs::Program { name: "telegram-desktop".into(), path: "/usr/bin/telegram-desktop".into(), pids: vec![1], uid: Some(1000), window: Some("Telegram".into()) },
            crate::procs::Program { name: "teleport-agent".into(), path: "/opt/t/teleport-agent".into(), pids: vec![2], uid: Some(1000), window: None },
            crate::procs::Program { name: "kitty".into(), path: "/usr/bin/kitty".into(), pids: vec![3], uid: Some(1000), window: Some("kitty".into()) },
        ];
        let conns = [c("t.me", "149.154.167.51", "telegram-desktop", "443", &["TELEGRAM"])];
        let logged = [crate::seen::Process { name: "telegram-cli".into(), last: 1, count: 3 }];
        let items = processes(&conns, &programs, &logged, false, "tele");
        let short: Vec<(&str, &str)> = items.iter().map(|i| (i.source, i.value.as_str())).collect();
        assert_eq!(short, [("conn", "telegram-desktop"), ("running", "teleport-agent"), ("log", "telegram-cli")]);
        assert!(items[0].detail.contains("«Telegram»"));
        let mut twice = programs.to_vec();
        twice.push(crate::procs::Program { name: "telegram-desktop".into(), path: "/opt/tg/telegram-desktop".into(), pids: vec![9], uid: Some(1000), window: None });
        let named = processes(&conns, &twice, &logged, false, "tele");
        assert_eq!(named.iter().filter(|i| i.value == "telegram-desktop").count(), 1, "one choice per name");
        assert!(named[0].detail.ends_with("· +1"));
        assert_eq!(processes(&conns, &twice, &logged, true, "telegram-desktop").len(), 2, "by path both stay");
        let all = processes(&conns, &programs, &logged, true, "");
        assert_eq!(all.iter().map(|i| i.value.as_str()).collect::<Vec<_>>(), ["/usr/bin/telegram-desktop", "/usr/bin/kitty"], "no query: networked and windowed only");
    }

    #[test]
    fn ports_and_known_lists() {
        let conns = [c("a.com", "1.1.1.1", "x", "443", &["A"]), c("b.com", "1.1.1.2", "x", "443", &["A"]), c("c.com", "1.1.1.3", "x", "8443", &["A"])];
        let items = ports(&conns, false, "44");
        assert_eq!(items[0].value, "443");
        assert_eq!(items[0].count, 2);
        assert!(items[0].detail.starts_with("https"));
        assert_eq!(known(ASNS, "telegram").len(), 4);
        assert_eq!(known(GEOIP, "ru")[0].value, "ru");
    }

    #[test]
    fn scopes_and_coverage_from_a_connection() {
        let conns = [c("api.anthropic.com", "160.79.104.10", "claude", "443", &["NY", "AI"]), c("statsig.anthropic.com", "1.1.1.1", "claude", "443", &["AI"]), c("x.org", "2.2.2.2", "firefox", "443", &["DIRECT"])];
        let logged = [seen("console.anthropic.com", "firefox")];
        let o = Origin { host: "api.anthropic.com".into(), ip: "160.79.104.10".into(), process: "claude".into(), ..Default::default() };
        let s = scopes(&o, &conns, &logged);
        let short: Vec<(MatchKind, &str, u64, u64)> = s.iter().map(|s| (s.kind, s.value.as_str(), s.coverage.connections, s.coverage.seen)).collect();
        assert_eq!(
            short,
            [
                (MatchKind::Domain, "api.anthropic.com", 1, 0),
                (MatchKind::DomainSuffix, "anthropic.com", 2, 1),
                (MatchKind::ProcessName, "claude", 2, 0),
                (MatchKind::IpCidr, "160.79.104.10/32", 1, 0)
            ]
        );
        assert_eq!(s[1].coverage.routes, [Route { rule: "DomainSuffix".into(), payload: "anthropic.com".into(), target: "AI".into(), count: 2 }]);

        let geo = UserRule { id: 0, kind: MatchKind::Geosite, value: "youtube".into(), target: "X".into(), enabled: true };
        assert!(coverage(&geo, &conns, &logged).unknown);
        let o = Origin { host: "149.154.167.51".into(), ..Default::default() };
        assert_eq!(scopes(&o, &[], &[]).iter().map(|s| s.kind).collect::<Vec<_>>(), [MatchKind::IpCidr], "an address has no domain scopes");
    }
}
