//! The `autofix.*` and `automation.*` methods.

use super::*;

pub async fn rpc(
    state: &Arc<State>,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, RpcError> {
    #[derive(Deserialize)]
    struct HostParam {
        host: String,
        target: Option<String>,
    }
    match method {
        "automation.get" => Ok(json!(state.autofix.lock().unwrap().settings)),
        "automation.set" => {
            let s: Automation = serde_json::from_value(params)
                .map_err(|e| RpcError::new(ErrorCode::BadRequest, e.to_string()))?;
            let saved = state.autofix.lock().unwrap().set_settings(s)?;
            Ok(json!(saved))
        }
        "autofix.list" => Ok(json!(state.autofix.lock().unwrap().list())),
        _ => {
            let p: HostParam = serde_json::from_value(params)
                .map_err(|e| RpcError::new(ErrorCode::BadRequest, e.to_string()))?;
            let host = p.host.trim().to_ascii_lowercase();
            let fix = state.autofix.lock().unwrap().get(&host);
            let t = now();
            let quiet = |state: &State, until: i64| {
                let fix = state.autofix.lock().unwrap().update(&host, t, |f| {
                    f.state = FixState::Dismissed;
                    f.until = Some(until);
                });
                if let Some(f) = &fix {
                    publish(state, f);
                }
                fix
            };
            match method {
                "autofix.get" => Ok(json!(fix)),
                "autofix.apply" => {
                    let fix = fix.ok_or_else(|| {
                        RpcError::new(ErrorCode::NotFound, format!("nothing known about {host}"))
                    })?;
                    let target = p.target.or(fix.target.clone()).ok_or_else(|| {
                        RpcError::new(ErrorCode::BadRequest, "no working route to apply")
                    })?;
                    if fix.state == FixState::Trial
                        && let Some(id) = fix.rule_id
                        && fix.target.as_deref() != Some(target.as_str())
                    {
                        delete_rule(state, id).await?;
                    }
                    Ok(json!(start_trial(state, &host, &target, false).await?))
                }
                "autofix.pin" => Ok(json!(pin(state, &host).await?)),
                // After a revert: keep the detour anyway, for good.
                "autofix.keep" => {
                    let fix =
                        fix.ok_or_else(|| RpcError::new(ErrorCode::NotFound, host.clone()))?;
                    let target = p
                        .target
                        .or(fix.target)
                        .ok_or_else(|| RpcError::new(ErrorCode::BadRequest, "no route to keep"))?;
                    start_trial(state, &host, &target, false).await?;
                    Ok(json!(pin(state, &host).await?))
                }
                "autofix.undo" => {
                    let fix =
                        fix.ok_or_else(|| RpcError::new(ErrorCode::NotFound, host.clone()))?;
                    if let Some(id) = fix.rule_id {
                        delete_rule(state, id).await?;
                    }
                    let fix = state.autofix.lock().unwrap().update(&host, t, |f| {
                        f.rule_id = None;
                        f.state = FixState::Dismissed;
                        f.until = Some(t + QUIET_MS);
                    });
                    if let Some(f) = &fix {
                        publish(state, f);
                    }
                    Ok(json!(fix))
                }
                "autofix.dismiss" => Ok(json!(quiet(state, t + QUIET_MS))),
                "autofix.recheck" => {
                    {
                        let mut a = state.autofix.lock().unwrap();
                        let f = a
                            .fixes
                            .get_mut(&host)
                            .ok_or_else(|| RpcError::new(ErrorCode::NotFound, host.clone()))?;
                        if f.state == FixState::Trial {
                            return Err(RpcError::new(
                                ErrorCode::BadRequest,
                                "a route is on trial",
                            ));
                        }
                        f.state = FixState::Checking;
                        f.tried.clear();
                    }
                    check(state.clone(), host.clone()).await;
                    Ok(json!(state.autofix.lock().unwrap().get(&host)))
                }
                "autofix.exclude" => {
                    if let Some(id) = fix
                        .as_ref()
                        .filter(|f| f.state == FixState::Trial)
                        .and_then(|f| f.rule_id)
                    {
                        delete_rule(state, id).await?;
                    }
                    let mut a = state.autofix.lock().unwrap();
                    let mut s = a.settings.clone();
                    if !s.ignore.contains(&host) {
                        s.ignore.push(host.clone());
                    }
                    a.set_settings(s)?;
                    a.fixes.remove(&host);
                    a.save();
                    Ok(json!({ "ok": true }))
                }
                _ => Err(RpcError::new(
                    ErrorCode::UnknownMethod,
                    format!("unknown method {method}"),
                )),
            }
        }
    }
}
