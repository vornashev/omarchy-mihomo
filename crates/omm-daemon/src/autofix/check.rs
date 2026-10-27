//! Checking a failing host through the profile's groups and picking a route.

use super::*;

/// Check a host through every group and act on the result.
pub async fn check(state: Arc<State>, host: String) {
    let Some(fix) = state.autofix.lock().unwrap().get(&host) else {
        return;
    };
    let settings = state.autofix.lock().unwrap().settings.clone();
    let rules = state
        .config
        .rules()
        .await
        .map(|(r, _)| r)
        .unwrap_or_default();
    if let Some(protect) = protection(&fix, &settings, &rules) {
        tracing::info!(
            host,
            reason = protect.reason,
            by = protect.by,
            "autofix: explicit route, left alone"
        );
        let t = now();
        let fix = state.autofix.lock().unwrap().update(&host, t, |f| {
            f.state = FixState::Protected;
            f.protect = Some(protect);
            f.until = Some(t + RECHECK_MS);
        });
        if let Some(fix) = fix {
            publish(&state, &fix);
        }
        return;
    }
    publish(&state, &fix);
    let mode = settings.mode;
    let proxies = state.core.proxies().await;
    let groups = match &proxies {
        Ok(p) => crate::methods::targets(p)
            .into_iter()
            .filter(|t| settings.usable(t))
            .collect::<Vec<_>>(),
        Err(e) => {
            tracing::debug!("autofix: listing groups: {e}");
            state.autofix.lock().unwrap().fixes.remove(&host);
            return;
        }
    };
    let mut targets: Vec<String> = groups
        .into_iter()
        .filter(|g| *g != fix.was && !fix.tried.contains(g))
        .take(31)
        .collect();
    if !fix.was.is_empty() {
        targets.push(fix.was.clone());
    }
    let probe = crate::route_check::probe_for(&host, fix.port);
    let mut results = crate::route_check::check(&state, &probe, &targets).await;
    // One probe can be lucky: among working routes, a node that lost fewer
    // tests over the last hours wins before a faster one.
    let loss = proxies.as_ref().map(|p| loss_tiers(&state, p, &targets)).unwrap_or_default();
    results.sort_by_key(|o| (!o.ok, o.target == fix.was, loss.get(&o.target).copied().unwrap_or(0), o.delay.unwrap_or(u32::MAX)));
    let t = now();
    let was_ok = results.iter().any(|o| o.ok && o.target == fix.was);
    let best = results
        .iter()
        .find(|o| o.ok && o.target != fix.was)
        .map(|o| o.target.clone());
    tracing::info!(
        host,
        best = best.as_deref().unwrap_or("-"),
        was_ok,
        "autofix checked"
    );

    if was_ok && best.is_none() {
        // The current route works again: a passing outage, nothing to do.
        state.autofix.lock().unwrap().update(&host, t, |f| {
            f.state = FixState::Dismissed;
            f.until = Some(t + RECHECK_MS);
        });
        return;
    }
    let Some(best) = best.filter(|_| !was_ok) else {
        let fix = state.autofix.lock().unwrap().update(&host, t, |f| {
            f.results = results;
            f.checked_at = Some(t);
            f.state = if was_ok { FixState::Dismissed } else { FixState::Unreachable };
            f.until = Some(t + RECHECK_MS);
        });
        if let Some(fix) = fix.filter(|f| f.state == FixState::Unreachable) {
            publish(&state, &fix);
            if mode != Mode::Show {
                notice(&state, &fix);
            }
        }
        return;
    };
    let fix = state.autofix.lock().unwrap().update(&host, t, |f| {
        f.results = results;
        f.checked_at = Some(t);
        f.state = FixState::Found;
        f.target = Some(best.clone());
    });
    let Some(fix) = fix else { return };
    match mode {
        Mode::Trial => {
            if let Err(e) = start_trial(&state, &host, &best, true).await {
                tracing::warn!(host, "autofix: applying {best}: {}", e.message);
                let fix = state.autofix.lock().unwrap().update(&host, now(), |f| f.error = Some(e.message.clone()));
                if let Some(fix) = fix {
                    publish(&state, &fix);
                    notice(&state, &fix);
                }
            }
        }
        Mode::Suggest => {
            publish(&state, &fix);
            notice(&state, &fix);
        }
        Mode::Show => publish(&state, &fix),
    }
}

/// Why autofix must not reroute this traffic, if it must not. Only
/// traffic nobody directed — the final `Match`, or DIRECT by default — is
/// fixed; an explicit decision stays, even while it fails.
pub(super) fn protection(
    fix: &Fix,
    settings: &Automation,
    rules: &[crate::user_rules::Stored],
) -> Option<Protect> {
    if settings.strict_groups.contains(&fix.was) {
        return Some(Protect {
            reason: "strict".into(),
            by: fix.was.clone(),
        });
    }
    // The user's own rule, by any match the log lets us check: the rule the
    // core reported, or the host, process and port of the connection.
    let ip = fix.host.parse::<IpAddr>().ok();
    let conn = omm_config::rules::Conn {
        host: &fix.host,
        ip,
        dst_port: fix.port,
        process: fix.apps.first().map(String::as_str).unwrap_or_default(),
        ..Default::default()
    };
    let reported = fix.was_rule.to_ascii_lowercase();
    let own = rules
        .iter()
        .filter(|s| s.rule.enabled && s.source != SOURCE)
        .find(|s| {
            format!("{}({})", s.rule.core_type(), s.rule.value).to_ascii_lowercase() == reported
                || s.rule.matches(&conn) == Some(true)
        });
    if let Some(s) = own {
        return Some(Protect {
            reason: "rule".into(),
            by: s.rule.line(),
        });
    }
    let rule = fix.was_rule.split('(').next().unwrap_or_default();
    if !matches!(rule, "" | "Match") && fix.was != "DIRECT" {
        return Some(Protect {
            reason: "subscription".into(),
            by: format!("{} → {}", fix.was_rule, fix.was),
        });
    }
    None
}

/// The node a group routes through now, following nested groups.
pub(super) fn node_of(p: &mihomo_api::model::Proxies, target: &str) -> Option<String> {
    let mut name = target.to_owned();
    for _ in 0..8 {
        let proxy = p.proxies.get(&name)?;
        match &proxy.now {
            Some(next) if proxy.is_group() => name = next.clone(),
            _ => return (!proxy.is_group()).then_some(name),
        }
    }
    None
}

/// Each target's share of failed delay tests over the last 6 hours, in
/// 10% steps so that noise does not outweigh speed.
pub(super) fn loss_tiers(state: &State, p: &mihomo_api::model::Proxies, targets: &[String]) -> HashMap<String, u32> {
    let nodes: Vec<(String, String)> = targets.iter().filter_map(|t| node_of(p, t).map(|n| (t.clone(), n))).collect();
    let q = crate::latency::Query { hours: 6, points: 1, nodes: Some(nodes.iter().map(|(_, n)| n.clone()).collect()) };
    let Ok(views) = state.latency.lock().unwrap().query(&q, now()) else { return HashMap::new() };
    nodes
        .into_iter()
        .filter_map(|(t, n)| {
            let v = views.get(&n).filter(|v| v.tests >= 5)?;
            Some((t, (v.failures * 10 + v.tests / 2) / v.tests))
        })
        .collect()
}
