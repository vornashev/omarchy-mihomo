.pragma library

// Search in the large window's title bar: sections, problem hosts, active
// connections and own rules, in that order, each matched by a
// case-insensitive substring.

function has(text, query) {
  return String(text || "").toLowerCase().indexOf(query) >= 0
}

// A connection by host, sniffed host, IP, process or proxy chain; `query`
// is lower case. Shared with the connections page filter.
function connMatches(c, query) {
  var m = c.metadata
  return has(m.host + " " + m.sniffHost + " " + m.destinationIP + " " + m.process + " " + c.chains.join(" "), query)
}

// [{ kind: "page" | "host" | "conn" | "rule", label, detail, page?, problem?, query?, rule? }]
// Connections are grouped by host, at most three of them.
function results(query, pages, hosts, conns, rules, limit) {
  var q = String(query || "").trim().toLowerCase()
  if (!q) return []
  var out = []
  ;(pages || []).forEach(function(p) {
    if (has(p.label, q) || has(p.key, q)) out.push({ kind: "page", label: p.label, detail: "", page: p.key, icon: p.icon })
  })
  ;(hosts || []).forEach(function(h) {
    if (!has(h.host, q)) return
    var route = h.route || {}
    out.push({ kind: "host", label: h.host, detail: route.rule ? route.rule + " → " + route.target : (route.target || ""), problem: h })
  })
  var groups = []
  ;(conns || []).forEach(function(c) {
    if (!connMatches(c, q)) return
    var m = c.metadata, host = m.host || m.sniffHost || m.destinationIP
    for (var i = 0; i < groups.length; i++) if (groups[i].query === host) { groups[i].count++; return }
    if (groups.length >= 3) return
    groups.push({ kind: "conn", label: host, query: host, count: 1,
      route: c.chains.slice().reverse().join(" → ") + " · " + c.rule + (c.rulePayload ? "(" + c.rulePayload + ")" : "") })
  })
  groups.forEach(function(g) {
    out.push({ kind: "conn", label: g.label, query: g.query, detail: g.route + (g.count > 1 ? " · ×" + g.count : "") })
  })
  ;(rules || []).forEach(function(r) {
    if (has(r.value, q) || has(r.target, q) || has(r.note, q)) out.push({ kind: "rule", label: r.value, detail: r.kind + " → " + r.target, rule: r })
  })
  return out.slice(0, limit || 8)
}
