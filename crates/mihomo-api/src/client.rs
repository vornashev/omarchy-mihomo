use std::path::{Path, PathBuf};
use std::time::Duration;

use bytes::{Bytes, BytesMut};
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::header::{CONTENT_TYPE, HOST};
use hyper::{Method, Request, Response};
use hyper_util::rt::TokioIo;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::net::UnixStream;

use crate::error::{Error, Result};
use crate::model::*;

/// Characters left unescaped in a path segment (RFC 3986 unreserved).
const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'_').remove(b'.').remove(b'~');

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// Handle to one mihomo core. Cheap to clone; every request opens a fresh
/// connection, which costs next to nothing on a unix socket and keeps the
/// client free of pooling state.
#[derive(Debug, Clone)]
pub struct Client {
    socket: PathBuf,
}

impl Client {
    pub fn new(socket: impl Into<PathBuf>) -> Self {
        Self { socket: socket.into() }
    }

    pub fn socket(&self) -> &Path {
        &self.socket
    }

    // ------------------------------------------------------------ transport

    async fn send(&self, method: Method, uri: &str, body: Option<&Value>) -> Result<Response<Incoming>> {
        let stream = UnixStream::connect(&self.socket).await.map_err(|source| Error::Connect {
            path: self.socket.display().to_string(),
            source,
        })?;
        let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;
        tokio::spawn(async move {
            if let Err(e) = conn.await {
                tracing::trace!("mihomo connection closed: {e}");
            }
        });

        let mut req = Request::builder().method(method).uri(uri).header(HOST, "mihomo");
        let body = match body {
            Some(v) => {
                req = req.header(CONTENT_TYPE, "application/json");
                Full::new(Bytes::from(serde_json::to_vec(v)?))
            }
            None => Full::new(Bytes::new()),
        };
        let resp = sender.send_request(req.body(body)?).await?;

        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        let bytes = resp.into_body().collect().await?.to_bytes();
        let message = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|v| v.get("message").and_then(Value::as_str).map(str::to_owned))
            .unwrap_or_else(|| String::from_utf8_lossy(&bytes).into_owned());
        Err(Error::Status { status: status.as_u16(), message })
    }

    /// Plain request with a deadline; returns the raw body.
    pub async fn call(&self, method: Method, uri: &str, body: Option<&Value>, timeout: Duration) -> Result<Bytes> {
        let work = async {
            let resp = self.send(method, uri, body).await?;
            Ok(resp.into_body().collect().await?.to_bytes())
        };
        tokio::time::timeout(timeout, work).await.map_err(|_| Error::Timeout(timeout))?
    }

    async fn get<T: DeserializeOwned>(&self, uri: &str) -> Result<T> {
        let bytes = self.call(Method::GET, uri, None, DEFAULT_TIMEOUT).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    async fn exec(&self, method: Method, uri: &str, body: Option<Value>) -> Result<()> {
        self.call(method, uri, body.as_ref(), DEFAULT_TIMEOUT).await.map(drop)
    }

    /// Open a streaming endpoint (`/logs`, `/traffic`, `/memory`).
    pub async fn stream(&self, uri: &str) -> Result<LineStream> {
        let resp = self.send(Method::GET, uri, None).await?;
        Ok(LineStream { body: resp.into_body(), buf: BytesMut::new() })
    }

    // ------------------------------------------------------------ endpoints

    pub async fn version(&self) -> Result<Version> {
        self.get("/version").await
    }

    pub async fn configs(&self) -> Result<BaseConfig> {
        self.get("/configs").await
    }

    /// `PATCH /configs` with a partial config, e.g. `{"mode":"global"}`.
    pub async fn patch_configs(&self, patch: Value) -> Result<()> {
        self.exec(Method::PATCH, "/configs", Some(patch)).await
    }

    /// Reload the core from a config file under its home directory.
    pub async fn reload_config(&self, path: &Path, force: bool) -> Result<()> {
        if omm_platform::IS_MACOS && self.socket == omm_platform::core_socket() {
            let yaml = tokio::fs::read_to_string(path).await.map_err(|source| Error::Connect { path:path.display().to_string(), source })?;
            return omm_platform::helper::call("core.reload", json!({"yaml":yaml})).await.map(drop)
                .map_err(|e| Error::Status { status:400, message:e.to_string() });
        }
        let uri = if force { "/configs?force=true" } else { "/configs" };
        let body = json!({ "path": path.to_string_lossy() });
        // Reloading re-resolves providers and can take a while.
        self.call(Method::PUT, uri, Some(&body), Duration::from_secs(60)).await.map(drop)
    }

    /// Download GeoIP, GeoSite and the MMDBs again; the core reloads its
    /// config afterwards. Fails with 400 while an update already runs.
    pub async fn update_geo(&self) -> Result<()> {
        self.call(Method::POST, "/configs/geo", Some(&json!({})), Duration::from_secs(300)).await.map(drop)
    }

    /// Resolve `name` the way the core does: `{Status, Answer: [{data, TTL, type}]}`.
    pub async fn dns_query(&self, name: &str, qtype: &str) -> Result<Value> {
        self.get(&format!("/dns/query?name={}&type={}", seg(name), seg(qtype))).await
    }

    pub async fn proxies(&self) -> Result<Proxies> {
        self.get("/proxies").await
    }

    /// Select `name` inside the selector group `group`.
    pub async fn select(&self, group: &str, name: &str) -> Result<()> {
        let uri = format!("/proxies/{}", seg(group));
        self.exec(Method::PUT, &uri, Some(json!({ "name": name }))).await
    }

    /// Measure the delay of one proxy or group against `url`.
    /// Fails with [`Error::Status`] 503/504 when the target is unreachable.
    pub async fn proxy_delay(&self, name: &str, url: &str, timeout_ms: u32) -> Result<Delay> {
        let uri = format!("/proxies/{}/delay?url={}&timeout={timeout_ms}", seg(name), seg(url));
        let bytes = self.call(Method::GET, &uri, None, delay_deadline(timeout_ms)).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Test every member of a group; `0` means the member failed.
    pub async fn group_delay(&self, group: &str, url: &str, timeout_ms: u32) -> Result<std::collections::BTreeMap<String, u32>> {
        let uri = format!("/group/{}/delay?url={}&timeout={timeout_ms}", seg(group), seg(url));
        let bytes = self.call(Method::GET, &uri, None, delay_deadline(timeout_ms)).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub async fn rules(&self) -> Result<Rules> {
        self.get("/rules").await
    }

    pub async fn rule_providers(&self) -> Result<RuleProviders> {
        self.get("/providers/rules").await
    }

    /// Download a rule provider again now.
    pub async fn update_rule_provider(&self, name: &str) -> Result<()> {
        self.call(Method::PUT, &format!("/providers/rules/{}", seg(name)), None, Duration::from_secs(60)).await.map(drop)
    }

    pub async fn connections(&self) -> Result<Connections> {
        self.get("/connections").await
    }

    pub async fn close_connection(&self, id: &str) -> Result<()> {
        self.exec(Method::DELETE, &format!("/connections/{}", seg(id)), None).await
    }

    pub async fn close_all_connections(&self) -> Result<()> {
        self.exec(Method::DELETE, "/connections", None).await
    }

    pub async fn logs(&self, level: &str) -> Result<LineStream> {
        self.stream(&format!("/logs?level={}", seg(level))).await
    }

    pub async fn traffic(&self) -> Result<LineStream> {
        self.stream("/traffic").await
    }

    pub async fn memory(&self) -> Result<LineStream> {
        self.stream("/memory").await
    }
}

/// Server-side timeout plus slack for the round trip.
fn delay_deadline(timeout_ms: u32) -> Duration {
    Duration::from_millis(u64::from(timeout_ms)) + Duration::from_secs(3)
}

fn seg(s: &str) -> String {
    utf8_percent_encode(s, SEGMENT).to_string()
}

/// Newline-delimited JSON read from a long-lived chunked response.
pub struct LineStream {
    body: Incoming,
    buf: BytesMut,
}

impl LineStream {
    /// Next non-empty line, or `None` when the core closes the stream.
    pub async fn next_line(&mut self) -> Result<Option<Bytes>> {
        loop {
            if let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
                let mut line = self.buf.split_to(pos + 1).freeze();
                line.truncate(pos);
                if line.last() == Some(&b'\r') {
                    line.truncate(line.len() - 1);
                }
                if line.is_empty() {
                    continue;
                }
                return Ok(Some(line));
            }
            match self.body.frame().await {
                Some(frame) => {
                    if let Ok(data) = frame?.into_data() {
                        self.buf.extend_from_slice(&data);
                    }
                }
                None if self.buf.is_empty() => return Ok(None),
                None => return Ok(Some(self.buf.split().freeze())),
            }
        }
    }

    /// Next line decoded as `T`.
    pub async fn next<T: DeserializeOwned>(&mut self) -> Result<Option<T>> {
        match self.next_line().await? {
            Some(line) => Ok(Some(serde_json::from_slice(&line)?)),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::seg;

    #[test]
    fn encodes_group_names() {
        assert_eq!(seg("AI"), "AI");
        assert_eq!(seg("Стриминг"), "%D0%A1%D1%82%D1%80%D0%B8%D0%BC%D0%B8%D0%BD%D0%B3");
        assert_eq!(seg("US West"), "US%20West");
        assert_eq!(seg("https://x.com/a?b"), "https%3A%2F%2Fx.com%2Fa%3Fb");
    }
}
