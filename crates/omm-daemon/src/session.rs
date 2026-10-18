//! Explicit network sessions. Linux retains its service-managed lifecycle.
use crate::state::State;
use omm_proto::{ErrorCode, RpcError};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::Mutex;

#[derive(Default)]
pub struct Session {
    lease: Mutex<Option<omm_platform::helper::Lease>>,
    pub ui_attached: AtomicBool,
}
fn err(e: impl std::fmt::Display) -> RpcError {
    RpcError::new(ErrorCode::HelperUnavailable, e.to_string())
}

pub async fn open(state: &State) -> Result<Value, RpcError> {
    if !omm_platform::IS_MACOS {
        return Ok(json!({"ok": true}));
    }
    let mut lease = state.session.lease.lock().await;
    if let Some(existing) = lease.as_mut() {
        if existing.alive().await {
            return Ok(json!({"ok":true}));
        }
        *lease = None;
    }
    *lease = Some(omm_platform::helper::Lease::acquire().await.map_err(err)?);
    Ok(json!({"ok":true}))
}
pub async fn start(state: &Arc<State>) -> Result<Value, RpcError> {
    if !omm_platform::IS_MACOS {
        return service("start").await;
    }
    open(state).await?;
    state.config.apply(state).await?;
    let yaml = tokio::fs::read_to_string(state.config.paths().runtime_config())
        .await
        .map_err(err)?;
    let result = omm_platform::helper::call("core.start", json!({"yaml":yaml}))
        .await
        .map_err(err)?;
    state.request_refresh();
    Ok(result)
}
pub async fn stop(state: &State) -> Result<Value, RpcError> {
    if !omm_platform::IS_MACOS {
        return service("stop").await;
    }
    // Stop is idempotent, including a dead helper. Dropping the lease is the fallback.
    let result = omm_platform::helper::call("core.stop", Value::Null).await;
    *state.session.lease.lock().await = None;
    state.request_refresh();
    result.map_err(err)
}
pub async fn command(state: &Arc<State>, verb: &str) -> Result<Value, RpcError> {
    if !omm_platform::IS_MACOS {
        return service(verb).await;
    }
    match verb {
        "start" => start(state).await,
        "stop" => stop(state).await,
        "restart" => {
            stop(state).await?;
            start(state).await
        }
        _ => Err(RpcError::new(
            ErrorCode::BadRequest,
            "unknown core operation",
        )),
    }
}
async fn service(verb: &str) -> Result<Value, RpcError> {
    let out = tokio::process::Command::new("systemctl")
        .args(["--user", verb, omm_config::host::CORE_UNIT])
        .output()
        .await
        .map_err(err)?;
    if !out.status.success() {
        return Err(RpcError::new(
            ErrorCode::Core,
            String::from_utf8_lossy(&out.stderr),
        ));
    }
    Ok(json!({"ok":true}))
}
pub async fn detached(state: &State) {
    if omm_platform::IS_MACOS {
        if let Err(e) = stop(state).await {
            tracing::warn!("ending UI session: {e}");
        }
    }
    state.session.ui_attached.store(false, Ordering::Release);
}
