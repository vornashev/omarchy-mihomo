.pragma library

// Desktop notification text for a daemon notice. The daemon says what
// happened; wording lives here so it follows the panel's language.
// `t` is I18n.t, `clock(ms)` formats a time of day.

function delayOf(fix, target) {
  var r = (fix.results || []).filter(function(o) { return o.target === target })[0]
  return r && r.delay ? r.delay : 0
}

function minutesLeft(fix, now) {
  return Math.max(1, Math.round(((fix.ends_at || now) - now) / 60000))
}

function autofix(fix, t, clock) {
  var host = fix.host
  if (fix.error) return { title: t("afTitleError").arg(host), body: fix.error, click: t("afClickDefault") }
  switch (fix.state) {
  case "trial":
    return {
      title: t("afTitleTrial").arg(host).arg(fix.target),
      body: t("afBodyTrial").arg(fix.was || "DIRECT").arg(fix.failures).arg(fix.target).arg(delayOf(fix, fix.target)).arg(clock(fix.ends_at)),
      click: t("afClickTrial")
    }
  case "found":
    return {
      title: t("afTitleFound").arg(host).arg(fix.was || "DIRECT"),
      body: t("afBodyFound").arg(fix.target).arg(delayOf(fix, fix.target)),
      click: t("afClickFound")
    }
  case "pinned":
    return { title: t("afTitlePinned").arg(host).arg(fix.target), body: t("afBodyPinned").arg(fix.trial_ok), click: t("afClickDefault") }
  case "rolled_back":
    var failed = (fix.tried || [])[(fix.tried || []).length - 1] || ""
    return {
      title: t("afTitleRolled").arg(host).arg(failed),
      body: fix.target ? t("afBodyRolled").arg(fix.target) : t("afBodyRolledNone"),
      click: fix.target ? t("afClickRolled") : t("afClickDefault")
    }
  case "unreachable":
    return { title: t("afTitleUnreachable").arg(host), body: t("afBodyUnreachable"), click: t("afClickDefault") }
  case "reverted":
    return { title: t("afTitleReverted").arg(host).arg(fix.was), body: t("afBodyReverted").arg(fix.target), click: t("afClickDefault") }
  }
  return null
}

// 1.2 ГБ, without the panel's Service at hand.
function bytes(n, t) {
  var u = ["B", "KB", "MB", "GB", "TB"], i = 0
  n = Number(n) || 0
  while (n >= 1024 && i < u.length - 1) { n /= 1024; i++ }
  return (i === 0 ? n.toFixed(0) : n.toFixed(1)) + " " + t("unit" + u[i])
}

function names(list) { return (list || []).map(function(p) { return p[0] }).join(", ") }

function text(n, t, clock) {
  var d = n.data || {}
  switch (n.kind) {
  case "autofix":
    return autofix(d, t, clock)
  case "core_down":
    return {
      title: d.restarting ? t("nCoreRestarting").arg(d.restart_count + 1) : t("nCoreDown"),
      body: t("nCoreDownBody"),
      click: t("nClickCore")
    }
  case "core_up":
    return { title: t("nCoreUp"), body: t("nCoreUpBody"), click: "" }
  case "dead_group":
    return { title: t("nDeadGroup").arg(d.group), body: t("nDeadGroupBody").arg(d.nodes).arg(d.group).arg(d.rules).arg(d.connections), click: t("nClickGroup") }
  case "strict_down":
    return { title: t("nStrictDown").arg(d.group), body: t("nStrictDownBody").arg(d.nodes).arg((d.hosts || []).length), click: t("nClickStrict") }
  case "strict_up":
    return { title: t("nStrictUp").arg(d.group), body: t("nStrictUpBody").arg(Math.max(1, Math.round((d.down_ms || 0) / 60000))), click: "" }
  case "group_alive":
    return { title: t("nGroupAlive").arg(d.group), body: "", click: "" }
  case "subscription":
    return { title: t("nSubFailed").arg(d.name), body: t("nSubFailedBody").arg(d.error || ""), click: t("afClickDefault") }
  case "orphans":
    var gone = []
    ;(d.rules || []).forEach(function(r) { if (gone.indexOf(r.target) === -1) gone.push(r.target) })
    return { title: t("nOrphans").arg((d.rules || []).length), body: t("nOrphansBody").arg(gone.join(", ")), click: t("nClickFix") }
  case "saved":
    return { title: t("nSaved"), body: d.line || "", click: t("nClickSaved") }
  case "traffic_spike":
    return {
      title: t("nTrafficSpike").arg(d.app).arg(bytes(d.bytes, t)),
      body: d.new ? t("nTrafficSpikeNew").arg(names(d.domains)) : t("nTrafficSpikeBody").arg(bytes(d.usual_per_hour, t)).arg(Math.round(d.bytes / Math.max(d.usual_per_hour, 1))).arg(names(d.domains)),
      click: t("afClickDefault")
    }
  case "traffic_route":
    return { title: t("nTrafficRoute").arg(d.app).arg(bytes(d.proxied, t)), body: t("nTrafficRouteBody").arg(names(d.groups)), click: t("afClickDefault") }
  case "core_update":
    return { title: t("nCoreUpdate").arg(d.latest), body: t("nCoreUpdateBody").arg(d.current || "—"), click: t("nOpenCoreSettings") }
  case "window_unknown":
    return { title: t("nWindowUnknown"), body: t("nWindowUnknownBody"), click: "" }
  }
  return null
}
