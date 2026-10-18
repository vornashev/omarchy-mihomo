//! Unix-socket server speaking the newline-delimited JSON protocol.

use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use omm_proto::{ErrorCode, Request, Response, RpcError, topic};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc};

use crate::methods;
use crate::state::{State, Subscription};

/// Bind the socket in a private directory, refusing to steal it from a live
/// daemon but cleaning up after a dead one.
pub fn bind(path: &Path) -> Result<UnixListener> {
    if let Some(dir) = path.parent() {
        fs::DirBuilder::new().recursive(true).mode(0o700).create(dir).with_context(|| format!("create {}", dir.display()))?;
    }
    if path.exists() {
        if std::os::unix::net::UnixStream::connect(path).is_ok() {
            bail!("another omarchy-mihomod is already listening on {}", path.display());
        }
        fs::remove_file(path).with_context(|| format!("remove stale {}", path.display()))?;
    }
    let listener = UnixListener::bind(path).with_context(|| format!("bind {}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    tracing::info!(socket = %path.display(), "listening");
    Ok(listener)
}

pub async fn serve(listener: UnixListener, state: Arc<State>) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(client(stream, state.clone()));
            }
            Err(e) => tracing::warn!("accept: {e}"),
        }
    }
}

async fn client(stream: UnixStream, state: Arc<State>) {
    let (read, mut write) = stream.into_split();
    let (tx, mut rx) = mpsc::channel::<String>(64);

    let writer = tokio::spawn(async move {
        while let Some(mut line) = rx.recv().await {
            line.push('\n');
            if write.write_all(line.as_bytes()).await.is_err() {
                break;
            }
        }
    });

    let mut lines = BufReader::new(read).lines();
    let mut events: Option<tokio::task::JoinHandle<()>> = None;
    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let req: Request = match serde_json::from_str(line) {
            Ok(r) => r,
            Err(e) => {
                let resp = Response::err(0, RpcError::new(ErrorCode::BadRequest, e.to_string()));
                let _ = tx.send(encode(&resp)).await;
                continue;
            }
        };

        // Subscriptions are per-connection state, so they are handled here.
        if req.method == "events.subscribe" {
            let resp = match parse_topics(&req.params) {
                Ok(topics) => {
                    if let Some(old) = events.take() {
                        old.abort();
                    }
                    let sub = state.subscribe(topics.clone());
                    // Capture the tail before yielding, then enqueue it before live
                    // events. The current-thread runtime keeps this handoff atomic.
                    if topics.contains(&topic::LOGS) {
                        let snapshot = serde_json::json!({
                            "event": topic::LOGS,
                            "data": { "lines": state.logs.lock().unwrap().snapshot() }
                        });
                        let _ = tx.send(encode(&snapshot)).await;
                    }
                    events = Some(tokio::spawn(forward(sub, tx.clone())));
                    // Send the current status right away so clients need no extra round trip.
                    if topics.contains(&topic::STATUS) {
                        let status = omm_proto::Event { event: topic::STATUS.into(), data: serde_json::to_value(state.status()).unwrap_or_default() };
                        let _ = tx.send(encode(&status)).await;
                    }
                    if topics.contains(&topic::PROBLEMS) {
                        let counts = state.problems.lock().unwrap().counts();
                        let update = crate::problems::Update { counts, recorded: 0 };
                        let ev = omm_proto::Event { event: topic::PROBLEMS.into(), data: serde_json::to_value(update).unwrap_or_default() };
                        let _ = tx.send(encode(&ev)).await;
                    }
                    Response::ok(req.id, serde_json::json!({ "topics": topics }))
                }
                Err(e) => Response::err(req.id, e),
            };
            let _ = tx.send(encode(&resp)).await;
            continue;
        }

        // Requests run concurrently: a 5 s delay test must not block the UI.
        let (state, tx) = (state.clone(), tx.clone());
        tokio::spawn(async move {
            let resp = match methods::dispatch(&state, &req.method, req.params).await {
                Ok(v) => Response::ok(req.id, v),
                Err(e) => Response::err(req.id, e),
            };
            let _ = tx.send(encode(&resp)).await;
        });
    }

    if let Some(h) = events {
        h.abort();
    }
    drop(tx);
    let _ = writer.await;
}

async fn forward(mut sub: Subscription, tx: mpsc::Sender<String>) {
    loop {
        match sub.rx.recv().await {
            Ok(ev) if sub.wants(ev.topic) => {
                if tx.send(ev.line.clone()).await.is_err() {
                    return;
                }
            }
            Ok(_) => {}
            Err(broadcast::error::RecvError::Lagged(n)) => tracing::debug!("client lagged, dropped {n} events"),
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

fn parse_topics(params: &Value) -> Result<Vec<&'static str>, RpcError> {
    let names = match params.get("topics") {
        None | Some(Value::Null) => return Ok(topic::ALL.to_vec()),
        Some(Value::Array(a)) => a,
        Some(_) => return Err(RpcError::new(ErrorCode::BadRequest, "topics must be an array")),
    };
    names
        .iter()
        .map(|v| {
            v.as_str()
                .and_then(topic::resolve)
                .ok_or_else(|| RpcError::new(ErrorCode::BadRequest, format!("unknown topic {v}")))
        })
        .collect()
}

fn encode(v: &impl serde::Serialize) -> String {
    serde_json::to_string(v).expect("frame serializes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mihomo_api::model::LogLine;

    #[tokio::test]
    async fn logs_subscription_delivers_history_then_live_and_reopens_without_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let paths = omm_config::paths::Paths::new(dir.path());
        paths.ensure().unwrap();
        let state = Arc::new(State::new(
            mihomo_api::Client::new(dir.path().join("core.sock")),
            crate::config::Manager::open(paths, None).unwrap(),
            omm_problems::Collector::open(&dir.path().join("state.db")).unwrap(),
            crate::system_proxy::Manager::open(dir.path().join("proxy.json")).unwrap(),
        ));
        state.logs.lock().unwrap().push(LogLine { level: "info".into(), payload: "before opening".into() }, 1);
        let (peer, server) = UnixStream::pair().unwrap();
        let task = tokio::spawn(client(server, state.clone()));
        let (read, mut write) = peer.into_split();
        let mut frames = BufReader::new(read).lines();
        write.write_all(b"{\"id\":1,\"method\":\"events.subscribe\",\"params\":{\"topics\":[\"logs\"]}}\n").await.unwrap();
        let first = next(&mut frames).await;
        assert_eq!(first["data"]["lines"][0]["payload"], "before opening");
        assert_eq!(next(&mut frames).await["id"], 1);

        let entry = state.logs.lock().unwrap().push(LogLine { level: "warning".into(), payload: "live".into() }, 2);
        state.publish(topic::LOGS, &entry);
        let live = next(&mut frames).await;
        assert_eq!(live["data"]["payload"], "live");
        assert_eq!(live["data"]["timestamp"], 2);

        write.write_all(b"{\"id\":2,\"method\":\"events.subscribe\",\"params\":{\"topics\":[]}}\n").await.unwrap();
        assert_eq!(next(&mut frames).await["id"], 2);
        write.write_all(b"{\"id\":3,\"method\":\"events.subscribe\",\"params\":{\"topics\":[\"logs\"]}}\n").await.unwrap();
        let reopened = next(&mut frames).await;
        assert_eq!(reopened["data"]["lines"].as_array().unwrap().len(), 2);
        assert_eq!(next(&mut frames).await["id"], 3);
        task.abort();
    }

    async fn next(frames: &mut tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>) -> Value {
        let line = tokio::time::timeout(std::time::Duration::from_secs(2), frames.next_line()).await.unwrap().unwrap().unwrap();
        serde_json::from_str(&line).unwrap()
    }
}
