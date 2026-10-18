//! The user's own routing rules.
//!
//! They are rendered above every profile rule, so they win over the
//! subscription and survive its updates. A rule whose target group vanished
//! from the profile is *skipped*, never silently rerouted: traffic then falls
//! through to the profile's own rules and the UI flags the rule as orphaned.

use std::collections::HashSet;
use std::net::IpAddr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Built-in policies a rule may target besides groups and proxies.
pub const BUILTIN_TARGETS: &[&str] = &["DIRECT", "REJECT", "REJECT-DROP", "PASS"];

/// Nesting allowed inside `AND` / `OR` / `NOT`.
const MAX_LOGIC_DEPTH: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MatchKind {
    /// Exactly this host.
    Domain,
    /// The domain and every subdomain.
    DomainSuffix,
    /// Hosts containing a word.
    DomainKeyword,
    /// `*` / `?` pattern over the whole host.
    DomainWildcard,
    DomainRegex,
    /// A category of the core's GeoSite database.
    Geosite,
    /// One address or a network.
    IpCidr,
    /// The last bits of an address (`8.8.8.8/24`).
    IpSuffix,
    IpAsn,
    Geoip,
    SrcIpCidr,
    SrcPort,
    DstPort,
    /// `tcp` or `udp`.
    Network,
    ProcessName,
    ProcessPath,
    ProcessNameRegex,
    ProcessPathRegex,
    Uid,
    /// A rule provider of the profile.
    RuleSet,
    And,
    Or,
    Not,
}

impl MatchKind {
    pub const ALL: &[MatchKind] = &[
        MatchKind::Domain,
        MatchKind::DomainSuffix,
        MatchKind::DomainKeyword,
        MatchKind::DomainWildcard,
        MatchKind::DomainRegex,
        MatchKind::Geosite,
        MatchKind::IpCidr,
        MatchKind::IpSuffix,
        MatchKind::IpAsn,
        MatchKind::Geoip,
        MatchKind::SrcIpCidr,
        MatchKind::SrcPort,
        MatchKind::DstPort,
        MatchKind::Network,
        MatchKind::ProcessName,
        MatchKind::ProcessPath,
        MatchKind::ProcessNameRegex,
        MatchKind::ProcessPathRegex,
        MatchKind::Uid,
        MatchKind::RuleSet,
        MatchKind::And,
        MatchKind::Or,
        MatchKind::Not,
    ];

    /// Stable name, as in JSON and the database.
    pub fn as_str(self) -> &'static str {
        match self {
            MatchKind::Domain => "domain",
            MatchKind::DomainSuffix => "domain-suffix",
            MatchKind::DomainKeyword => "domain-keyword",
            MatchKind::DomainWildcard => "domain-wildcard",
            MatchKind::DomainRegex => "domain-regex",
            MatchKind::Geosite => "geosite",
            MatchKind::IpCidr => "ip-cidr",
            MatchKind::IpSuffix => "ip-suffix",
            MatchKind::IpAsn => "ip-asn",
            MatchKind::Geoip => "geoip",
            MatchKind::SrcIpCidr => "src-ip-cidr",
            MatchKind::SrcPort => "src-port",
            MatchKind::DstPort => "dst-port",
            MatchKind::Network => "network",
            MatchKind::ProcessName => "process-name",
            MatchKind::ProcessPath => "process-path",
            MatchKind::ProcessNameRegex => "process-name-regex",
            MatchKind::ProcessPathRegex => "process-path-regex",
            MatchKind::Uid => "uid",
            MatchKind::RuleSet => "rule-set",
            MatchKind::And => "and",
            MatchKind::Or => "or",
            MatchKind::Not => "not",
        }
    }

    pub fn is_logic(self) -> bool {
        matches!(self, MatchKind::And | MatchKind::Or | MatchKind::Not)
    }

    /// Rules on the destination address: rendered with `no-resolve` so that
    /// domain traffic is not forced through DNS just to test them.
    fn no_resolve(self) -> bool {
        matches!(self, MatchKind::IpCidr | MatchKind::IpSuffix | MatchKind::IpAsn | MatchKind::Geoip)
    }

    /// The mihomo rule keyword.
    pub fn keyword(self, value: &str) -> &'static str {
        match self {
            MatchKind::Domain => "DOMAIN",
            MatchKind::DomainSuffix => "DOMAIN-SUFFIX",
            MatchKind::DomainKeyword => "DOMAIN-KEYWORD",
            MatchKind::DomainWildcard => "DOMAIN-WILDCARD",
            MatchKind::DomainRegex => "DOMAIN-REGEX",
            MatchKind::Geosite => "GEOSITE",
            MatchKind::IpCidr if value.contains(':') => "IP-CIDR6",
            MatchKind::IpCidr => "IP-CIDR",
            MatchKind::IpSuffix => "IP-SUFFIX",
            MatchKind::IpAsn => "IP-ASN",
            MatchKind::Geoip => "GEOIP",
            MatchKind::SrcIpCidr => "SRC-IP-CIDR",
            MatchKind::SrcPort => "SRC-PORT",
            MatchKind::DstPort => "DST-PORT",
            MatchKind::Network => "NETWORK",
            MatchKind::ProcessName => "PROCESS-NAME",
            MatchKind::ProcessPath => "PROCESS-PATH",
            MatchKind::ProcessNameRegex => "PROCESS-NAME-REGEX",
            MatchKind::ProcessPathRegex => "PROCESS-PATH-REGEX",
            MatchKind::Uid => "UID",
            MatchKind::RuleSet => "RULE-SET",
            MatchKind::And => "AND",
            MatchKind::Or => "OR",
            MatchKind::Not => "NOT",
        }
    }

    /// The kind a mihomo rule keyword stands for.
    pub fn from_keyword(keyword: &str) -> Option<MatchKind> {
        let k = keyword.trim().to_ascii_uppercase();
        if k == "IP-CIDR6" {
            return Some(MatchKind::IpCidr);
        }
        MatchKind::ALL.iter().copied().find(|kind| kind.keyword("") == k)
    }

    /// The rule type name the core reports in `GET /rules`.
    fn core_type(self) -> &'static str {
        match self {
            MatchKind::Domain => "Domain",
            MatchKind::DomainSuffix => "DomainSuffix",
            MatchKind::DomainKeyword => "DomainKeyword",
            MatchKind::DomainWildcard => "DomainWildcard",
            MatchKind::DomainRegex => "DomainRegex",
            MatchKind::Geosite => "GeoSite",
            MatchKind::IpCidr => "IPCIDR",
            MatchKind::IpSuffix => "IPSuffix",
            MatchKind::IpAsn => "IPASN",
            MatchKind::Geoip => "GeoIP",
            MatchKind::SrcIpCidr => "SrcIPCIDR",
            MatchKind::SrcPort => "SrcPort",
            MatchKind::DstPort => "DstPort",
            MatchKind::Network => "Network",
            MatchKind::ProcessName => "Process",
            MatchKind::ProcessPath => "ProcessPath",
            MatchKind::ProcessNameRegex => "ProcessNameRegex",
            MatchKind::ProcessPathRegex => "ProcessPathRegex",
            MatchKind::Uid => "Uid",
            MatchKind::RuleSet => "RuleSet",
            MatchKind::And => "AND",
            MatchKind::Or => "OR",
            MatchKind::Not => "NOT",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserRule {
    pub id: i64,
    pub kind: MatchKind,
    /// Normalized: ASCII (punycode) lowercase domain, a CIDR, a port list,
    /// or for `AND` / `OR` / `NOT` the parenthesized conditions.
    pub value: String,
    pub target: String,
    pub enabled: bool,
}

/// What is known about one connection, for testing rules against it.
#[derive(Debug, Clone, Default)]
pub struct Conn<'a> {
    pub host: &'a str,
    pub ip: Option<IpAddr>,
    pub src_ip: Option<IpAddr>,
    pub dst_port: Option<u16>,
    pub src_port: Option<u16>,
    /// `tcp` / `udp`.
    pub network: &'a str,
    pub process: &'a str,
    pub process_path: &'a str,
    pub uid: Option<u32>,
}

/// One condition of a logic rule.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Part {
    pub kind: MatchKind,
    pub value: String,
}

impl UserRule {
    /// Whether the connection falls under this rule; `None` when it depends
    /// on data the daemon does not have (GeoSite, GeoIP, ASN, rule sets).
    pub fn matches(&self, c: &Conn) -> Option<bool> {
        matches_kind(self.kind, &self.value, c, 0)
    }

    /// Shorthand for host / address checks.
    pub fn matches_host(&self, host: &str, ip: Option<IpAddr>) -> bool {
        self.matches(&Conn { host, ip, ..Default::default() }) == Some(true)
    }

    /// The mihomo rule type name as reported by `GET /rules`.
    pub fn core_type(&self) -> &'static str {
        self.kind.core_type()
    }

    /// The mihomo rule line.
    pub fn line(&self) -> String {
        let kw = self.kind.keyword(&self.value);
        if self.kind.no_resolve() {
            format!("{kw},{},{},no-resolve", self.value, self.target)
        } else {
            format!("{kw},{},{}", self.value, self.target)
        }
    }
}

fn matches_kind(kind: MatchKind, value: &str, c: &Conn, depth: usize) -> Option<bool> {
    let host = c.host.to_ascii_lowercase();
    let host = host.trim_end_matches('.');
    Some(match kind {
        MatchKind::Domain => !host.is_empty() && host == value,
        MatchKind::DomainSuffix => host == value || host.strip_suffix(value).is_some_and(|rest| rest.ends_with('.')),
        MatchKind::DomainKeyword => !host.is_empty() && host.contains(value),
        MatchKind::DomainWildcard => !host.is_empty() && wildcard(value.as_bytes(), host.as_bytes()),
        MatchKind::DomainRegex => !host.is_empty() && regex::Regex::new(value).is_ok_and(|r| r.is_match(host)),
        MatchKind::IpCidr => c.ip.is_some_and(|ip| cidr_contains(value, ip)),
        MatchKind::SrcIpCidr => c.src_ip.is_some_and(|ip| cidr_contains(value, ip)),
        MatchKind::IpSuffix => c.ip.is_some_and(|ip| suffix_contains(value, ip)),
        MatchKind::DstPort => c.dst_port.is_some_and(|p| in_ranges(value, p as u32)),
        MatchKind::SrcPort => c.src_port.is_some_and(|p| in_ranges(value, p as u32)),
        MatchKind::Uid => c.uid.is_some_and(|u| in_ranges(value, u)),
        MatchKind::Network => c.network.eq_ignore_ascii_case(value),
        MatchKind::ProcessName => !c.process.is_empty() && c.process.eq_ignore_ascii_case(value),
        MatchKind::ProcessPath => !c.process_path.is_empty() && c.process_path == value,
        MatchKind::ProcessNameRegex => !c.process.is_empty() && regex::Regex::new(value).is_ok_and(|r| r.is_match(c.process)),
        MatchKind::ProcessPathRegex => !c.process_path.is_empty() && regex::Regex::new(value).is_ok_and(|r| r.is_match(c.process_path)),
        MatchKind::Geosite | MatchKind::Geoip | MatchKind::IpAsn | MatchKind::RuleSet => return None,
        MatchKind::And | MatchKind::Or | MatchKind::Not => {
            if depth > MAX_LOGIC_DEPTH {
                return None;
            }
            let parts = logic_parts(value).ok()?;
            let results: Vec<Option<bool>> = parts.iter().map(|p| matches_kind(p.kind, &p.value, c, depth + 1)).collect();
            return match kind {
                MatchKind::And if results.contains(&Some(false)) => Some(false),
                MatchKind::And if results.iter().all(|r| *r == Some(true)) => Some(true),
                MatchKind::Or if results.contains(&Some(true)) => Some(true),
                MatchKind::Or if results.iter().all(|r| *r == Some(false)) => Some(false),
                MatchKind::Not => results.first().copied().flatten().map(|r| !r),
                _ => None,
            };
        }
    })
}

/// `*` matches any run of characters, `?` exactly one.
fn wildcard(pattern: &[u8], text: &[u8]) -> bool {
    let (mut p, mut t) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            mark = t;
            p += 1;
        } else if let Some(s) = star {
            p = s + 1;
            mark += 1;
            t = mark;
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&b| b == b'*')
}

/// `80/443/1000-2000`.
fn in_ranges(list: &str, n: u32) -> bool {
    list.split('/').any(|item| match item.split_once('-') {
        Some((a, b)) => a.parse::<u32>().is_ok_and(|a| a <= n) && b.parse::<u32>().is_ok_and(|b| n <= b),
        None => item.parse::<u32>().is_ok_and(|v| v == n),
    })
}

/// Validate and normalize user input for `kind`.
///
/// Accepts what people paste: URLs, `*.example.com`, trailing dots,
/// Cyrillic domains, bare IPs, `AS13335`, `geosite:youtube`.
pub fn normalize(kind: MatchKind, input: &str) -> Result<String> {
    normalize_at(kind, input, 0)
}

fn normalize_at(kind: MatchKind, input: &str, depth: usize) -> Result<String> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err(invalid("empty value"));
    }
    if !kind.is_logic() && (raw.contains(',') || raw.contains('\n')) {
        return Err(invalid("a value cannot contain commas"));
    }
    match kind {
        MatchKind::IpCidr | MatchKind::IpSuffix | MatchKind::SrcIpCidr => normalize_cidr(raw),
        MatchKind::Domain | MatchKind::DomainSuffix => normalize_domain(raw),
        MatchKind::DomainKeyword => {
            let v = raw.to_lowercase();
            if !v.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)) {
                return Err(invalid(format!("invalid keyword {raw:?}: use letters, digits, '-', '_' and '.'")));
            }
            Ok(v)
        }
        MatchKind::DomainWildcard => {
            let v = strip_url(raw).trim_end_matches('.').to_lowercase();
            if !v.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.*?".contains(&b)) || v.bytes().all(|b| b == b'*' || b == b'?' || b == b'.') {
                return Err(invalid(format!("invalid pattern {raw:?}")));
            }
            Ok(v)
        }
        MatchKind::DomainRegex | MatchKind::ProcessNameRegex | MatchKind::ProcessPathRegex => {
            regex::Regex::new(raw).map_err(|e| invalid(format!("invalid regular expression: {}", e.to_string().lines().last().unwrap_or_default().trim())))?;
            Ok(raw.to_owned())
        }
        MatchKind::Geosite | MatchKind::Geoip => {
            let prefix = if kind == MatchKind::Geosite { "geosite:" } else { "geoip:" };
            let v = raw.to_lowercase();
            let v = v.strip_prefix(prefix).unwrap_or(&v).trim().to_owned();
            if v.is_empty() || !v.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_!@:.".contains(&b)) {
                return Err(invalid(format!("invalid category {raw:?}")));
            }
            Ok(v)
        }
        MatchKind::IpAsn => {
            let v = raw.trim_start_matches(['A', 'a']).trim_start_matches(['S', 's']);
            v.parse::<u32>().map(|n| n.to_string()).map_err(|_| invalid(format!("invalid AS number {raw:?}")))
        }
        MatchKind::DstPort | MatchKind::SrcPort => normalize_ranges(raw, 65535, "port"),
        MatchKind::Uid => normalize_ranges(raw, u32::MAX, "uid"),
        MatchKind::Network => match raw.to_ascii_lowercase().as_str() {
            v @ ("tcp" | "udp") => Ok(v.to_owned()),
            _ => Err(invalid("network is tcp or udp")),
        },
        MatchKind::ProcessName => {
            if raw.contains('/') {
                return Err(invalid("a process name has no '/'; match the full path instead"));
            }
            Ok(raw.to_owned())
        }
        MatchKind::ProcessPath => {
            if !raw.starts_with('/') {
                return Err(invalid("a process path starts with '/'"));
            }
            Ok(raw.to_owned())
        }
        MatchKind::RuleSet => {
            if raw.chars().any(|c| c.is_whitespace() || c == '(' || c == ')') {
                return Err(invalid(format!("invalid rule provider name {raw:?}")));
            }
            Ok(raw.to_owned())
        }
        MatchKind::And | MatchKind::Or | MatchKind::Not => {
            if depth >= MAX_LOGIC_DEPTH {
                return Err(invalid("conditions are nested too deep"));
            }
            let parts = split_logic(raw)?
                .into_iter()
                .map(|(kw, value)| {
                    let sub = MatchKind::from_keyword(&kw).ok_or_else(|| invalid(format!("unknown condition {kw:?}")))?;
                    let value = normalize_at(sub, &value, depth + 1)?;
                    if !sub.is_logic() && (value.contains('(') || value.contains(')')) {
                        return Err(invalid("a condition value cannot contain parentheses"));
                    }
                    Ok(format!("({},{value})", sub.keyword(&value)))
                })
                .collect::<Result<Vec<_>>>()?;
            match (kind, parts.len()) {
                (MatchKind::Not, 1) | (MatchKind::And | MatchKind::Or, 2..) => Ok(format!("({})", parts.join(","))),
                (MatchKind::Not, _) => Err(invalid("NOT takes exactly one condition")),
                _ => Err(invalid("give at least two conditions")),
            }
        }
    }
}

/// Conditions of a normalized logic value, for editing.
pub fn logic_parts(value: &str) -> Result<Vec<Part>> {
    split_logic(value)?
        .into_iter()
        .map(|(kw, value)| Ok(Part { kind: MatchKind::from_keyword(&kw).ok_or_else(|| invalid(format!("unknown condition {kw:?}")))?, value }))
        .collect()
}

/// `((A,x),(B,y))` or `(A,x),(B,y)` into `[(A, x), (B, y)]`; a nested
/// logic condition keeps its parenthesized value.
fn split_logic(raw: &str) -> Result<Vec<(String, String)>> {
    let mut s = raw.trim();
    // Drop one wrapping pair when it encloses everything.
    if s.starts_with("((") && closing(s, 0) == Some(s.len() - 1) {
        s = &s[1..s.len() - 1];
    }
    let mut out = Vec::new();
    let mut rest = s.trim();
    while !rest.is_empty() {
        if !rest.starts_with('(') {
            return Err(invalid("write conditions as (TYPE,value)"));
        }
        let end = closing(rest, 0).ok_or_else(|| invalid("unbalanced parentheses"))?;
        let inner = &rest[1..end];
        let (kw, value) = inner.split_once(',').ok_or_else(|| invalid(format!("condition {inner:?} has no value")))?;
        out.push((kw.trim().to_owned(), value.trim().to_owned()));
        rest = rest[end + 1..].trim_start();
        rest = match rest.strip_prefix(',') {
            Some(r) => r.trim_start(),
            None if rest.is_empty() => rest,
            None => return Err(invalid("separate conditions with commas")),
        };
    }
    if out.is_empty() {
        return Err(invalid("no conditions"));
    }
    Ok(out)
}

/// Index of the parenthesis closing the one at `open`.
fn closing(s: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, b) in s.bytes().enumerate().skip(open) {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn normalize_ranges(raw: &str, max: u32, what: &str) -> Result<String> {
    let bad = || invalid(format!("invalid {what} list {raw:?}: use 443, 1000-2000 or 80/443"));
    let items = raw
        .split(['/', ' '])
        .filter(|s| !s.is_empty())
        .map(|item| {
            let num = |s: &str| s.trim().parse::<u32>().ok().filter(|n| *n <= max).ok_or_else(bad);
            match item.split_once('-') {
                Some((a, b)) => {
                    let (a, b) = (num(a)?, num(b)?);
                    if a > b {
                        return Err(bad());
                    }
                    Ok(format!("{a}-{b}"))
                }
                None => Ok(num(item)?.to_string()),
            }
        })
        .collect::<Result<Vec<_>>>()?;
    if items.is_empty() {
        return Err(bad());
    }
    Ok(items.join("/"))
}

fn normalize_domain(raw: &str) -> Result<String> {
    let host = strip_url(raw);
    let host = host.strip_prefix("*.").or_else(|| host.strip_prefix('.')).unwrap_or(host);
    let host = host.trim_end_matches('.');
    if host.is_empty() {
        return Err(invalid("empty domain"));
    }
    if host.parse::<IpAddr>().is_ok() {
        return Err(invalid(format!("{host} is an IP address, not a domain")));
    }
    let ascii = idna::domain_to_ascii(host).map_err(|_| invalid(format!("invalid domain {host:?}")))?;
    let valid = ascii.split('.').all(|label| {
        !label.is_empty() && label.len() <= 63 && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    });
    if !valid || ascii.len() > 253 {
        return Err(invalid(format!("invalid domain {host:?}")));
    }
    Ok(ascii)
}

fn invalid(msg: impl Into<String>) -> Error {
    Error::Invalid(msg.into())
}

fn strip_url(s: &str) -> &str {
    let s = s.split_once("://").map_or(s, |(_, rest)| rest);
    let s = s.split(['/', '?', '#']).next().unwrap_or(s);
    let s = s.rsplit_once('@').map_or(s, |(_, host)| host);
    // Drop a port, but not the colons of a bare IPv6 address.
    match s.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') && port.bytes().all(|b| b.is_ascii_digit()) => host,
        _ => s,
    }
}

fn normalize_cidr(s: &str) -> Result<String> {
    let s = s.trim_start_matches('[').trim_end_matches(']');
    let (addr, prefix) = match s.split_once('/') {
        Some((a, p)) => (a, Some(p)),
        None => (s, None),
    };
    let ip: IpAddr = addr.parse().map_err(|_| invalid(format!("invalid IP address {addr:?}")))?;
    let max = if ip.is_ipv4() { 32 } else { 128 };
    let prefix = match prefix {
        Some(p) => p.parse::<u8>().ok().filter(|p| *p <= max).ok_or_else(|| invalid(format!("invalid prefix /{p}")))?,
        None => max,
    };
    Ok(format!("{ip}/{prefix}"))
}

/// Multi-label public suffixes common enough to matter; anything else is
/// treated as a single-label TLD.
const SECOND_LEVEL: &[&str] = &[
    "co.uk", "org.uk", "ac.uk", "gov.uk", "me.uk", "com.au", "net.au", "org.au", "co.jp", "ne.jp", "or.jp", "com.br", "com.cn", "net.cn", "org.cn",
    "com.tr", "com.ua", "org.ua", "net.ua", "pp.ua", "in.ua", "kiev.ua", "com.ru", "net.ru", "org.ru", "pp.ru", "msk.ru", "spb.ru", "com.kz", "org.kz",
    "co.kr", "co.in", "co.id", "co.nz", "co.za", "com.mx", "com.ar", "com.sg", "com.hk", "com.tw", "com.by", "github.io", "gitlab.io", "pages.dev",
    "workers.dev", "vercel.app", "netlify.app", "herokuapp.com", "appspot.com", "cloudfront.net", "azurewebsites.net", "blogspot.com",
];

/// The registrable domain (eTLD+1) of a host: `rr3.googlevideo.com` →
/// `googlevideo.com`, `news.bbc.co.uk` → `bbc.co.uk`.
pub fn registrable(host: &str) -> String {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() <= 2 || host.parse::<IpAddr>().is_ok() {
        return host;
    }
    let last2 = labels[labels.len() - 2..].join(".");
    let keep = if SECOND_LEVEL.contains(&last2.as_str()) { 3 } else { 2 };
    labels[labels.len().saturating_sub(keep)..].join(".")
}

/// Order rules so a narrower one is never shadowed by a broader one
/// (mihomo stops at the first match): `AND` conditions, exact domains,
/// networks from the longest prefix, suffixes from the most labels, then
/// patterns, apps, ports and whole databases; `OR` / `NOT` last. Ties keep
/// id order.
pub fn sort_by_specificity(rules: &mut [UserRule]) {
    rules.sort_by_key(|r| {
        let prefix = || 255 - r.value.rsplit_once('/').and_then(|(_, p)| p.parse::<u32>().ok()).unwrap_or(0);
        let rank = match r.kind {
            MatchKind::And => (0, 0),
            MatchKind::Domain => (1, 0),
            MatchKind::IpCidr | MatchKind::SrcIpCidr => (2, prefix()),
            MatchKind::DomainSuffix => (3, 255 - r.value.split('.').count() as u32),
            MatchKind::DomainKeyword | MatchKind::DomainWildcard | MatchKind::DomainRegex => (4, 0),
            MatchKind::ProcessPath | MatchKind::ProcessName | MatchKind::ProcessPathRegex | MatchKind::ProcessNameRegex | MatchKind::Uid => (5, 0),
            MatchKind::DstPort | MatchKind::SrcPort | MatchKind::Network => (6, 0),
            MatchKind::IpSuffix | MatchKind::IpAsn | MatchKind::Geosite | MatchKind::Geoip | MatchKind::RuleSet => (7, 0),
            MatchKind::Or | MatchKind::Not => (8, 0),
        };
        (rank, r.id)
    });
}

fn cidr_contains(cidr: &str, ip: IpAddr) -> bool {
    let Some((net, prefix)) = cidr.split_once('/') else { return false };
    let (Ok(net), Ok(prefix)) = (net.parse::<IpAddr>(), prefix.parse::<u32>()) else { return false };
    match (net, ip) {
        (IpAddr::V4(n), IpAddr::V4(a)) => {
            let mask = u32::MAX.checked_shl(32 - prefix.min(32)).unwrap_or(0);
            u32::from(n) & mask == u32::from(a) & mask
        }
        (IpAddr::V6(n), IpAddr::V6(a)) => {
            let mask = u128::MAX.checked_shl(128 - prefix.min(128)).unwrap_or(0);
            u128::from(n) & mask == u128::from(a) & mask
        }
        _ => false,
    }
}

/// `IP-SUFFIX`: the last `bits` bits of the address are those of `net`.
fn suffix_contains(suffix: &str, ip: IpAddr) -> bool {
    let Some((net, bits)) = suffix.split_once('/') else { return false };
    let (Ok(net), Ok(bits)) = (net.parse::<IpAddr>(), bits.parse::<u32>()) else { return false };
    match (net, ip) {
        (IpAddr::V4(n), IpAddr::V4(a)) => {
            let mask = if bits >= 32 { u32::MAX } else { (1u32 << bits) - 1 };
            u32::from(n) & mask == u32::from(a) & mask
        }
        (IpAddr::V6(n), IpAddr::V6(a)) => {
            let mask = if bits >= 128 { u128::MAX } else { (1u128 << bits) - 1 };
            u128::from(n) & mask == u128::from(a) & mask
        }
        _ => false,
    }
}

/// Rule lines in order, and the ids of rules left out.
#[derive(Debug, Default, PartialEq)]
pub struct Rendered {
    pub lines: Vec<String>,
    /// Enabled rules whose target does not exist in the current profile.
    pub orphans: Vec<i64>,
}

pub fn render(rules: &[UserRule], targets: &HashSet<&str>) -> Rendered {
    let mut out = Rendered::default();
    for r in rules.iter().filter(|r| r.enabled) {
        if targets.contains(r.target.as_str()) || BUILTIN_TARGETS.contains(&r.target.as_str()) {
            out.lines.push(r.line());
        } else {
            out.orphans.push(r.id);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_pasted_domains() {
        use MatchKind::*;
        assert_eq!(normalize(Domain, "https://API.Anthropic.com:443/v1?x").unwrap(), "api.anthropic.com");
        assert_eq!(normalize(DomainSuffix, "*.example.org.").unwrap(), "example.org");
        assert_eq!(normalize(DomainSuffix, "госуслуги.рф").unwrap(), "xn--c1aapkosapc.xn--p1ai");
        assert_eq!(normalize(Domain, "user@host.io").unwrap(), "host.io");
        assert!(normalize(Domain, "1.2.3.4").is_err());
        assert!(normalize(Domain, "bad domain").is_err());
        assert!(normalize(Domain, "").is_err());
    }

    #[test]
    fn normalizes_addresses() {
        use MatchKind::IpCidr;
        assert_eq!(normalize(IpCidr, "149.154.167.51").unwrap(), "149.154.167.51/32");
        assert_eq!(normalize(IpCidr, "10.0.0.0/8").unwrap(), "10.0.0.0/8");
        assert_eq!(normalize(IpCidr, "[2001:db8::1]").unwrap(), "2001:db8::1/128");
        assert!(normalize(IpCidr, "1.2.3.4/33").is_err());
        assert!(normalize(IpCidr, "example.com").is_err());
    }

    fn rule(id: i64, kind: MatchKind, value: &str) -> UserRule {
        UserRule { id, kind, value: value.into(), target: "AI".into(), enabled: true }
    }

    #[test]
    fn matching_hosts_and_addresses() {
        let suffix = rule(1, MatchKind::DomainSuffix, "x.com");
        assert!(suffix.matches_host("x.com", None) && suffix.matches_host("API.x.com", None));
        assert!(!suffix.matches_host("notx.com", None));
        let exact = rule(2, MatchKind::Domain, "api.x.com");
        assert!(exact.matches_host("api.x.com", None) && !exact.matches_host("x.com", None));
        let net = rule(3, MatchKind::IpCidr, "91.108.4.0/22");
        assert!(net.matches_host("", Some("91.108.7.255".parse().unwrap())));
        assert!(!net.matches_host("", Some("91.108.8.0".parse().unwrap())));
        assert!(rule(4, MatchKind::IpCidr, "0.0.0.0/0").matches_host("", Some("8.8.8.8".parse().unwrap())));
        let v6 = rule(5, MatchKind::IpCidr, "2001:db8::/32");
        assert!(v6.matches_host("", Some("2001:db8:1::1".parse().unwrap())));
        assert!(!v6.matches_host("", Some("1.2.3.4".parse().unwrap())));
    }

    #[test]
    fn specific_rules_come_first() {
        let mut rules = vec![
            rule(1, MatchKind::DomainSuffix, "x.com"),
            rule(2, MatchKind::DomainSuffix, "api.x.com"),
            rule(3, MatchKind::IpCidr, "10.0.0.0/8"),
            rule(4, MatchKind::Domain, "a.x.com"),
            rule(5, MatchKind::IpCidr, "10.1.2.3/32"),
        ];
        sort_by_specificity(&mut rules);
        assert_eq!(rules.iter().map(|r| r.id).collect::<Vec<_>>(), [4, 5, 3, 2, 1]);
    }

    #[test]
    fn renders_lines_and_skips_orphans() {
        let r = |id, kind, value: &str, target: &str, enabled| UserRule { id, kind, value: value.into(), target: target.into(), enabled };
        let rules = [
            r(1, MatchKind::DomainSuffix, "anthropic.com", "AI", true),
            r(2, MatchKind::IpCidr, "2001:db8::/32", "DIRECT", true),
            r(3, MatchKind::Domain, "tiktok.com", "TikTok", true),
            r(4, MatchKind::Domain, "off.example", "AI", false),
            r(5, MatchKind::IpCidr, "1.1.1.1/32", "AI", true),
        ];
        let targets: HashSet<&str> = ["AI", "PROXY"].into();
        assert_eq!(
            render(&rules, &targets),
            Rendered {
                lines: vec![
                    "DOMAIN-SUFFIX,anthropic.com,AI".into(),
                    "IP-CIDR6,2001:db8::/32,DIRECT,no-resolve".into(),
                    "IP-CIDR,1.1.1.1/32,AI,no-resolve".into(),
                ],
                orphans: vec![3],
            }
        );
    }

    #[test]
    fn normalizes_every_kind() {
        use MatchKind::*;
        let n = |k, v| normalize(k, v).unwrap();
        assert_eq!(n(DomainKeyword, " Google "), "google");
        assert_eq!(n(DomainWildcard, "*.CDN.*.com"), "*.cdn.*.com");
        assert_eq!(n(DomainRegex, r"^rr\d+---.*"), r"^rr\d+---.*");
        assert!(normalize(DomainRegex, "(").is_err());
        assert_eq!(n(Geosite, "geosite:YouTube"), "youtube");
        assert_eq!(n(Geoip, "RU"), "ru");
        assert_eq!(n(IpAsn, "AS13335"), "13335");
        assert_eq!(n(IpSuffix, "8.8.8.8/24"), "8.8.8.8/24");
        assert_eq!(n(DstPort, "443"), "443");
        assert_eq!(n(DstPort, "80/443 6881-6889"), "80/443/6881-6889");
        assert!(normalize(DstPort, "70000").is_err());
        assert!(normalize(DstPort, "9-1").is_err());
        assert_eq!(n(Network, "UDP"), "udp");
        assert!(normalize(Network, "icmp").is_err());
        assert_eq!(n(ProcessName, "telegram-desktop"), "telegram-desktop");
        assert!(normalize(ProcessName, "/usr/bin/x").is_err());
        assert_eq!(n(ProcessPath, "/usr/bin/telegram-desktop"), "/usr/bin/telegram-desktop");
        assert!(normalize(ProcessPath, "telegram").is_err());
        assert_eq!(n(Uid, "1000"), "1000");
        assert!(normalize(RuleSet, "my list").is_err());
        assert!(normalize(DomainKeyword, "a,b").is_err(), "commas would split the rule line");
    }

    #[test]
    fn logic_rules_normalize_nest_and_round_trip() {
        use MatchKind::*;
        let v = normalize(And, "(process-name,firefox), (domain-suffix,*.YouTube.com),(NETWORK,UDP)").unwrap();
        assert_eq!(v, "((PROCESS-NAME,firefox),(DOMAIN-SUFFIX,youtube.com),(NETWORK,udp))");
        assert_eq!(normalize(And, &v).unwrap(), v, "idempotent");
        let parts = logic_parts(&v).unwrap();
        assert_eq!(parts[1], Part { kind: DomainSuffix, value: "youtube.com".into() });
        let nested = normalize(And, "((OR,((DOMAIN,a.com),(DOMAIN,b.com))),(NETWORK,tcp))").unwrap();
        assert_eq!(nested, "((OR,((DOMAIN,a.com),(DOMAIN,b.com))),(NETWORK,tcp))");
        assert_eq!(normalize(Not, "((IP-CIDR,10.0.0.0/8))").unwrap(), "((IP-CIDR,10.0.0.0/8))");
        assert!(normalize(Not, "(DOMAIN,a.com),(DOMAIN,b.com)").is_err());
        assert!(normalize(And, "(DOMAIN,a.com)").is_err());
        assert!(normalize(And, "(BOGUS,x),(DOMAIN,a.com)").is_err());
        assert!(normalize(And, "(DOMAIN,a.com),(DOMAIN-REGEX,^(a|b)$)").is_err());
        let rule = UserRule { id: 1, kind: And, value: v, target: "Stream".into(), enabled: true };
        assert_eq!(rule.line(), "AND,((PROCESS-NAME,firefox),(DOMAIN-SUFFIX,youtube.com),(NETWORK,udp)),Stream");
        assert_eq!(MatchKind::from_keyword("ip-cidr6"), Some(IpCidr));
        assert_eq!(MatchKind::from_keyword("PROCESS-NAME"), Some(ProcessName));
    }

    #[test]
    fn matching_connections() {
        use MatchKind::*;
        let r = |kind, value: &str| UserRule { id: 1, kind, value: value.into(), target: "X".into(), enabled: true };
        let c = Conn {
            host: "rr3---sn-x.googlevideo.com",
            ip: Some("172.217.1.10".parse().unwrap()),
            dst_port: Some(443),
            network: "udp",
            process: "firefox",
            process_path: "/usr/lib/firefox/firefox",
            uid: Some(1000),
            ..Default::default()
        };
        assert_eq!(r(DomainKeyword, "googlevideo").matches(&c), Some(true));
        assert_eq!(r(DomainWildcard, "rr?---*.googlevideo.com").matches(&c), Some(true));
        assert_eq!(r(DomainWildcard, "*.youtube.com").matches(&c), Some(false));
        assert_eq!(r(DomainRegex, r"^rr\d+---").matches(&c), Some(true));
        assert_eq!(r(DstPort, "80/400-500").matches(&c), Some(true));
        assert_eq!(r(Network, "tcp").matches(&c), Some(false));
        assert_eq!(r(ProcessName, "Firefox").matches(&c), Some(true));
        assert_eq!(r(ProcessPathRegex, "^/usr/lib/").matches(&c), Some(true));
        assert_eq!(r(Uid, "1000").matches(&c), Some(true));
        assert_eq!(r(IpSuffix, "0.0.1.10/16").matches(&c), Some(true));
        assert_eq!(r(Geosite, "youtube").matches(&c), None, "needs the GeoSite database");
        assert_eq!(r(And, "((PROCESS-NAME,firefox),(NETWORK,udp))").matches(&c), Some(true));
        assert_eq!(r(And, "((PROCESS-NAME,firefox),(NETWORK,tcp))").matches(&c), Some(false));
        assert_eq!(r(And, "((PROCESS-NAME,firefox),(GEOSITE,youtube))").matches(&c), None);
        assert_eq!(r(Or, "((NETWORK,tcp),(GEOSITE,youtube))").matches(&c), None);
        assert_eq!(r(Or, "((NETWORK,udp),(GEOSITE,youtube))").matches(&c), Some(true));
        assert_eq!(r(Not, "((NETWORK,tcp))").matches(&c), Some(true));
    }

    #[test]
    fn registrable_domains() {
        assert_eq!(registrable("rr3---sn-4g5e6nsz.googlevideo.com"), "googlevideo.com");
        assert_eq!(registrable("news.bbc.co.uk"), "bbc.co.uk");
        assert_eq!(registrable("x.com"), "x.com");
        assert_eq!(registrable("API.Anthropic.com."), "anthropic.com");
        assert_eq!(registrable("149.154.167.51"), "149.154.167.51");
    }
}
