//! Filtered controller relay: no upgrades, arbitrary reload paths or file APIs.
use crate::Service;
use anyhow::{Result, bail};
use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited, combinators::UnsyncBoxBody};
use hyper::{Method, Request, Response, body::Incoming};
use hyper_util::rt::TokioIo;
use std::{convert::Infallible, path::Path, sync::Arc};
use tokio::net::{UnixListener, UnixStream};
type Body = UnsyncBoxBody<Bytes, anyhow::Error>;
fn full(bytes: impl Into<Bytes>) -> Body {
    Full::new(bytes.into())
        .map_err(|never| match never {})
        .boxed_unsync()
}
fn error(message: &str) -> Response<Body> {
    Response::builder()
        .status(400)
        .header("Content-Type", "application/json")
        .body(full(serde_json::json!({"message":message}).to_string()))
        .unwrap()
}
fn allowed(method: &Method, path: &str) -> bool {
    let parts: Vec<_> = path.split('/').filter(|s| !s.is_empty()).collect();
    if parts.iter().any(|p| *p == "." || *p == "..") {
        return false;
    }
    match *method {
        Method::GET => {
            matches!(
                path,
                "/version"
                    | "/configs"
                    | "/proxies"
                    | "/rules"
                    | "/providers/rules"
                    | "/providers/proxies"
                    | "/connections"
                    | "/logs"
                    | "/traffic"
                    | "/memory"
            ) || matches!(
                parts.as_slice(),
                ["proxies", _] | ["proxies", _, "delay"] | ["group", _, "delay"]
            )
        }
        Method::PATCH => path == "/configs",
        Method::PUT => matches!(parts.as_slice(), ["proxies", _]),
        Method::DELETE => path == "/connections" || matches!(parts.as_slice(), ["connections", _]),
        _ => false,
    }
}
async fn forward(req: Request<Incoming>, service: Arc<Service>) -> Result<Response<Body>> {
    if !allowed(req.method(), req.uri().path()) || req.headers().contains_key("upgrade") {
        bail!("controller operation is not allowed");
    }
    if !service.runtime.lock().await.leased {
        bail!("no active network session");
    }
    let (parts, body) = req.into_parts();
    let bytes = Limited::new(body, 1024 * 1024)
        .collect()
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
        .to_bytes();
    if parts.method == Method::PATCH {
        if !crate::config::allowed_patch(&serde_json::from_slice(&bytes)?) {
            bail!("configuration patch is not allowed");
        }
    }
    if parts.method == Method::PUT {
        let v: serde_json::Value = serde_json::from_slice(&bytes)?;
        if !v.as_object().is_some_and(|m| {
            m.len() == 1 && m.get("name").is_some_and(serde_json::Value::is_string)
        }) {
            bail!("invalid proxy selection");
        }
    }
    let socket = UnixStream::connect(service.controller()).await?;
    let (mut sender, connection) =
        hyper::client::conn::http1::handshake(TokioIo::new(socket)).await?;
    tokio::spawn(async move {
        let _ = connection.await;
    });
    let request = Request::builder()
        .method(parts.method)
        .uri(parts.uri)
        .header("Host", "mihomo")
        .header("Content-Type", "application/json")
        .body(Full::new(bytes))?;
    let response = sender.send_request(request).await?;
    Ok(response.map(|b| b.map_err(anyhow::Error::from).boxed_unsync()))
}
pub async fn serve(listener: UnixListener, service: Arc<Service>) {
    while let Ok((socket, _)) = listener.accept().await {
        if socket.peer_cred().map(|p| p.uid()).ok() != Some(service.user) {
            continue;
        }
        let service = service.clone();
        tokio::spawn(async move {
            let handler = hyper::service::service_fn(move |request| {
                let service = service.clone();
                async move {
                    Ok::<_, Infallible>(match forward(request, service).await {
                        Ok(r) => r,
                        Err(e) => error(&e.to_string()),
                    })
                }
            });
            let _ = hyper::server::conn::http1::Builder::new()
                .serve_connection(TokioIo::new(socket), handler)
                .await;
        });
    }
}
pub async fn reload(socket: &Path) -> Result<()> {
    let stream = UnixStream::connect(socket).await?;
    let (mut sender, connection) =
        hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;
    tokio::spawn(async move {
        let _ = connection.await;
    });
    let request = Request::builder()
        .method(Method::PUT)
        .uri("/configs")
        .header("Host", "mihomo")
        .header("Content-Type", "application/json")
        .body(Full::new(Bytes::from_static(b"{\"path\":\"config.yaml\"}")))?;
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        sender.send_request(request),
    )
    .await??;
    if !response.status().is_success() {
        bail!("Mihomo rejected the reload");
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blocks_sensitive_controller_routes() {
        for path in [
            "/configs",
            "/restart",
            "/upgrade",
            "/upgrade/ui",
            "/cache/fakeip/flush",
        ] {
            assert!(!allowed(&Method::PUT, path));
        }
        assert!(allowed(&Method::PUT, "/proxies/AI"));
        assert!(allowed(&Method::GET, "/proxies/AI/delay"));
        assert!(!allowed(&Method::GET, "/providers/proxies/../../configs"));
    }
}
