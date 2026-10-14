//! Parsing mihomo's tunnel log lines (`tunnel/tunnel.go`).
//!
//! ```text
//! [TCP] dial DIRECT (match Match/) 198.18.0.1:38164(Telegram, uid=1000) --> 142.252.58.41:2137 error: dial tcp ...: i/o timeout
//! [TCP] dial PROXY 198.18.0.1:5000 --> example.com:443 error: ...                      (global/direct mode)
//! [TCP] 198.18.0.1:48082(chromium, uid=1000) --> www.google.com:443 match DomainSuffix(google.com) using WARP[Стандартный 2]
//! [UDP] 198.18.0.1:5353 --> 1.1.1.1:53 using DIRECT
//! ```
//!
//! Group and process names may contain spaces and commas, so fields are
//! located by the ` --> ` arrow and the parentheses, never by splitting on
//! whitespace.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Proto {
    Tcp,
    Udp,
}

/// Rule that sent the connection where it went.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Route {
    /// mihomo rule type (`DomainSuffix`, `Match`, `RuleSet`...); empty in
    /// global/direct mode.
    pub rule: String,
    pub payload: String,
    /// Outbound named by the rule: a group, a proxy or `DIRECT`.
    pub target: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Endpoint {
    /// Host name, or IP when the connection had none.
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    pub process: Option<String>,
    /// Connections made by mihomo itself (DNS, health checks).
    pub inner: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DialError {
    pub proto: Proto,
    pub route: Route,
    pub source: Source,
    pub dest: Endpoint,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Connected {
    pub proto: Proto,
    pub source: Source,
    pub dest: Endpoint,
    /// `Group[Node]`, `DIRECT`, ...
    pub chain: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    DialError(DialError),
    Connected(Connected),
}

/// Parse one `/logs` payload; anything else (DNS chatter, config messages)
/// yields `None`.
pub fn parse(payload: &str) -> Option<Line> {
    let (proto, rest) = proto_prefix(payload)?;
    let (left, right) = rest.split_once(" --> ")?;
    match left.strip_prefix("dial ") {
        Some(dial) => parse_error(proto, dial, right).map(Line::DialError),
        None => parse_connected(proto, left, right).map(Line::Connected),
    }
}

fn proto_prefix(s: &str) -> Option<(Proto, &str)> {
    if let Some(r) = s.strip_prefix("[TCP] ") {
        Some((Proto::Tcp, r))
    } else {
        s.strip_prefix("[UDP] ").map(|r| (Proto::Udp, r))
    }
}

/// `{target} [(match {Type}/{payload})] {source}` + `{dest} error: {err}`
fn parse_error(proto: Proto, left: &str, right: &str) -> Option<DialError> {
    let (head, source) = split_source(left)?;
    if head.is_empty() {
        return None;
    }
    let route = match head.rsplit_once(" (match ") {
        Some((target, rule)) => {
            let rule = rule.strip_suffix(')')?;
            let (kind, payload) = rule.split_once('/').unwrap_or((rule, ""));
            Route { rule: kind.into(), payload: payload.into(), target: target.into() }
        }
        None => Route { target: head.into(), ..Default::default() },
    };
    let (dest, error) = right.split_once(" error: ")?;
    Some(DialError { proto, route, source, dest: endpoint(dest)?, error: error.trim().into() })
}

/// `{source}` + `{dest} [match ...] using {chain}`
fn parse_connected(proto: Proto, left: &str, right: &str) -> Option<Connected> {
    let (_, source) = split_source(left)?;
    let (dest, tail) = right.split_once(' ')?;
    let chain = tail.rsplit_once("using ").map(|(_, c)| c.trim())?;
    Some(Connected { proto, source, dest: endpoint(dest)?, chain: chain.into() })
}

/// Split `"[{head} ]{addr}[({process}[, uid=N])]"` into head (possibly
/// empty) and source.
fn split_source(s: &str) -> Option<(&str, Source)> {
    let s = s.trim_end();
    if s == "mihomo" {
        return Some(("", Source { process: None, inner: true }));
    }
    if let Some(head) = s.strip_suffix(" mihomo") {
        return Some((head, Source { process: None, inner: true }));
    }
    let (rest, detail) = match s.strip_suffix(')') {
        Some(inner) => {
            let open = inner.rfind('(')?;
            (&inner[..open], Some(&inner[open + 1..]))
        }
        None => (s, None),
    };
    // `rest` ends with the source address, which never contains spaces.
    let head = rest.rsplit_once(' ').map_or("", |(h, _)| h);
    let process = detail
        .map(|d| match d.rsplit_once(", uid=") {
            Some((name, _)) => name,
            None if d.starts_with("uid=") => "",
            None => d,
        })
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_owned);
    Some((head, Source { process, inner: false }))
}

/// `host:port` or `[v6]:port`.
fn endpoint(s: &str) -> Option<Endpoint> {
    let (host, port) = s.rsplit_once(':')?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host.is_empty() || host == "<nil>" {
        return None;
    }
    Some(Endpoint { host: host.to_ascii_lowercase(), port: port.parse().ok()? })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(s: &str) -> DialError {
        match parse(s) {
            Some(Line::DialError(e)) => e,
            other => panic!("{s}: {other:?}"),
        }
    }

    #[test]
    fn real_dial_error() {
        let e = err("[TCP] dial DIRECT (match Match/) 198.18.0.1:38164(Telegram, uid=1000) --> 142.252.58.41:2137 error: dial tcp 142.252.58.41:2137: i/o timeout");
        assert_eq!(e.proto, Proto::Tcp);
        assert_eq!(e.route, Route { rule: "Match".into(), payload: "".into(), target: "DIRECT".into() });
        assert_eq!(e.source.process.as_deref(), Some("Telegram"));
        assert_eq!(e.dest, Endpoint { host: "142.252.58.41".into(), port: 2137 });
        assert_eq!(e.error, "dial tcp 142.252.58.41:2137: i/o timeout");
    }

    #[test]
    fn names_with_spaces_commas_and_slashes() {
        let e = err("[TCP] dial Стандартный 2 (match IPCIDR/91.108.4.0/22) 198.18.0.1:1(Web Content, Isolated, uid=1000) --> [2001:db8::1]:443 error: connect: connection refused");
        assert_eq!(e.route.target, "Стандартный 2");
        assert_eq!(e.route.rule, "IPCIDR");
        assert_eq!(e.route.payload, "91.108.4.0/22");
        assert_eq!(e.source.process.as_deref(), Some("Web Content, Isolated"));
        assert_eq!(e.dest.host, "2001:db8::1");
    }

    #[test]
    fn error_without_rule_and_source_variants() {
        let e = err("[UDP] dial GLOBAL 10.0.0.2:5000 --> Example.COM:443 error: x");
        assert_eq!(e.proto, Proto::Udp);
        assert_eq!(e.route, Route { target: "GLOBAL".into(), ..Default::default() });
        assert_eq!(e.source.process, None);
        assert_eq!(e.dest.host, "example.com");

        let e = err("[TCP] dial PROXY (match DomainSuffix/a.com) 10.0.0.2:5000(uid=1000) --> a.com:443 error: x");
        assert_eq!(e.source.process, None);

        let e = err("[TCP] dial PROXY (match DomainSuffix/a.com) mihomo --> a.com:443 error: x");
        assert!(e.source.inner);
    }

    #[test]
    fn connected_lines() {
        let Some(Line::Connected(c)) = parse("[TCP] 198.18.0.1:48082(chromium, uid=1000) --> www.google.com:443 match DomainSuffix(google.com) using WARP[Стандартный 2]") else { panic!() };
        assert_eq!(c.source.process.as_deref(), Some("chromium"));
        assert_eq!(c.dest.host, "www.google.com");
        assert_eq!(c.chain, "WARP[Стандартный 2]");

        let Some(Line::Connected(c)) = parse("[UDP] 198.18.0.1:5353 --> 1.1.1.1:53 using DIRECT") else { panic!() };
        assert_eq!(c.chain, "DIRECT");
        assert!(parse("[TCP] 1.2.3.4:5 --> x.com:443 doesn't match any rule using DIRECT").is_some());
    }

    #[test]
    fn ignores_other_lines() {
        assert_eq!(parse("Start initial configuration in progress"), None);
        assert_eq!(parse("[DNS] resolve a.com error: x"), None);
        assert_eq!(parse("[TCP] dial X 1.2.3.4:5 --> <nil>:0 error: x"), None);
    }
}
