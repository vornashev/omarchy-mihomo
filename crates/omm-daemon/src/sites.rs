//! Site checks: the user's list of sites, each tested by a real request
//! through the core's mixed port, so it takes the route its traffic would
//! take. The route is read back from the core's connection while the
//! response is still open.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mihomo_api::model::Connection;
use omm_proto::{ErrorCode, RpcError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;

use crate::problems::now_ms;
use crate::state::State;

const TIMEOUT: Duration = Duration::from_millis(crate::route_check::CHECK_TIMEOUT_MS as u64);
const ICON_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_ICON: usize = 512 * 1024;
/// Checks at once; "check all" sends one request per site.
const CONCURRENCY: usize = 6;
const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";
const ICON_EXTS: &[&str] = &["png", "ico", "svg", "jpg", "gif", "webp"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Site {
    pub id: String,
    pub name: String,
    pub url: String,
    /// `None`: the site's favicon; `letter`: none; else an image URL or file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    /// `core_down`, `timeout`, `connect` or the error text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Outbounds, top group first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub chain: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    pub at: i64,
}

const DEFAULTS: &[(&str, &str)] = &[
    ("YouTube", "https://www.youtube.com/"),
    ("Telegram", "https://web.telegram.org/"),
    ("Discord", "https://discord.com/"),
    ("Instagram", "https://www.instagram.com/"),
    ("ChatGPT", "https://chatgpt.com/"),
    ("Claude", "https://claude.ai/"),
    ("GitHub", "https://github.com/"),
    ("Google", "https://www.google.com/"),
    ("X", "https://x.com/"),
    ("Spotify", "https://open.spotify.com/"),
];

pub struct Sites {
    path: PathBuf,
    icons: PathBuf,
    pub list: Vec<Site>,
    results: HashMap<String, Outcome>,
    /// Icon sources that failed this run: not retried until a restart.
    failed: HashSet<String>,
    checks: Arc<Semaphore>,
    serial: u64,
}

impl Sites {
    /// The defaults until the user saves a list of their own.
    pub fn load(root: &Path) -> Self {
        let path = root.join("sites.json");
        let list = match std::fs::read_to_string(&path) {
            Ok(t) => serde_json::from_str(&t).unwrap_or_else(|e| {
                tracing::warn!("{}: {e}", path.display());
                defaults()
            }),
            Err(_) => defaults(),
        };
        Self { path, icons: root.join("favicons"), list, results: HashMap::new(), failed: HashSet::new(), checks: Arc::new(Semaphore::new(CONCURRENCY)), serial: 0 }
    }

    #[cfg(test)]
    fn in_memory(list: Vec<Site>) -> Self {
        Self { path: PathBuf::new(), icons: PathBuf::new(), list, results: HashMap::new(), failed: HashSet::new(), checks: Arc::new(Semaphore::new(1)), serial: 0 }
    }

    fn save(&self) -> Result<(), RpcError> {
        if self.path.as_os_str().is_empty() {
            return Ok(());
        }
        let text = serde_json::to_vec_pretty(&self.list).expect("sites serialize");
        omm_config::paths::write_atomic(&self.path, &text).map_err(|e| RpcError::new(ErrorCode::Internal, e.to_string()))
    }

    fn new_id(&mut self) -> String {
        self.serial += 1;
        format!("{:x}{}", now_ms(), self.serial)
    }

    /// Insert or replace by id; the URL is normalized and the name defaults to the host.
    fn upsert(&mut self, id: Option<String>, name: &str, url: &str, icon: Option<String>) -> Result<Site, RpcError> {
        let url = normalize_url(url).ok_or_else(|| bad(format!("not a web address: {url}")))?;
        let name = match name.trim() {
            "" => default_name(&url),
            n => n.to_owned(),
        };
        let icon = icon.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
        let id = id.filter(|i| self.list.iter().any(|s| &s.id == i));
        let site = Site { id: id.clone().unwrap_or_else(|| self.new_id()), name, url, icon };
        match id.and_then(|i| self.list.iter_mut().find(|s| s.id == i)) {
            Some(s) => {
                if s.url != site.url {
                    self.results.remove(&site.id);
                }
                *s = site.clone();
            }
            None => self.list.push(site.clone()),
        }
        Ok(site)
    }

    fn remove(&mut self, id: &str) -> bool {
        let before = self.list.len();
        self.list.retain(|s| s.id != id);
        self.results.remove(id);
        self.list.len() != before
    }

    /// The given ids first, in that order; sites left out keep theirs after them.
    fn order(&mut self, ids: &[String]) {
        let mut rest = std::mem::take(&mut self.list);
        for id in ids {
            if let Some(i) = rest.iter().position(|s| &s.id == id) {
                self.list.push(rest.remove(i));
            }
        }
        self.list.append(&mut rest);
    }

    /// Add the sites whose URL is not in the list yet; returns how many.
    pub fn merge(&mut self, sites: Vec<Site>) -> usize {
        let mut added = 0;
        for s in sites {
            if self.list.iter().any(|x| x.url == s.url) {
                continue;
            }
            if self.upsert(None, &s.name, &s.url, s.icon).is_ok() {
                added += 1;
            }
        }
        if added > 0
            && let Err(e) = self.save()
        {
            tracing::warn!("saving sites: {}", e.message);
        }
        added
    }

    fn snapshot(&self) -> Value {
        let sites: Vec<Value> = self
            .list
            .iter()
            .map(|s| {
                let mut v = serde_json::to_value(s).unwrap_or_default();
                if let Some(f) = self.cached_icon(s) {
                    v["icon_file"] = json!(f);
                }
                v
            })
            .collect();
        let results: HashMap<&String, &Outcome> = self.results.iter().filter(|(id, _)| self.list.iter().any(|s| &s.id == *id)).collect();
        json!({ "sites": sites, "results": results })
    }

    fn cached_icon(&self, s: &Site) -> Option<String> {
        let src = icon_source(s)?;
        if let IconSource::File(f) = &src {
            return Path::new(f).is_file().then(|| f.clone());
        }
        let stem = self.icons.join(src.key());
        ICON_EXTS.iter().map(|e| stem.with_extension(e)).find(|p| p.is_file()).map(|p| p.display().to_string())
    }
}

pub fn defaults() -> Vec<Site> {
    DEFAULTS.iter().enumerate().map(|(i, (name, url))| Site { id: format!("d{i}"), name: (*name).into(), url: (*url).into(), icon: None }).collect()
}

/// `youtube.com` → `https://youtube.com/`; only http and https with a host.
pub fn normalize_url(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() || s.contains(char::is_whitespace) {
        return None;
    }
    let with_scheme = if s.contains("://") { s.to_owned() } else { format!("https://{s}") };
    let url = reqwest::Url::parse(&with_scheme).ok()?;
    (matches!(url.scheme(), "http" | "https") && url.host_str().is_some_and(|h| !h.is_empty())).then(|| url.to_string())
}

fn host_of(url: &str) -> String {
    reqwest::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_owned)).unwrap_or_default()
}

/// `https://www.youtube.com/` → `youtube.com`.
fn default_name(url: &str) -> String {
    let host = host_of(url);
    host.strip_prefix("www.").unwrap_or(&host).to_owned()
}

/// The connection of our probe: the newest one to `host` that came in
/// through the mixed port from this machine.
fn probe_connection<'a>(conns: &'a [Connection], host: &str) -> Option<&'a Connection> {
    conns
        .iter()
        .filter(|c| {
            let m = &c.metadata;
            (m.host.eq_ignore_ascii_case(host) || m.sniff_host.eq_ignore_ascii_case(host))
                && m.kind.starts_with("HTTP")
                && matches!(m.source_ip.as_str(), "127.0.0.1" | "::1")
        })
        .max_by(|a, b| a.start.cmp(&b.start))
}

fn rule_of(c: &Connection) -> Option<String> {
    match (c.rule.as_str(), c.rule_payload.as_str()) {
        ("", _) => None,
        (r, "") => Some(r.to_owned()),
        (r, p) => Some(format!("{r},{p}")),
    }
}

async fn check(state: &State, site: &Site) -> Outcome {
    let mut out = Outcome { id: site.id.clone(), ok: false, delay: None, status: None, error: None, chain: Vec::new(), rule: None, at: now_ms() };
    let Some(proxy) = crate::config::proxy_of(state) else {
        out.error = Some("core_down".into());
        return out;
    };
    let client = reqwest::Proxy::all(&proxy)
        .and_then(|p| reqwest::Client::builder().no_proxy().proxy(p).timeout(TIMEOUT).redirect(reqwest::redirect::Policy::none()).build());
    let client = match client {
        Ok(c) => c,
        Err(e) => {
            out.error = Some(e.to_string());
            return out;
        }
    };
    let host = host_of(&site.url);
    let started = Instant::now();
    let resp = client.get(&site.url).header(reqwest::header::USER_AGENT, USER_AGENT).send().await;
    out.at = now_ms();
    match &resp {
        // Any HTTP answer means the site is reachable.
        Ok(r) => {
            out.ok = true;
            out.delay = Some(u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX).max(1));
            out.status = Some(r.status().as_u16());
        }
        Err(e) if e.is_timeout() => out.error = Some("timeout".into()),
        Err(e) if e.is_connect() => out.error = Some("connect".into()),
        Err(e) => out.error = Some(error_text(e)),
    }
    // The connection stays open while `resp` lives.
    if let Ok(conns) = state.core.connections().await
        && let Some(c) = probe_connection(&conns.connections, &host)
    {
        out.chain = c.chains.iter().rev().cloned().collect();
        out.rule = rule_of(c);
    }
    drop(resp);
    out
}

/// The innermost cause, without the URL reqwest puts in front.
fn error_text(e: &reqwest::Error) -> String {
    let mut src: &dyn std::error::Error = e;
    while let Some(next) = src.source() {
        src = next;
    }
    src.to_string()
}

enum IconSource {
    Auto(String),
    Url(String),
    File(String),
}

impl IconSource {
    /// File stem in the icon cache.
    fn key(&self) -> String {
        let s = match self {
            Self::Auto(h) => format!("auto:{h}"),
            Self::Url(u) | Self::File(u) => u.clone(),
        };
        Sha256::digest(s.as_bytes()).iter().take(8).map(|b| format!("{b:02x}")).collect()
    }
}

fn icon_source(s: &Site) -> Option<IconSource> {
    match s.icon.as_deref() {
        None => Some(IconSource::Auto(host_of(&s.url))).filter(|i| !matches!(i, IconSource::Auto(h) if h.is_empty())),
        Some("letter") => None,
        Some(i) if i.starts_with("file://") => Some(IconSource::File(i.trim_start_matches("file://").to_owned())),
        Some(i) if i.starts_with('/') => Some(IconSource::File(i.to_owned())),
        Some(i) => Some(IconSource::Url(i.to_owned())),
    }
}

/// File extension for image bytes; `None` for anything else (an HTML error page).
pub fn image_ext(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(b"\x89PNG") {
        Some("png")
    } else if b.starts_with(&[0, 0, 1, 0]) {
        Some("ico")
    } else if b.starts_with(b"GIF8") {
        Some("gif")
    } else if b.starts_with(&[0xff, 0xd8]) {
        Some("jpg")
    } else if b.len() > 12 && b.starts_with(b"RIFF") && &b[8..12] == b"WEBP" {
        Some("webp")
    } else if String::from_utf8_lossy(&b[..b.len().min(512)]).contains("<svg") {
        Some("svg")
    } else {
        None
    }
}

async fn download(client: &reqwest::Client, url: &str) -> Option<(Vec<u8>, &'static str)> {
    let r = client.get(url).header(reqwest::header::USER_AGENT, USER_AGENT).send().await.ok()?.error_for_status().ok()?;
    let b = r.bytes().await.ok()?;
    if b.len() > MAX_ICON {
        return None;
    }
    let ext = image_ext(&b)?;
    Some((b.to_vec(), ext))
}

async fn fetch_icon(state: &State, src: &IconSource, dir: &Path) -> Option<String> {
    let urls = match src {
        IconSource::Auto(host) => vec![format!("https://www.google.com/s2/favicons?domain={host}&sz=64"), format!("https://{host}/favicon.ico")],
        IconSource::Url(u) => vec![u.clone()],
        IconSource::File(_) => return None,
    };
    let mut b = reqwest::Client::builder().timeout(ICON_TIMEOUT).no_proxy();
    if let Some(p) = crate::config::proxy_of(state).and_then(|p| reqwest::Proxy::all(p).ok()) {
        b = b.proxy(p);
    }
    let client = b.build().ok()?;
    for url in urls {
        if let Some((bytes, ext)) = download(&client, &url).await {
            std::fs::create_dir_all(dir).ok()?;
            let path = dir.join(src.key()).with_extension(ext);
            omm_config::paths::write_atomic(&path, &bytes).ok()?;
            return Some(path.display().to_string());
        }
    }
    None
}

fn bad(m: impl Into<String>) -> RpcError {
    RpcError::new(ErrorCode::BadRequest, m)
}

fn find(state: &State, id: &str) -> Result<Site, RpcError> {
    state.sites.lock().unwrap().list.iter().find(|s| s.id == id).cloned().ok_or_else(|| RpcError::new(ErrorCode::NotFound, format!("no site {id}")))
}

pub async fn rpc(state: &Arc<State>, method: &str, params: Value) -> Result<Value, RpcError> {
    #[derive(Deserialize)]
    struct Id {
        id: String,
    }
    let parse_id = |p: Value| serde_json::from_value::<Id>(p).map(|p| p.id).map_err(|e| bad(e.to_string()));
    match method {
        "sites.list" => {}
        "sites.save" => {
            #[derive(Deserialize)]
            struct P {
                id: Option<String>,
                #[serde(default)]
                name: String,
                url: String,
                icon: Option<String>,
            }
            let p: P = serde_json::from_value(params).map_err(|e| bad(e.to_string()))?;
            let mut sites = state.sites.lock().unwrap();
            let site = sites.upsert(p.id, &p.name, &p.url, p.icon)?;
            sites.save()?;
            let mut v = sites.snapshot();
            v["saved"] = json!(site.id);
            return Ok(v);
        }
        "sites.remove" => {
            let id = parse_id(params)?;
            let mut sites = state.sites.lock().unwrap();
            if sites.remove(&id) {
                sites.save()?;
            }
        }
        "sites.order" => {
            #[derive(Deserialize)]
            struct P {
                ids: Vec<String>,
            }
            let p: P = serde_json::from_value(params).map_err(|e| bad(e.to_string()))?;
            let mut sites = state.sites.lock().unwrap();
            sites.order(&p.ids);
            sites.save()?;
        }
        "sites.defaults" => {
            state.sites.lock().unwrap().merge(defaults());
        }
        "sites.test" => {
            let site = find(state, &parse_id(params)?)?;
            let permits = state.sites.lock().unwrap().checks.clone();
            let _permit = permits.acquire_owned().await.map_err(|e| RpcError::new(ErrorCode::Internal, e.to_string()))?;
            let out = check(state, &site).await;
            state.sites.lock().unwrap().results.insert(site.id.clone(), out.clone());
            return Ok(serde_json::to_value(out).unwrap_or_default());
        }
        "sites.icon" => {
            let site = find(state, &parse_id(params)?)?;
            let (cached, dir) = {
                let sites = state.sites.lock().unwrap();
                (sites.cached_icon(&site), sites.icons.clone())
            };
            if cached.is_some() {
                return Ok(json!({ "file": cached }));
            }
            let Some(src) = icon_source(&site) else { return Ok(json!({ "file": null })) };
            let key = src.key();
            if state.sites.lock().unwrap().failed.contains(&key) {
                return Ok(json!({ "file": null }));
            }
            let file = fetch_icon(state, &src, &dir).await;
            if file.is_none() {
                state.sites.lock().unwrap().failed.insert(key);
            }
            return Ok(json!({ "file": file }));
        }
        _ => return Err(RpcError::new(ErrorCode::BadRequest, format!("unknown method {method}"))),
    }
    Ok(state.sites.lock().unwrap().snapshot())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mihomo_api::model::Metadata;

    fn site(id: &str, url: &str) -> Site {
        Site { id: id.into(), name: id.into(), url: url.into(), icon: None }
    }

    #[test]
    fn urls_are_normalized() {
        assert_eq!(normalize_url("youtube.com").as_deref(), Some("https://youtube.com/"));
        assert_eq!(normalize_url(" http://example.org/x ").as_deref(), Some("http://example.org/x"));
        assert_eq!(normalize_url("ftp://example.org"), None);
        assert_eq!(normalize_url("not a url"), None);
        assert_eq!(normalize_url(""), None);
        assert_eq!(default_name("https://www.youtube.com/"), "youtube.com");
    }

    #[test]
    fn upsert_order_remove() {
        let mut s = Sites::in_memory(vec![site("a", "https://a.com/"), site("b", "https://b.com/"), site("c", "https://c.com/")]);
        let new = s.upsert(None, "", "d.com", None).unwrap();
        assert_eq!(new.name, "d.com");
        assert_eq!(new.url, "https://d.com/");
        s.upsert(Some("a".into()), "A", "https://a.org", Some("letter".into())).unwrap();
        assert_eq!(s.list[0].name, "A");
        assert_eq!(s.list[0].icon.as_deref(), Some("letter"));
        // Unknown ids are ignored, omitted ones keep their order after the given ones.
        s.order(&["c".into(), "zzz".into(), "a".into()]);
        let ids: Vec<&str> = s.list.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(ids, ["c", "a", "b", new.id.as_str()]);
        assert!(s.remove("b"));
        assert!(!s.remove("b"));
        assert_eq!(s.list.len(), 3);
        assert!(s.upsert(None, "x", "mailto:x", None).is_err());
    }

    #[test]
    fn merge_skips_known_urls() {
        let mut s = Sites::in_memory(vec![site("a", "https://github.com/")]);
        assert_eq!(s.merge(defaults()), DEFAULTS.len() - 1);
        assert_eq!(s.merge(defaults()), 0);
    }

    fn conn(host: &str, kind: &str, src: &str, start: &str, chains: &[&str]) -> Connection {
        Connection {
            id: start.into(),
            metadata: Metadata { host: host.into(), kind: kind.into(), source_ip: src.into(), ..Default::default() },
            upload: 0,
            download: 0,
            start: start.into(),
            chains: chains.iter().map(|s| (*s).into()).collect(),
            rule: "DomainSuffix".into(),
            rule_payload: "youtube.com".into(),
        }
    }

    #[test]
    fn finds_the_probe_connection() {
        let conns = vec![
            conn("www.youtube.com", "Tun", "198.18.0.1", "2026-11-01T10:00:03Z", &["DIRECT"]),
            conn("www.youtube.com", "HTTPConnect", "127.0.0.1", "2026-11-01T10:00:01Z", &["old"]),
            conn("www.youtube.com", "HTTPConnect", "127.0.0.1", "2026-11-01T10:00:02Z", &["NL-02", "Europe"]),
            conn("other.com", "HTTPConnect", "127.0.0.1", "2026-11-01T10:00:04Z", &["DIRECT"]),
        ];
        let c = probe_connection(&conns, "WWW.youtube.com").unwrap();
        assert_eq!(c.chains, ["NL-02", "Europe"]);
        assert_eq!(rule_of(c).as_deref(), Some("DomainSuffix,youtube.com"));
        assert!(probe_connection(&conns, "nothing.com").is_none());
    }

    #[test]
    fn image_types() {
        assert_eq!(image_ext(b"\x89PNG\r\n"), Some("png"));
        assert_eq!(image_ext(&[0, 0, 1, 0, 1]), Some("ico"));
        assert_eq!(image_ext(b"<?xml version='1.0'?><svg xmlns=''/>"), Some("svg"));
        assert_eq!(image_ext(b"<!doctype html><html>"), None);
    }

    #[test]
    fn icon_sources() {
        let mut s = site("a", "https://www.youtube.com/");
        assert!(matches!(icon_source(&s), Some(IconSource::Auto(h)) if h == "www.youtube.com"));
        s.icon = Some("letter".into());
        assert!(icon_source(&s).is_none());
        s.icon = Some("file:///tmp/x.png".into());
        assert!(matches!(icon_source(&s), Some(IconSource::File(f)) if f == "/tmp/x.png"));
    }
}
