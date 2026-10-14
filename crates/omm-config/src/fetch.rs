//! Downloading subscriptions.

use std::time::Duration;

use reqwest::StatusCode;
use reqwest::header::{CONTENT_DISPOSITION, ETAG, HeaderMap, IF_NONE_MATCH, USER_AGENT};

use crate::error::{Error, Result};
use crate::profiles::Usage;
use crate::yaml::{Value, parse_mapping};

/// Providers serve full Clash/mihomo YAML to clients that look like one.
pub const DEFAULT_USER_AGENT: &str = concat!("clash.meta omarchy-mihomo/", env!("CARGO_PKG_VERSION"));

pub struct Request<'a> {
    pub url: &'a str,
    pub user_agent: Option<&'a str>,
    pub etag: Option<&'a str>,
    /// Retry through this proxy (the core's mixed port) if direct fails:
    /// subscription hosts are often blocked exactly where a proxy is needed.
    pub fallback_proxy: Option<String>,
}

#[derive(Debug)]
pub enum Fetched {
    NotModified,
    New(Download),
}

#[derive(Debug)]
pub struct Download {
    pub body: String,
    pub etag: Option<String>,
    pub usage: Option<Usage>,
    pub interval_hours: Option<u32>,
    /// Suggested name from `content-disposition`.
    pub filename: Option<String>,
    pub via_proxy: bool,
}

pub async fn fetch(req: &Request<'_>) -> Result<Fetched> {
    match get(req, None).await {
        Ok(r) => Ok(r),
        Err(direct) => match &req.fallback_proxy {
            Some(proxy) => {
                tracing::info!("direct download failed ({direct}), retrying through {proxy}");
                get(req, Some(proxy)).await.map_err(|e| Error::Fetch(format!("{direct}; via proxy: {e}")))
            }
            None => Err(Error::Fetch(direct)),
        },
    }
}

async fn get(req: &Request<'_>, proxy: Option<&str>) -> Result<Fetched, String> {
    let mut client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .no_proxy();
    if let Some(p) = proxy {
        client = client.proxy(reqwest::Proxy::all(p).map_err(|e| e.to_string())?);
    }
    let client = client.build().map_err(|e| e.to_string())?;

    let mut rb = client.get(req.url).header(USER_AGENT, req.user_agent.unwrap_or(DEFAULT_USER_AGENT));
    if let Some(tag) = req.etag {
        rb = rb.header(IF_NONE_MATCH, tag);
    }
    let resp = rb.send().await.map_err(|e| without_url(&e))?;
    if resp.status() == StatusCode::NOT_MODIFIED {
        return Ok(Fetched::NotModified);
    }
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let headers = resp.headers().clone();
    let body = resp.text().await.map_err(|e| without_url(&e))?;
    check_body(&body)?;
    Ok(Fetched::New(Download {
        body,
        etag: header(&headers, ETAG.as_str()),
        usage: header(&headers, "subscription-userinfo").and_then(|v| parse_userinfo(&v)),
        interval_hours: header(&headers, "profile-update-interval").and_then(|v| v.trim().parse().ok()),
        filename: header(&headers, CONTENT_DISPOSITION.as_str()).and_then(|v| parse_filename(&v)),
        via_proxy: proxy.is_some(),
    }))
}

/// Subscription URLs carry tokens; keep them out of errors and logs.
fn without_url(e: &reqwest::Error) -> String {
    let mut s = e.to_string();
    if let Some(url) = e.url() {
        s = s.replace(url.as_str(), "<subscription>");
    }
    s
}

fn header(h: &HeaderMap, name: &str) -> Option<String> {
    h.get(name).and_then(|v| v.to_str().ok()).map(str::to_owned)
}

/// The body must be a mihomo config, not a base64 link list or an HTML page.
fn check_body(body: &str) -> Result<(), String> {
    let trimmed = body.trim_start();
    if trimmed.starts_with('<') {
        return Err("server returned HTML instead of a config".into());
    }
    let looks_base64 = !trimmed.contains(':') && trimmed.len() > 32;
    let map = match parse_mapping(body, "subscription") {
        Ok(m) => m,
        Err(_) if looks_base64 => return Err("server returned a link list, not a Clash/mihomo config (try another user agent)".into()),
        Err(e) => return Err(e.to_string()),
    };
    let has = |k: &str| map.get(k).is_some_and(|v| !matches!(v, Value::Null));
    if !(has("proxies") || has("proxy-providers") || has("proxy-groups")) {
        return Err("config has no proxies, proxy-providers or proxy-groups".into());
    }
    Ok(())
}

/// `upload=1; download=2; total=3; expire=4`
pub fn parse_userinfo(v: &str) -> Option<Usage> {
    let mut u = Usage::default();
    let mut any = false;
    for part in v.split(';') {
        let Some((k, val)) = part.split_once('=') else { continue };
        // Some providers send floats or empty values.
        let n = val.trim().parse::<f64>().ok().filter(|n| *n >= 0.0).map(|n| n as u64);
        match (k.trim().to_ascii_lowercase().as_str(), n) {
            ("upload", Some(n)) => u.upload = n,
            ("download", Some(n)) => u.download = n,
            ("total", Some(n)) => u.total = n,
            ("expire", Some(n)) if n > 0 => u.expire = Some(n),
            _ => continue,
        }
        any = true;
    }
    any.then_some(u)
}

/// `attachment; filename*=UTF-8''%D0%9C%D0%BE%D0%B9.yaml` or `filename="x.yaml"`.
pub fn parse_filename(v: &str) -> Option<String> {
    let mut plain = None;
    for part in v.split(';').map(str::trim) {
        if let Some(rest) = part.strip_prefix("filename*=") {
            let enc = rest.split_once("''").map_or(rest, |(_, s)| s);
            if let Some(name) = percent_decode(enc.trim_matches('"')) {
                return Some(clean_name(&name));
            }
        } else if let Some(rest) = part.strip_prefix("filename=") {
            plain = Some(clean_name(rest.trim_matches('"')));
        }
    }
    plain.filter(|n| !n.is_empty())
}

fn clean_name(n: &str) -> String {
    let n = n.trim();
    [".yaml", ".yml"].iter().find_map(|ext| n.strip_suffix(ext)).unwrap_or(n).to_owned()
}

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn userinfo() {
        assert_eq!(
            parse_userinfo("upload=455727941; download=6174315083; total=1073741824000; expire=1798675200"),
            Some(Usage { upload: 455727941, download: 6174315083, total: 1073741824000, expire: Some(1798675200) })
        );
        assert_eq!(parse_userinfo("upload=0;download=1.5e3;total=;expire=0"), Some(Usage { download: 1500, ..Default::default() }));
        assert_eq!(parse_userinfo("garbage"), None);
    }

    #[test]
    fn filenames() {
        assert_eq!(parse_filename("attachment; filename*=UTF-8''%D0%9C%D0%BE%D0%B9.yaml").as_deref(), Some("Мой"));
        assert_eq!(parse_filename(r#"attachment; filename="Config.yml""#).as_deref(), Some("Config"));
        assert_eq!(parse_filename("attachment").as_deref(), None);
    }

    #[test]
    fn body_checks() {
        assert!(check_body("proxies:\n  - {name: a, type: ss}\n").is_ok());
        assert!(check_body("proxy-providers: {p: {type: http, url: x}}\n").is_ok());
        assert!(check_body("<html>blocked</html>").unwrap_err().contains("HTML"));
        assert!(check_body("dm1lc3M6Ly9leUoySWpvaU1pSXNJbkJ6SWpvaVlTSXNJbUZrWkNJNkltRWlmUT09Cg==").unwrap_err().contains("link list"));
        assert!(check_body("mode: rule\n").unwrap_err().contains("no proxies"));
    }
}
