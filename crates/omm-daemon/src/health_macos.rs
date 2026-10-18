//! macOS health never auto-starts a network session or modifies installed binaries.
use std::sync::Arc;
use omm_proto::{ErrorCode, Issue, IssueLevel, RpcError};
use serde_json::{Value, json};
use crate::state::State;
#[derive(Debug, Clone, Default)]
pub enum Install { #[default] Idle }
pub async fn check(state: &State) -> Vec<Issue> {
    let result = omm_platform::helper::call("status", Value::Null).await;
    let mut issues = Vec::new();
    let helper = match result {
        Ok(v) => omm_proto::HelperStatus { available:true, session_active:v["running"] == true, error:v["error"].as_str().map(Into::into) },
        Err(e) => omm_proto::HelperStatus { error:Some(e.to_string()), ..Default::default() },
    };
    if !helper.available || helper.error.is_some() {
        issues.push(Issue { id:"helper_unavailable".into(), level:IssueLevel::Error, detail:helper.error.clone(), fix:None });
    }
    state.set_helper_status(helper);
    if !state.config.has_profile().await { issues.push(Issue { id:"no_profile".into(), level:IssueLevel::Warn, detail:Some("Import a profile to begin".into()), fix:None }); }
    if let Some(e) = &state.status().core.tun_issue { issues.push(Issue { id:"tun_failed".into(), level:IssueLevel::Error, detail:e.message.clone(), fix:None }); }
    issues
}
pub async fn run(state: &Arc<State>, id: &str) -> Result<Value, RpcError> {
    match id {
        "start_core" => crate::session::command(state, "start").await,
        "restart_core" => crate::session::command(state, "restart").await,
        _ => Err(RpcError::new(ErrorCode::Unsupported, "Use the macOS installer to install or update the network service")),
    }
}
pub async fn auto(_: Arc<State>) {}
pub async fn start_core_if_free(_: &State) {}
