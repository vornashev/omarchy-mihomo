.pragma library

function routeKey(problem) {
  var r = problem.route || {}
  return JSON.stringify([r.rule || "", r.payload || "", r.target || ""])
}

function groups(items) {
  var result = [], byKey = Object.create(null)
  items.forEach(function(p) {
    var key = routeKey(p), g = byKey[key]
    if (!g) {
      g = { key: key, route: p.route || {}, items: [], count: 0, kinds: {}, apps: {}, unresolved: false }
      byKey[key] = g
      result.push(g)
    }
    g.items.push(p)
    g.count += p.count || 0
    g.unresolved = g.unresolved || !(p.ok_since_error > 0)
    Object.keys(p.kinds || {}).forEach(function(k) { g.kinds[k] = (g.kinds[k] || 0) + p.kinds[k] })
    Object.keys(p.apps || {}).forEach(function(a) { g.apps[a] = (g.apps[a] || 0) + p.apps[a] })
  })
  return result
}

function rows(items, grouped, collapsed) {
  if (!grouped) return items.map(function(p) { return { key: "host:" + p.host, problem: p } })
  var result = []
  groups(items).forEach(function(g) {
    var closed = collapsed[g.key] === true
    result.push({ key: "group:" + g.key, group: g, closed: closed, last: closed })
    if (!closed) g.items.forEach(function(p, i) {
      result.push({ key: "host:" + p.host, problem: p, groupKey: g.key, last: i === g.items.length - 1 })
    })
  })
  return result
}

// Route only the listed resources. Never turn a Match group into a global rule.
function groupScopes(group) {
  var seen = Object.create(null), result = []
  group.items.forEach(function(p) {
    var value = p.domain_hint || p.host
    var kind = p.domain_hint ? (p.route && p.route.rule === "DomainSuffix" ? "domain-suffix" : "domain")
      : (p.is_ip || value.indexOf(":") !== -1 || /^[0-9.]+$/.test(value) ? "ip-cidr" : "domain")
    var key = JSON.stringify([kind, value])
    if (!seen[key]) { seen[key] = true; result.push({ kind: kind, value: value }) }
  })
  return result
}
