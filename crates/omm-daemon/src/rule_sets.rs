//! The user's rule sets: listing them beside the profile's providers,
//! adding (by URL or from the catalog), removing and refreshing.

use std::sync::Arc;
use std::time::Duration;

use omm_config::rule_sets::{RuleSet, catalog};
use omm_config::rules::MatchKind;
use omm_proto::{ErrorCode, RpcError};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::state::State;

fn bad(m: impl Into<String>) -> RpcError {
    RpcError::new(ErrorCode::BadRequest, m)
}

/// The user's rules that name `set`.
async fn users_of(state: &State, set: &str) -> Vec<String> {
    let rules = state.config.rules().await.map(|(r, _)| r).unwrap_or_default();
    let needle = format!("RULE-SET,{set}");
    rules
        .into_iter()
        .filter(|s| match s.rule.kind {
            MatchKind::RuleSet => s.rule.value == set,
            k if k.is_logic() => s.rule.value.contains(&needle),
            _ => false,
        })
        .map(|s| s.rule.line())
        .collect()
}

/// Refuse a URL that does not answer before the core chokes on it.
async fn reachable(state: &State, url: &str) -> Result<(), RpcError> {
    let mut last = String::new();
    for via in [None, crate::config::proxy_of(state)] {
        let mut b = reqwest::Client::builder().timeout(Duration::from_secs(20)).no_proxy();
        if let Some(p) = &via {
            match reqwest::Proxy::all(p) {
                Ok(p) => b = b.proxy(p),
                Err(_) => continue,
            }
        }
        let Ok(client) = b.build() else { continue };
        match client.head(url).send().await {
            Ok(r) if r.status().is_success() => return Ok(()),
            Ok(r) => last = format!("HTTP {}", r.status()),
            Err(e) => last = e.without_url().to_string(),
        }
        if via.is_none() && last.starts_with("HTTP 404") {
            break;
        }
    }
    Err(RpcError::new(ErrorCode::Fetch, format!("{url}: {last}")))
}

async fn save(state: &Arc<State>, sets: Vec<RuleSet>) -> Result<(), RpcError> {
    let patch = json!({ "rule-sets": serde_json::to_value(sets).map_err(|e| RpcError::new(ErrorCode::Internal, e.to_string()))? });
    state.config.set_settings(state, patch).await.map(drop)
}

async fn list(state: &State) -> Value {
    let mine = state.config.settings().await.rule_sets;
    let runtime = state.core.rule_providers().await.map(|p| p.providers).unwrap_or_default();
    let info = |name: &str| runtime.get(name).map(|p| json!({ "rule_count": p.rule_count, "updated_at": p.updated_at, "behavior": p.behavior.to_lowercase() }));
    let mine_names: Vec<&str> = mine.iter().map(|s| s.name.as_str()).collect();
    let profile: Vec<Value> = runtime
        .values()
        .filter(|p| !mine_names.contains(&p.name.as_str()))
        .map(|p| json!({ "name": p.name, "behavior": p.behavior.to_lowercase(), "rule_count": p.rule_count, "updated_at": p.updated_at, "vehicle": p.vehicle_type }))
        .collect();
    let mine: Vec<Value> = mine
        .iter()
        .map(|s| {
            let mut v = serde_json::to_value(s).unwrap_or_default();
            v["runtime"] = info(&s.name).unwrap_or(Value::Null);
            v
        })
        .collect();
    json!({ "mine": mine, "profile": profile })
}

pub async fn rpc(state: &Arc<State>, method: &str, params: Value) -> Result<Value, RpcError> {
    match method {
        "rule_sets.list" => {}
        // `{catalog: "geosite:openai"}` or a whole set.
        "rule_sets.add" => {
            #[derive(Deserialize)]
            struct P {
                catalog: Option<String>,
                #[serde(flatten)]
                set: Option<RuleSet>,
            }
            let p: P = serde_json::from_value(params).map_err(|e| bad(e.to_string()))?;
            let set = match (p.catalog, p.set) {
                (Some(spec), _) => catalog(&spec).ok_or_else(|| bad(format!("not a catalog name: {spec}")))?,
                (None, Some(set)) => set,
                (None, None) => return Err(bad("a catalog name or a rule set is required")),
            };
            set.validate().map_err(|e| bad(e.to_string()))?;
            let mut sets = state.config.settings().await.rule_sets;
            let taken = state.core.rule_providers().await.map(|p| p.providers.contains_key(&set.name)).unwrap_or(false);
            let ours = sets.iter().any(|s| s.name == set.name);
            if ours {
                // Picking a catalog entry twice is not an error.
                return Ok(json!({ "name": set.name, "list": list(state).await }));
            }
            if taken {
                return Err(RpcError::new(ErrorCode::Conflict, format!("the profile already has a rule set named {}", set.name)));
            }
            reachable(state, &set.url).await?;
            let name = set.name.clone();
            sets.push(set);
            save(state, sets).await?;
            return Ok(json!({ "name": name, "list": list(state).await }));
        }
        "rule_sets.remove" => {
            #[derive(Deserialize)]
            struct P {
                name: String,
            }
            let name = serde_json::from_value::<P>(params).map_err(|e| bad(e.to_string()))?.name;
            let users = users_of(state, &name).await;
            if !users.is_empty() {
                return Err(RpcError::new(ErrorCode::Conflict, format!("rules still use {name}: {}", users.join("; "))));
            }
            let mut sets = state.config.settings().await.rule_sets;
            let before = sets.len();
            sets.retain(|s| s.name != name);
            if sets.len() == before {
                return Err(RpcError::new(ErrorCode::NotFound, format!("no rule set of yours named {name}")));
            }
            save(state, sets).await?;
        }
        "rule_sets.refresh" => {
            #[derive(Deserialize)]
            struct P {
                name: String,
            }
            let name = serde_json::from_value::<P>(params).map_err(|e| bad(e.to_string()))?.name;
            state.core.update_rule_provider(&name).await.map_err(|e| RpcError::new(ErrorCode::Core, e.to_string()))?;
        }
        _ => return Err(RpcError::new(ErrorCode::UnknownMethod, method.to_owned())),
    }
    Ok(list(state).await)
}
