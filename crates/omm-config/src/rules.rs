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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MatchKind {
    /// Exactly this host.
    Domain,
    /// The domain and every subdomain.
    DomainSuffix,
    /// One address or a network.
    IpCidr,
}

impl MatchKind {
    fn keyword(self, value: &str) -> &'static str {
        match self {
            MatchKind::Domain => "DOMAIN",
            MatchKind::DomainSuffix => "DOMAIN-SUFFIX",
            MatchKind::IpCidr if value.contains(':') => "IP-CIDR6",
            MatchKind::IpCidr => "IP-CIDR",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserRule {
    pub id: i64,
    pub kind: MatchKind,
    /// Normalized: ASCII (punycode) lowercase domain, or a CIDR.
    pub value: String,
    pub target: String,
    pub enabled: bool,
}

impl UserRule {
    /// Whether a connection to `host` / `ip` falls under this rule.
    pub fn matches(&self, host: &str, ip: Option<IpAddr>) -> bool {
        match self.kind {
            MatchKind::Domain => host.eq_ignore_ascii_case(&self.value),
            MatchKind::DomainSuffix => {
                let host = host.to_ascii_lowercase();
                host == self.value || host.strip_suffix(&self.value).is_some_and(|rest| rest.ends_with('.'))
            }
            MatchKind::IpCidr => ip.is_some_and(|ip| cidr_contains(&self.value, ip)),
        }
    }

    /// The mihomo rule type name as reported by `GET /rules`.
    pub fn core_type(&self) -> &'static str {
        match self.kind {
            MatchKind::Domain => "Domain",
            MatchKind::DomainSuffix => "DomainSuffix",
            MatchKind::IpCidr if self.value.contains(':') => "IPCIDR6",
            MatchKind::IpCidr => "IPCIDR",
        }
    }

    /// The mihomo rule line. IP rules get `no-resolve` so that domain
    /// traffic is not forced through DNS just to test them.
    pub fn line(&self) -> String {
        let kw = self.kind.keyword(&self.value);
        match self.kind {
            MatchKind::IpCidr => format!("{kw},{},{},no-resolve", self.value, self.target),
            _ => format!("{kw},{},{}", self.value, self.target),
        }
    }
}

/// Validate and normalize user input for `kind`.
///
/// Accepts what people paste: URLs, `*.example.com`, trailing dots,
/// Cyrillic domains, bare IPs.
pub fn normalize(kind: MatchKind, input: &str) -> Result<String> {
    let raw = input.trim();
    match kind {
        MatchKind::IpCidr => normalize_cidr(raw),
        MatchKind::Domain | MatchKind::DomainSuffix => {
            let host = strip_url(raw);
            let host = host.strip_prefix("*.").or_else(|| host.strip_prefix('.')).unwrap_or(host);
            let host = host.trim_end_matches('.');
            if host.is_empty() {
                return Err(Error::Invalid("empty domain".into()));
            }
            if host.parse::<IpAddr>().is_ok() {
                return Err(Error::Invalid(format!("{host} is an IP address, not a domain")));
            }
            let ascii = idna::domain_to_ascii(host).map_err(|_| Error::Invalid(format!("invalid domain {host:?}")))?;
            let valid = ascii.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            });
            if !valid || ascii.len() > 253 {
                return Err(Error::Invalid(format!("invalid domain {host:?}")));
            }
            Ok(ascii)
        }
    }
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
    let ip: IpAddr = addr.parse().map_err(|_| Error::Invalid(format!("invalid IP address {addr:?}")))?;
    let max = if ip.is_ipv4() { 32 } else { 128 };
    let prefix = match prefix {
        Some(p) => p.parse::<u8>().ok().filter(|p| *p <= max).ok_or_else(|| Error::Invalid(format!("invalid prefix /{p}")))?,
        None => max,
    };
    Ok(format!("{ip}/{prefix}"))
}

/// Order rules so a narrower one is never shadowed by a broader one
/// (mihomo stops at the first match): exact domains, then networks from
/// the longest prefix, then suffixes from the most labels. Ties keep id order.
pub fn sort_by_specificity(rules: &mut [UserRule]) {
    rules.sort_by_key(|r| {
        let rank = match r.kind {
            MatchKind::Domain => (0, 0),
            MatchKind::IpCidr => (1, 255 - r.value.rsplit_once('/').and_then(|(_, p)| p.parse::<u32>().ok()).unwrap_or(0)),
            MatchKind::DomainSuffix => (2, 255 - r.value.split('.').count() as u32),
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
        assert!(suffix.matches("x.com", None) && suffix.matches("API.x.com", None));
        assert!(!suffix.matches("notx.com", None));
        let exact = rule(2, MatchKind::Domain, "api.x.com");
        assert!(exact.matches("api.x.com", None) && !exact.matches("x.com", None));
        let net = rule(3, MatchKind::IpCidr, "91.108.4.0/22");
        assert!(net.matches("", Some("91.108.7.255".parse().unwrap())));
        assert!(!net.matches("", Some("91.108.8.0".parse().unwrap())));
        assert!(rule(4, MatchKind::IpCidr, "0.0.0.0/0").matches("", Some("8.8.8.8".parse().unwrap())));
        let v6 = rule(5, MatchKind::IpCidr, "2001:db8::/32");
        assert!(v6.matches("", Some("2001:db8:1::1".parse().unwrap())));
        assert!(!v6.matches("", Some("1.2.3.4".parse().unwrap())));
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
}
