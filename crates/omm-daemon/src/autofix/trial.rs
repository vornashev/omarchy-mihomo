//! A route on trial: starting it, rolling it back, pinning it, and the
//! loop that decides when its time is up.

use super::*;

/// Add the rule for `target` with a trial expiry.
/// `announce`: send a notice too (not when the user asked for it).
pub async fn start_trial(
    state: &State,
    host: &str,
    target: &str,
    announce: bool,
) -> Result<Fix, RpcError> {
    let trial = state.autofix.lock().unwrap().settings.trial_ms();
    let fix =
        state.autofix.lock().unwrap().get(host).ok_or_else(|| {
            RpcError::new(ErrorCode::NotFound, format!("nothing known about {host}"))
        })?;
    let t = now();
    let (kind, raw) = rule_for(host);
    let value = omm_config::rules::normalize(kind, &raw)
        .map_err(|e| RpcError::new(ErrorCode::BadRequest, e.to_string()))?;
    let measured = fix.results.iter().find(|o| o.target == target);
    let new = crate::user_rules::NewRule {
        kind,
        value,
        target: target.to_owned(),
        note: None,
        source: SOURCE.into(),
        test: measured.map(|o| crate::user_rules::Check {
            target: o.target.clone(),
            ok: o.ok,
            delay: o.delay,
            error: o.error.clone(),
            at: fix.checked_at.unwrap_or(t),
        }),
        lifetime: crate::user_rules::Lifetime {
            expires: Some(t + trial + GRACE_MS),
            until_reboot: false,
        },
    };
    let (stored, report) = state
        .config
        .change_rules(state, true, |store| {
            let (s, c) = store.upsert(new, t).map_err(crate::config::db_err)?;
            Ok((s, vec![c]))
        })
        .await?;
    if report.reloaded {
        crate::route_check::close_matching(state, &stored.rule).await;
    }
    let fix = state
        .autofix
        .lock()
        .unwrap()
        .update(host, t, |f| {
            f.state = FixState::Trial;
            f.target = Some(target.to_owned());
            f.rule_id = Some(stored.rule.id);
            f.rule = Some(stored.rule.line());
            f.applied_at = Some(t);
            f.ends_at = Some(t + trial);
            f.trial_ok = 0;
            f.trial_failed = 0;
            f.until = None;
            f.error = None;
        })
        .ok_or_else(|| RpcError::new(ErrorCode::NotFound, host.to_owned()))?;
    publish(state, &fix);
    if announce {
        notice(state, &fix);
    }
    Ok(fix)
}

pub(super) async fn delete_rule(state: &State, id: i64) -> Result<(), RpcError> {
    let t = now();
    let (rule, report) = state
        .config
        .change_rules(state, false, |store| {
            let before = store.get(id).map_err(crate::config::db_err)?;
            let change = store.delete(id, t).map_err(crate::config::db_err)?;
            Ok((before, change.into_iter().collect()))
        })
        .await?;
    if report.reloaded
        && let Some(rule) = rule
    {
        crate::route_check::close_matching(state, &rule.rule).await;
    }
    Ok(())
}

/// The trial route failed too: remove the rule, look for another route.
pub async fn roll_back(state: Arc<State>, host: String) {
    let Some(fix) = state.autofix.lock().unwrap().get(&host) else {
        return;
    };
    if fix.state != FixState::Trial {
        return;
    }
    if let Some(id) = fix.rule_id
        && let Err(e) = delete_rule(&state, id).await
    {
        tracing::warn!(host, "autofix: removing rule: {}", e.message);
    }
    let failed = fix.target.clone().unwrap_or_default();
    tracing::info!(host, failed, "autofix: trial failed, rule removed");
    let settings = state.autofix.lock().unwrap().settings.clone();
    let groups = state
        .core
        .proxies()
        .await
        .map(|p| crate::methods::targets(&p))
        .unwrap_or_default();
    let mut tried = fix.tried.clone();
    tried.push(failed.clone());
    let targets: Vec<String> = groups
        .into_iter()
        .filter(|g| settings.usable(g) && *g != fix.was && !tried.contains(g))
        .take(32)
        .collect();
    let mut results = if targets.is_empty() {
        Vec::new()
    } else {
        crate::route_check::check(
            &state,
            &crate::route_check::probe_for(&host, fix.port),
            &targets,
        )
        .await
    };
    results.sort_by_key(|o| (!o.ok, o.delay.unwrap_or(u32::MAX)));
    let next = results.iter().find(|o| o.ok).map(|o| o.target.clone());
    let t = now();
    let fix = state.autofix.lock().unwrap().update(&host, t, |f| {
        f.state = FixState::RolledBack;
        f.tried = tried;
        f.rule_id = None;
        f.results = results;
        f.checked_at = Some(t);
        f.target = next;
        f.until = Some(t + QUIET_MS);
    });
    if let Some(fix) = fix {
        publish(&state, &fix);
        notice(&state, &fix);
    }
}

pub(super) async fn pin(state: &State, host: &str) -> Result<Fix, RpcError> {
    let fix = state
        .autofix
        .lock()
        .unwrap()
        .get(host)
        .ok_or_else(|| RpcError::new(ErrorCode::NotFound, host.to_owned()))?;
    let id = fix
        .rule_id
        .ok_or_else(|| RpcError::new(ErrorCode::BadRequest, "no rule to pin"))?;
    let t = now();
    state
        .config
        .change_rules(state, false, |store| {
            match store
                .update(
                    id,
                    crate::user_rules::Patch {
                        lifetime: Some(Default::default()),
                        ..Default::default()
                    },
                    t,
                )
                .map_err(crate::config::db_err)?
            {
                Some((_, c)) => Ok(((), vec![c])),
                None => Err(RpcError::new(ErrorCode::NotFound, format!("no rule #{id}"))),
            }
        })
        .await?;
    let fix = state
        .autofix
        .lock()
        .unwrap()
        .update(host, t, |f| {
            f.state = FixState::Pinned;
            f.ends_at = Some(t);
        })
        .ok_or_else(|| RpcError::new(ErrorCode::NotFound, host.to_owned()))?;
    publish(state, &fix);
    Ok(fix)
}

/// Before keeping a fix for good, try the route it replaced: a short outage
/// must not leave a permanent detour. Returns true when the rule was removed.
pub(super) async fn revert_if_recovered(state: &State, fix: &Fix) -> bool {
    if fix.was.is_empty() {
        return false;
    }
    let probe = crate::route_check::probe_for(&fix.host, fix.port);
    let was = crate::route_check::check(state, &probe, std::slice::from_ref(&fix.was)).await;
    if !was.first().is_some_and(|o| o.ok) {
        return false;
    }
    if let Some(id) = fix.rule_id
        && let Err(e) = delete_rule(state, id).await
    {
        tracing::warn!(host = fix.host, "autofix: removing rule: {}", e.message);
        return false;
    }
    tracing::info!(
        host = fix.host,
        was = fix.was,
        "autofix: original route works again, rule removed"
    );
    let t = now();
    let mut results = was;
    results.extend(
        fix.results
            .iter()
            .filter(|o| Some(&o.target) == fix.target.as_ref())
            .cloned(),
    );
    let fix = state.autofix.lock().unwrap().update(&fix.host, t, |f| {
        f.state = FixState::Reverted;
        f.rule_id = None;
        f.results = results;
        f.checked_at = Some(t);
        f.until = Some(t + RECHECK_MS);
    });
    if let Some(fix) = fix {
        publish(state, &fix);
        notice(state, &fix);
    }
    true
}

/// Pin trials that passed; drop fixes whose rule the user removed.
pub async fn watch(state: Arc<State>) {
    let mut tick = tokio::time::interval(Duration::from_secs(30));
    loop {
        tick.tick().await;
        // A route tried right after waking up would fail for no reason.
        if state.network_quiet() {
            continue;
        }
        let t = now();
        let trials: Vec<Fix> = {
            let mut a = state.autofix.lock().unwrap();
            a.sweep(t);
            a.fixes
                .values()
                .filter(|f| f.state == FixState::Trial)
                .cloned()
                .collect()
        };
        if trials.is_empty() {
            continue;
        }
        let Ok((rules, _)) = state.config.rules().await else {
            continue;
        };
        for fix in trials {
            let alive = fix
                .rule_id
                .and_then(|id| rules.iter().find(|s| s.rule.id == id))
                .is_some_and(|s| {
                    s.rule.enabled && s.rule.target == fix.target.clone().unwrap_or_default()
                });
            if !alive {
                // Edited or removed by the user: theirs now.
                state.autofix.lock().unwrap().update(&fix.host, t, |f| {
                    f.state = FixState::Dismissed;
                    f.until = Some(t + QUIET_MS);
                });
                continue;
            }
            if fix.ends_at.is_some_and(|e| e <= t) {
                if revert_if_recovered(&state, &fix).await {
                    continue;
                }
                match pin(&state, &fix.host).await {
                    Ok(fix) => {
                        tracing::info!(host = fix.host, "autofix: trial passed, rule pinned");
                        notice(&state, &fix);
                    }
                    Err(e) => tracing::warn!(host = fix.host, "autofix: pinning: {}", e.message),
                }
            }
        }
    }
}
