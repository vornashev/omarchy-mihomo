//! DNS on top of the profile's: what the core uses now, the user's own
//! values, and a way out of a DNS sinkhole for one site.

use std::sync::Arc;

use omm_config::settings::{Dns, Policy};
use omm_proto::{ErrorCode, RpcError};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::state::State;

/// Used for a site's policy when the config names no DoH server.
const FALLBACK_DOH: &str = "https://1.1.1.1/dns-query";

/// The `dns` and `sniffer` sections of the config the core runs.
fn effective(state: &State) -> Value {
    let path = state.config.paths().runtime_config();
    let config = std::fs::read_to_string(&path).ok().and_then(|t| omm_config::yaml::parse_mapping(&t, "config.yaml").ok()).unwrap_or_default();
    let dns = config.get("dns").map(|v| serde_json::to_value(v).unwrap_or_default()).unwrap_or(Value::Null);
    let sniffer = config.get("sniffer").and_then(|s| s.get("enable")).and_then(|v| v.as_bool()).unwrap_or(false);
    json!({ "dns": dns, "sniffer": sniffer })
}

/// The first encrypted server the core already trusts.
fn doh_server(effective: &Value) -> String {
    effective["dns"]["nameserver"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .find(|s| s.starts_with("https://") || s.starts_with("tls://") || s.starts_with("quic://"))
        .unwrap_or(FALLBACK_DOH)
        .to_owned()
}

/// `example.org`, `*.example.org` and `+.example.org` all mean the domain
/// with its subdomains, as mihomo writes it.
fn policy_domain(domain: &str) -> Option<String> {
    let d = domain.trim().trim_start_matches("+.").trim_start_matches("*.").trim_start_matches('.').to_ascii_lowercase();
    let valid = !d.is_empty() && d.contains('.') && d.chars().all(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_'));
    valid.then(|| format!("+.{d}"))
}

async fn info(state: &State) -> Value {
    let settings = state.config.settings().await.dns;
    let effective = effective(state);
    json!({ "settings": settings, "effective": effective, "doh": doh_server(&effective) })
}

async fn save(state: &Arc<State>, dns: &Dns) -> Result<(), RpcError> {
    let patch = json!({ "dns": serde_json::to_value(dns).map_err(|e| RpcError::new(ErrorCode::Internal, e.to_string()))? });
    state.config.set_settings(state, patch).await.map(drop)
}

pub async fn rpc(state: &Arc<State>, method: &str, params: Value) -> Result<Value, RpcError> {
    let bad = |e: serde_json::Error| RpcError::new(ErrorCode::BadRequest, e.to_string());
    match method {
        "dns.get" => {}
        // A whole `Dns`: lists replace, `null` hands a value back to the profile.
        "dns.set" => {
            let dns: Dns = serde_json::from_value(params).map_err(bad)?;
            save(state, &dns).await?;
        }
        // One site through another server: the cure for a DNS sinkhole.
        "dns.policy_add" => {
            #[derive(Deserialize)]
            struct P {
                domain: String,
                server: Option<String>,
            }
            let p: P = serde_json::from_value(params).map_err(bad)?;
            let domain = policy_domain(&p.domain).ok_or_else(|| RpcError::new(ErrorCode::BadRequest, format!("not a domain: {}", p.domain)))?;
            let server = p.server.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| doh_server(&effective(state)));
            let mut dns = state.config.settings().await.dns;
            dns.policy.retain(|x| x.domain != domain);
            dns.policy.push(Policy { domain, server });
            save(state, &dns).await?;
        }
        "dns.query" => {
            #[derive(Deserialize)]
            struct P {
                name: String,
                #[serde(rename = "type")]
                qtype: Option<String>,
            }
            let p: P = serde_json::from_value(params).map_err(bad)?;
            let qtype = p.qtype.unwrap_or_else(|| "A".into());
            return state.core.dns_query(p.name.trim(), &qtype).await.map_err(|e| RpcError::new(ErrorCode::Core, e.to_string()));
        }
        _ => return Err(RpcError::new(ErrorCode::UnknownMethod, method.to_owned())),
    }
    Ok(info(state).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_domains() {
        assert_eq!(policy_domain("Example.org").as_deref(), Some("+.example.org"));
        assert_eq!(policy_domain("*.example.org").as_deref(), Some("+.example.org"));
        assert_eq!(policy_domain("+.example.org").as_deref(), Some("+.example.org"));
        assert_eq!(policy_domain("localhost"), None);
        assert_eq!(policy_domain("a b.org"), None);
    }

    #[test]
    fn doh_from_the_config() {
        let e = json!({ "dns": { "nameserver": ["223.5.5.5", "https://dns.quad9.net/dns-query"] } });
        assert_eq!(doh_server(&e), "https://dns.quad9.net/dns-query");
        assert_eq!(doh_server(&json!({ "dns": null })), FALLBACK_DOH);
    }
}
