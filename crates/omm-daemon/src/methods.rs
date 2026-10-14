//! RPC method table.

use std::sync::Arc;

use mihomo_api::model::{Mode, Proxies};
use omm_config::rules::{MatchKind, UserRule};
use omm_proto::{ErrorCode, ProxyGroup, ProxyMember, RpcError};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::state::State;

pub const CORE_UNIT: &str = "omarchy-mihomo-core.service";

/// URL used for latency tests unless the caller names one.
const DEFAULT_TEST_URL: &str = "https://www.gstatic.com/generate_204";
const DEFAULT_TEST_TIMEOUT_MS: u32 = 5000;

type Reply = Result<Value, RpcError>;

pub async fn dispatch(state: &Arc<State>, method: &str, params: Value) -> Reply {
    match method {
        "ping" => Ok(json!({ "pong": true })),
        "status" => to_value(state.status()),

        "mode.set" => {
            #[derive(Deserialize)]
            struct P {
                mode: Mode,
            }
            let p: P = parse(params)?;
            state.core.patch_configs(json!({ "mode": p.mode })).await.map_err(core_err)?;
            let mode = crate::core::mode_name(p.mode).to_owned();
            state.config.persist(|s| s.mode = mode).await?;
            refreshed(state)
        }
        "tun.set" => {
            #[derive(Deserialize)]
            struct P {
                enable: bool,
            }
            let p: P = parse(params)?;
            state.core.patch_configs(json!({ "tun": { "enable": p.enable } })).await.map_err(core_err)?;
            state.config.persist(|s| s.tun.enable = p.enable).await?;
            refreshed(state)
        }

        "proxies.list" => {
            let proxies = state.core.proxies().await.map_err(core_err)?;
            to_value(groups(&proxies))
        }
        "proxies.targets" => {
            let proxies = state.core.proxies().await.map_err(core_err)?;
            to_value(targets(&proxies))
        }
        "proxies.select" => {
            #[derive(Deserialize)]
            struct P {
                group: String,
                name: String,
            }
            let p: P = parse(params)?;
            state.core.select(&p.group, &p.name).await.map_err(core_err)?;
            Ok(json!({ "ok": true }))
        }
        "proxies.delay" => {
            let p: DelayParams = parse(params)?;
            match state.core.proxy_delay(&p.name, p.url(), p.timeout()).await {
                Ok(d) => Ok(json!({ "delay": d.delay })),
                // Timeouts and dial failures are results, not RPC errors.
                Err(mihomo_api::Error::Status { status: 503 | 504, message }) => Ok(json!({ "delay": 0, "error": message })),
                Err(e) => Err(core_err(e)),
            }
        }
        "group.delay" => {
            let p: DelayParams = parse(params)?;
            let r = state.core.group_delay(&p.name, p.url(), p.timeout()).await.map_err(core_err)?;
            to_value(r)
        }

        "connections.list" => to_value(state.core.connections().await.map_err(core_err)?),
        "connections.close" => {
            #[derive(Deserialize)]
            struct P {
                id: Option<String>,
            }
            let p: P = parse(params)?;
            match p.id {
                Some(id) => state.core.close_connection(&id).await,
                None => state.core.close_all_connections().await,
            }
            .map_err(core_err)?;
            Ok(json!({ "ok": true }))
        }

        "config.apply" => to_value(state.config.apply(state).await?),
        "config.snapshots" => to_value(state.config.snapshots()?),
        "config.rollback" => {
            #[derive(Deserialize)]
            struct P {
                name: Option<String>,
            }
            let p: P = parse(params)?;
            state.config.rollback(state, p.name).await
        }
        "settings.get" => to_value(state.config.settings().await),
        "settings.set" => {
            if !params.is_object() {
                return Err(RpcError::new(ErrorCode::BadRequest, "settings.set expects an object"));
            }
            to_value(state.config.set_settings(state, params).await?)
        }

        "profiles.list" => Ok(state.config.list().await),
        "profiles.add" => {
            #[derive(Deserialize)]
            struct P {
                url: Option<String>,
                name: Option<String>,
                user_agent: Option<String>,
                /// Local profile: YAML text or a file to read.
                content: Option<String>,
                path: Option<std::path::PathBuf>,
            }
            let p: P = parse(params)?;
            match (p.url, p.content, p.path) {
                (Some(url), None, None) => state.config.add_remote(state, url, p.name, p.user_agent).await,
                (None, content, path) if content.is_some() != path.is_some() => {
                    let text = match (content, &path) {
                        (Some(c), _) => c,
                        (None, Some(path)) => std::fs::read_to_string(path).map_err(|e| RpcError::new(ErrorCode::BadRequest, format!("{}: {e}", path.display())))?,
                        _ => unreachable!(),
                    };
                    let name = p.name.or_else(|| path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned())).unwrap_or_else(|| "local".into());
                    state.config.add_local(state, name, text).await
                }
                _ => Err(RpcError::new(ErrorCode::BadRequest, "give exactly one of url, content, path")),
            }
        }
        "profiles.update" => {
            #[derive(Deserialize)]
            struct P {
                id: Option<String>,
                #[serde(default)]
                force: bool,
            }
            let p: P = parse(params)?;
            to_value(state.config.update(state, p.id, p.force).await)
        }
        "profiles.select" => to_value(state.config.select(state, &parse::<IdParam>(params)?.id).await?),
        "profiles.remove" => state.config.remove(state, &parse::<IdParam>(params)?.id).await,

        "problems.list" => {
            let query: omm_problems::Query = parse(params)?;
            let c = state.problems.lock().unwrap();
            let (apps, targets) = c.facets();
            Ok(json!({ "items": c.list(&query), "counts": c.counts(), "apps": apps, "targets": targets }))
        }
        "problems.get" => {
            #[derive(Deserialize)]
            struct P {
                host: String,
            }
            let p: P = parse(params)?;
            let c = state.problems.lock().unwrap();
            let view = c.get(&p.host).ok_or_else(|| RpcError::new(ErrorCode::NotFound, format!("no problem for {}", p.host)))?;
            to_value(view)
        }
        "problems.mark_seen" => {
            state.problems.lock().unwrap().mark_seen(crate::problems::now_ms()).map_err(db_err)?;
            crate::problems::announce(state);
            Ok(json!({ "ok": true }))
        }
        "problems.hide" => {
            #[derive(Deserialize)]
            struct P {
                kind: omm_problems::IgnoreKind,
                value: String,
            }
            let p: P = parse(params)?;
            if p.value.trim().is_empty() {
                return Err(RpcError::new(ErrorCode::BadRequest, "value is empty"));
            }
            let ignore = state.problems.lock().unwrap().hide(p.kind, &p.value, crate::problems::now_ms()).map_err(db_err)?;
            crate::problems::announce(state);
            to_value(ignore)
        }
        "problems.unhide" => {
            #[derive(Deserialize)]
            struct P {
                id: i64,
            }
            let p: P = parse(params)?;
            let removed = state.problems.lock().unwrap().unhide(p.id).map_err(db_err)?;
            crate::problems::announce(state);
            Ok(json!({ "removed": removed }))
        }
        "problems.ignores" => to_value(state.problems.lock().unwrap().ignores()),
        "problems.clear" => {
            #[derive(Deserialize)]
            struct P {
                host: Option<String>,
            }
            let p: P = parse(params)?;
            let n = state.problems.lock().unwrap().clear(p.host.as_deref()).map_err(db_err)?;
            crate::problems::announce(state);
            Ok(json!({ "removed": n }))
        }

        "rules.list" => {
            let (rules, orphans) = state.config.rules().await?;
            // Hit counters from the core, for rules it has loaded.
            let hits: std::collections::HashMap<(String, String, String), u64> = match state.core.rules().await {
                Ok(r) => r.rules.into_iter().map(|x| ((x.kind.clone(), x.payload.clone(), x.proxy.clone()), x.hits())).collect(),
                Err(_) => Default::default(),
            };
            let items: Vec<Value> = rules
                .into_iter()
                .map(|s| {
                    let key = (s.rule.core_type().to_owned(), s.rule.value.clone(), s.rule.target.clone());
                    let mut v = serde_json::to_value(&s).unwrap_or_default();
                    v["orphan"] = json!(orphans.contains(&s.rule.id));
                    v["line"] = json!(s.rule.line());
                    v["hits"] = json!(hits.get(&key));
                    v
                })
                .collect();
            Ok(json!({ "items": items, "orphans": orphans }))
        }
        "rules.preview" => {
            let p: RuleInput = parse(params)?;
            let value = normalize(p.kind, &p.value)?;
            let rule = UserRule { id: 0, kind: p.kind, value: value.clone(), target: p.target.unwrap_or_else(|| "DIRECT".into()), enabled: true };
            let (rules, _) = state.config.rules().await?;
            let existing = rules.iter().find(|s| s.rule.kind == p.kind && s.rule.value == value);
            Ok(json!({ "value": value, "line": rule.line(), "existing": existing }))
        }
        "rules.test" => {
            #[derive(Deserialize)]
            struct P {
                host: String,
                port: Option<u16>,
                targets: Vec<String>,
            }
            let p: P = parse(params)?;
            if p.targets.is_empty() || p.targets.len() > 32 {
                return Err(RpcError::new(ErrorCode::BadRequest, "give 1 to 32 targets"));
            }
            let probe = crate::route_check::probe_for(p.host.trim(), p.port);
            let results = crate::route_check::check(state, &probe, &p.targets).await;
            Ok(json!({ "probe": probe, "results": results }))
        }
        "rules.save" => {
            #[derive(Deserialize)]
            struct P {
                #[serde(flatten)]
                input: RuleInput,
                note: Option<String>,
                test: Option<crate::user_rules::Check>,
                #[serde(default = "user_source")]
                source: String,
            }
            let p: P = parse(params)?;
            let value = normalize(p.input.kind, &p.input.value)?;
            let target = p.input.target.filter(|t| !t.trim().is_empty()).ok_or_else(|| RpcError::new(ErrorCode::BadRequest, "target is required"))?;
            let new = crate::user_rules::NewRule { kind: p.input.kind, value, target, note: p.note, source: p.source, test: p.test };
            let now = crate::problems::now_ms();
            let (stored, report) = state
                .config
                .change_rules(state, true, |store| {
                    let (stored, change) = store.upsert(new, now).map_err(crate::config::db_err)?;
                    Ok((stored, vec![change]))
                })
                .await?;
            let closed = if report.reloaded { crate::route_check::close_matching(state, &stored.rule).await } else { 0 };
            Ok(json!({ "rule": stored, "apply": report, "closed": closed }))
        }
        "rules.update" => {
            #[derive(Deserialize)]
            struct P {
                id: i64,
                #[serde(flatten)]
                patch: crate::user_rules::Patch,
            }
            let p: P = parse(params)?;
            let now = crate::problems::now_ms();
            let (stored, report) = state
                .config
                .change_rules(state, true, |store| match store.update(p.id, p.patch, now).map_err(crate::config::db_err)? {
                    Some((s, c)) => Ok((s, vec![c])),
                    None => Err(RpcError::new(ErrorCode::NotFound, format!("no rule #{}", p.id))),
                })
                .await?;
            let closed = if report.reloaded { crate::route_check::close_matching(state, &stored.rule).await } else { 0 };
            Ok(json!({ "rule": stored, "apply": report, "closed": closed }))
        }
        "rules.delete" => {
            let id = parse::<NumId>(params)?.id;
            let now = crate::problems::now_ms();
            let (rule, report) = state
                .config
                .change_rules(state, false, |store| match store.delete(id, now).map_err(crate::config::db_err)? {
                    Some(c) => Ok((c.before.clone(), vec![c])),
                    None => Err(RpcError::new(ErrorCode::NotFound, format!("no rule #{id}"))),
                })
                .await?;
            let closed = match (&rule, report.reloaded) {
                (Some(r), true) => crate::route_check::close_matching(state, &r.rule).await,
                _ => 0,
            };
            Ok(json!({ "apply": report, "closed": closed }))
        }
        "rules.history" => {
            #[derive(Deserialize)]
            struct P {
                limit: Option<usize>,
            }
            let p: P = parse(params)?;
            to_value(state.config.rule_history(p.limit.unwrap_or(50).min(500)).await?)
        }
        "rules.rollback" => {
            #[derive(Deserialize)]
            struct P {
                /// Undo every change after this one; 0 undoes all.
                to: i64,
            }
            let p: P = parse(params)?;
            let now = crate::problems::now_ms();
            let (undone, report) = state
                .config
                .change_rules(state, false, |store| {
                    let undone = store.rollback_to(p.to, now).map_err(crate::config::db_err)?;
                    Ok((undone.len(), undone))
                })
                .await?;
            Ok(json!({ "undone": undone, "apply": report }))
        }

        "core.start" | "core.stop" | "core.restart" => {
            let verb = method.trim_start_matches("core.");
            let out = tokio::process::Command::new("systemctl")
                .args(["--user", verb, CORE_UNIT])
                .output()
                .await
                .map_err(|e| RpcError::new(ErrorCode::Internal, e.to_string()))?;
            if !out.status.success() {
                return Err(RpcError::new(ErrorCode::Core, String::from_utf8_lossy(&out.stderr).trim().to_owned()));
            }
            state.request_refresh();
            Ok(json!({ "ok": true }))
        }

        "core.rules" => to_value(state.core.rules().await.map_err(core_err)?),

        _ => Err(RpcError::new(ErrorCode::UnknownMethod, format!("unknown method {method}"))),
    }
}

#[derive(Deserialize)]
struct RuleInput {
    kind: MatchKind,
    value: String,
    target: Option<String>,
}

#[derive(Deserialize)]
struct NumId {
    id: i64,
}

fn user_source() -> String {
    "user".into()
}

fn normalize(kind: MatchKind, value: &str) -> Result<String, RpcError> {
    omm_config::rules::normalize(kind, value).map_err(|e| RpcError::new(ErrorCode::BadRequest, e.to_string()))
}

#[derive(Deserialize)]
struct IdParam {
    id: String,
}

#[derive(Deserialize)]
struct DelayParams {
    /// Proxy or group name.
    #[serde(alias = "group")]
    name: String,
    url: Option<String>,
    timeout: Option<u32>,
}

impl DelayParams {
    fn url(&self) -> &str {
        self.url.as_deref().unwrap_or(DEFAULT_TEST_URL)
    }
    fn timeout(&self) -> u32 {
        self.timeout.unwrap_or(DEFAULT_TEST_TIMEOUT_MS).clamp(500, 30_000)
    }
}

/// Groups in config order. mihomo keeps that order in `GLOBAL.all`;
/// `GLOBAL` itself goes last, as dashboards show it.
pub fn groups(p: &Proxies) -> Vec<ProxyGroup> {
    let order = p.proxies.get("GLOBAL").and_then(|g| g.all.clone()).unwrap_or_default();
    let mut names: Vec<&str> = order
        .iter()
        .map(String::as_str)
        .filter(|n| p.proxies.get(*n).is_some_and(|x| x.is_group()))
        .collect();
    // Groups hidden from GLOBAL (or a core without it) still belong in the list.
    for (name, proxy) in &p.proxies {
        if proxy.is_group() && name != "GLOBAL" && !names.contains(&name.as_str()) {
            names.push(name);
        }
    }
    if p.proxies.contains_key("GLOBAL") {
        names.push("GLOBAL");
    }

    names
        .into_iter()
        .filter_map(|n| p.proxies.get(n))
        .map(|g| ProxyGroup {
            name: g.name.clone(),
            kind: g.kind.clone(),
            now: g.now.clone(),
            members: g
                .all
                .iter()
                .flatten()
                .map(|m| {
                    let px = p.proxies.get(m);
                    ProxyMember {
                        name: m.clone(),
                        kind: px.map(|x| x.kind.clone()).unwrap_or_default(),
                        delay: px.and_then(|x| x.last_delay()),
                        alive: px.and_then(|x| x.alive),
                    }
                })
                .collect(),
        })
        .collect()
}

/// Valid targets for a user rule: `DIRECT` plus every group except `GLOBAL`.
pub fn targets(p: &Proxies) -> Vec<String> {
    std::iter::once("DIRECT".to_owned())
        .chain(groups(p).into_iter().map(|g| g.name).filter(|n| n != "GLOBAL"))
        .collect()
}

fn refreshed(state: &State) -> Reply {
    state.request_refresh();
    Ok(json!({ "ok": true }))
}

fn parse<T: DeserializeOwned>(params: Value) -> Result<T, RpcError> {
    let params = if params.is_null() { json!({}) } else { params };
    serde_json::from_value(params).map_err(|e| RpcError::new(ErrorCode::BadRequest, e.to_string()))
}

fn to_value(v: impl serde::Serialize) -> Reply {
    serde_json::to_value(v).map_err(|e| RpcError::new(ErrorCode::Internal, e.to_string()))
}

fn db_err(e: impl std::fmt::Display) -> RpcError {
    RpcError::new(ErrorCode::Internal, format!("database: {e}"))
}

fn core_err(e: mihomo_api::Error) -> RpcError {
    let code = match &e {
        mihomo_api::Error::Connect { .. } => ErrorCode::CoreUnavailable,
        mihomo_api::Error::Status { status: 404, .. } => ErrorCode::NotFound,
        _ => ErrorCode::Core,
    };
    RpcError::new(code, e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Proxies {
        serde_json::from_value(json!({"proxies": {
            "GLOBAL": {"name": "GLOBAL", "type": "Selector", "now": "DIRECT", "all": ["DIRECT", "PROXY", "AI", "FI"]},
            "AI": {"name": "AI", "type": "Selector", "now": "FI", "all": ["FI"]},
            "PROXY": {"name": "PROXY", "type": "Fallback", "now": "FI", "all": ["FI"]},
            "Hidden": {"name": "Hidden", "type": "URLTest", "now": "FI", "all": ["FI"]},
            "FI": {"name": "FI", "type": "Vless", "history": [{"time": "t", "delay": 212}]},
            "DIRECT": {"name": "DIRECT", "type": "Direct"}
        }}))
        .unwrap()
    }

    #[test]
    fn groups_follow_global_order() {
        let names: Vec<_> = groups(&sample()).into_iter().map(|g| g.name).collect();
        assert_eq!(names, ["PROXY", "AI", "Hidden", "GLOBAL"]);
    }

    #[test]
    fn member_delay_comes_from_history() {
        let g = groups(&sample()).into_iter().find(|g| g.name == "AI").unwrap();
        assert_eq!(g.members[0].delay, Some(212));
        assert_eq!(g.members[0].kind, "Vless");
    }

    #[test]
    fn targets_exclude_global() {
        assert_eq!(targets(&sample()), ["DIRECT", "PROXY", "AI", "Hidden"]);
    }
}
