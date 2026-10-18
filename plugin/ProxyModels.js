.pragma library

function clone(v) { return v === undefined ? undefined : JSON.parse(JSON.stringify(v)) }
function put(value, key, next) {
  var out = clone(value) || {}
  if (next === undefined || next === null) delete out[key]
  else out[key] = next
  return out
}
function visible(field, context) {
  if (!field.when) return true
  for (var key in field.when) if ((context || {})[key] !== field.when[key]) return false
  return true
}
function fresh(field) {
  if (field.kind === "object" || field.kind === "map") return {}
  if (field.kind === "list") return []
  if (field.kind === "boolean") return false
  if (field.kind === "number") return 0
  return ""
}
function suggestedName(name, existing) {
  if (existing.indexOf(name) < 0) return name
  var i = 2
  while (existing.indexOf(name + " (" + i + ")") >= 0) i++
  return name + " (" + i + ")"
}
function imported(rows, existing) {
  var names = existing.slice()
  return rows.map(function(r) {
    var d = clone(r.definition)
    var original = d ? d.name : ""
    if (d) { d.name = suggestedName(d.name, names); names.push(d.name) }
    return { index:r.index, definition:d, original:original, error:r.error || "", warnings:r.warnings || [], selected:!!d && !(r.warnings || []).length, action:"rename" }
  })
}
function schema(catalog, group, type) {
  var list = catalog ? (group ? catalog.groups : catalog.nodes) : []
  return (list || []).filter(function(s) { return s.type === type })[0] || {type:type,fields:[]}
}

function section(field, group) {
  var k=field.key
  if(group) {
    if(["name","default-selected","strategy","tolerance"].indexOf(k)>=0)return "basic"
    if(["proxies","use","include-all","include-all-proxies","include-all-providers","filter","exclude-filter","exclude-type"].indexOf(k)>=0)return "members"
    if(["url","interval","timeout","max-failed-times","expected-status","lazy","tolerance","empty-fallback"].indexOf(k)>=0)return "health"
    return "advanced"
  }
  if(["network","ws-opts","grpc-opts","xhttp-opts","h2-opts","http-opts","mkcp-opts","mekya-opts","smux","ws-headers"].indexOf(k)>=0)return "transport"
  if(["tls","sni","servername","alpn","client-fingerprint","fingerprint","skip-cert-verify","name-cert-verify","certificate","reality-opts","ech-opts","shadow-tls-opts","restls-opts","jls-opts","tlsmirror-opts"].indexOf(k)>=0)return "security"
  if(["name","server","port","uuid","username","password","cipher","flow","alterId","ip","ipv6","private-key","public-key","peers","reserved","allowed-ips","amnezia-wg-option","auth","auth-str","token","network-id","config"].indexOf(k)>=0)return "basic"
  return "advanced"
}

function delayText(delay, unit) {
  return delay === undefined || delay === null ? "—" : delay === 0 ? "×" : String(delay) + " " + (unit || "ms")
}
function moveMember(members, from, to) {
  var next = (members || []).slice()
  if (from < 0 || to < 0 || from >= next.length || to >= next.length) return next
  next.splice(to, 0, next.splice(from, 1)[0])
  return next
}
function toggleMember(members, name, enabled) {
  var next = (members || []).filter(function(n) { return n !== name })
  if (enabled) next.push(name)
  return next
}
// The preview accepts common Go/RE2 expressions, including a leading (?i).
// Core validation remains authoritative for expressions outside this subset.
function nameRegex(pattern) {
  var flags = ""
  if (pattern.indexOf("(?i)") === 0) { flags = "i"; pattern = pattern.slice(4) }
  return new RegExp(pattern, flags)
}
function filterNodes(nodes, draft) {
  var include, exclude
  try {
    include = draft.filter ? nameRegex(draft.filter) : null
    exclude = draft["exclude-filter"] ? nameRegex(draft["exclude-filter"]) : null
  } catch(e) { return { error: String(e), items: [] } }
  var types = String(draft["exclude-type"] || "").toLowerCase().split("|")
  var items = nodes.filter(function(n) {
    return draft["include-all"] || (n.source === "provider" ? draft["include-all-providers"] || (draft.use || []).indexOf(n.provider) >= 0 : draft["include-all-proxies"])
  }).map(function(n) {
    var reason = exclude && exclude.test(n.name) ? "proxyFilterExcluded" : types.indexOf(String(n.type).toLowerCase()) >= 0 ? "proxyExcludeTypes" : include && !include.test(n.name) ? "proxyFilterMismatch" : ""
    return { name: n.name, reason: reason }
  })
  return { error: "", items: items }
}

// Lookup for select labels; absent latency stays unknown, never healthy.
function choiceDetails(items, groups) {
  var result = {};
  (items || []).forEach(function(item) { result[item.name] = item; });
  (groups || []).forEach(function(group) {
    (group.members || []).forEach(function(member) { result[member.name] = member; });
    var current = (group.members || []).filter(function(member) { return member.name === group.now; })[0];
    if (current) result[group.name] = { delay: current.delay };
  });
  return result;
}
