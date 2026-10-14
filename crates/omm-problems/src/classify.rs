//! What kind of failure a dial error is.
//!
//! This is a *diagnosis* shown to the user, never a verdict that a resource
//! is blocked: a timeout through DIRECT can be censorship, a dead server or a
//! flaky Wi-Fi. Nothing in this crate changes routing.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Timeout,
    /// Connection reset or closed mid-handshake.
    Reset,
    Refused,
    /// The name did not resolve.
    DnsFail,
    /// The name resolved to a sinkhole (127.0.0.1, 0.0.0.0): a block list
    /// or a tampered DNS answer. A proxy alone will not fix it.
    DnsStub,
    NoRoute,
    Tls,
    /// The proxy server itself failed (auth, protocol, unreachable node).
    Proxy,
    Other,
}

impl ErrorKind {
    pub const ALL: [ErrorKind; 9] = [
        ErrorKind::Timeout,
        ErrorKind::Reset,
        ErrorKind::Refused,
        ErrorKind::DnsFail,
        ErrorKind::DnsStub,
        ErrorKind::NoRoute,
        ErrorKind::Tls,
        ErrorKind::Proxy,
        ErrorKind::Other,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ErrorKind::Timeout => "timeout",
            ErrorKind::Reset => "reset",
            ErrorKind::Refused => "refused",
            ErrorKind::DnsFail => "dns_fail",
            ErrorKind::DnsStub => "dns_stub",
            ErrorKind::NoRoute => "no_route",
            ErrorKind::Tls => "tls",
            ErrorKind::Proxy => "proxy",
            ErrorKind::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

pub fn classify(error: &str, target: &str) -> ErrorKind {
    let e = error.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| e.contains(n));

    // A dial to loopback/unspecified means DNS handed out a sinkhole.
    if has(&["dial tcp 127.", "dial udp 127.", "dial tcp 0.0.0.0", "dial tcp [::1]", "dial tcp [::]:"]) {
        return ErrorKind::DnsStub;
    }
    if has(&["dns resolve failed", "couldn't find ip", "no such host", "all dns requests failed", "lookup "]) {
        return ErrorKind::DnsFail;
    }
    if has(&["tls:", "x509", "certificate", "handshake failure", "reality"]) {
        return ErrorKind::Tls;
    }
    if has(&["i/o timeout", "deadline exceeded", "timed out", "timeout"]) {
        return ErrorKind::Timeout;
    }
    if has(&["connection reset", "broken pipe", "unexpected eof", "forcibly closed"]) || e.ends_with(": eof") || e == "eof" {
        return ErrorKind::Reset;
    }
    if has(&["connection refused"]) {
        return ErrorKind::Refused;
    }
    if has(&["no route to host", "network is unreachable", "host is unreachable"]) {
        return ErrorKind::NoRoute;
    }
    // Errors reported by a proxy outbound rather than the network path.
    if target != "DIRECT" && has(&["authentication", "auth failed", "unauthorized", "invalid user", "proxy", "socks", "http connect", "403", "407"]) {
        return ErrorKind::Proxy;
    }
    ErrorKind::Other
}

#[cfg(test)]
mod tests {
    use super::*;
    use ErrorKind::*;

    #[test]
    fn common_errors() {
        let cases = [
            ("dial tcp 142.252.58.41:2137: i/o timeout", "DIRECT", Timeout),
            ("context deadline exceeded", "AI", Timeout),
            ("read tcp 1.2.3.4:5->6.7.8.9:443: read: connection reset by peer", "DIRECT", Reset),
            ("unexpected EOF", "PROXY", Reset),
            ("dial tcp 127.0.0.1:443: connect: connection refused", "DIRECT", DnsStub),
            ("dial tcp 1.2.3.4:443: connect: connection refused", "DIRECT", Refused),
            ("dns resolve failed: couldn't find ip", "DIRECT", DnsFail),
            ("dial tcp: lookup example.invalid: no such host", "DIRECT", DnsFail),
            ("dial tcp 1.2.3.4:443: connect: no route to host", "DIRECT", NoRoute),
            ("dial tcp [2a00::1]:443: connect: network is unreachable", "DIRECT", NoRoute),
            ("tls: failed to verify certificate: x509: certificate signed by unknown authority", "PROXY", Tls),
            ("socks5 authentication failed", "PROXY", Proxy),
            ("something new", "DIRECT", Other),
        ];
        for (e, t, want) in cases {
            assert_eq!(classify(e, t), want, "{e}");
        }
    }

    #[test]
    fn names_roundtrip() {
        for k in ErrorKind::ALL {
            assert_eq!(ErrorKind::parse(k.as_str()), Some(k));
            assert_eq!(serde_json::to_value(k).unwrap(), serde_json::Value::from(k.as_str()));
        }
    }
}
