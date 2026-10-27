.pragma library

// Rule types grouped the way the editor shows them: a tab, then variants
// with the mihomo keyword underneath. Labels are I18n keys.
var tabs = [
  { key: "domain", icon: "globe", variants: ["domain-suffix", "domain", "domain-keyword", "domain-wildcard", "domain-regex"] },
  { key: "ip", icon: "ip", variants: ["ip-cidr", "ip-suffix", "src-ip-cidr"] },
  { key: "process", icon: "cpu", variants: ["process-name", "process-path", "process-name-regex", "process-path-regex", "uid"] },
  { key: "port", icon: "plug", variants: ["dst-port", "src-port", "network"] },
  { key: "sets", icon: "sets", variants: ["geosite", "geoip", "ip-asn", "rule-set"] },
  { key: "logic", icon: "logic", variants: ["and", "or", "not"] }
]

var keywords = {
  "domain": "DOMAIN", "domain-suffix": "DOMAIN-SUFFIX", "domain-keyword": "DOMAIN-KEYWORD",
  "domain-wildcard": "DOMAIN-WILDCARD", "domain-regex": "DOMAIN-REGEX", "geosite": "GEOSITE",
  "ip-cidr": "IP-CIDR", "ip-suffix": "IP-SUFFIX", "ip-asn": "IP-ASN", "geoip": "GEOIP",
  "src-ip-cidr": "SRC-IP-CIDR", "src-port": "SRC-PORT", "dst-port": "DST-PORT", "network": "NETWORK",
  "process-name": "PROCESS-NAME", "process-path": "PROCESS-PATH", "process-name-regex": "PROCESS-NAME-REGEX",
  "process-path-regex": "PROCESS-PATH-REGEX", "uid": "UID", "rule-set": "RULE-SET",
  "and": "AND", "or": "OR", "not": "NOT"
}

// Kinds a logic condition may use (no nesting in the form).
var conditionKinds = ["domain-suffix", "domain", "domain-keyword", "process-name", "process-path", "dst-port", "network", "ip-cidr", "src-ip-cidr", "geosite", "geoip", "ip-asn", "rule-set", "uid"]

function tabOf(kind) {
  for (var i = 0; i < tabs.length; i++) if (tabs[i].variants.indexOf(kind) !== -1) return tabs[i].key
  return "domain"
}

function tab(key) {
  for (var i = 0; i < tabs.length; i++) if (tabs[i].key === key) return tabs[i]
  return tabs[0]
}

function isLogic(kind) { return kind === "and" || kind === "or" || kind === "not" }

function keyword(kind, value) {
  if (kind === "ip-cidr" && String(value || "").indexOf(":") !== -1) return "IP-CIDR6"
  return keywords[kind] || String(kind).toUpperCase()
}

function kindOfKeyword(kw) {
  var k = String(kw).trim().toUpperCase()
  if (k === "IP-CIDR6") return "ip-cidr"
  for (var kind in keywords) if (keywords[kind] === k) return kind
  return ""
}

// The parenthesized value of a logic rule from [{kind, value}].
function logicValue(parts) {
  return "(" + parts.filter(function(p) { return String(p.value).trim() !== "" })
    .map(function(p) { return "(" + keyword(p.kind, p.value) + "," + String(p.value).trim() + ")" }).join(",") + ")"
}

// Domains shown with "*." when they include subdomains.
function display(kind, value) {
  if (kind === "domain-suffix") return "*." + value
  return value
}

function isIp(s) { return /^[0-9.]+$/.test(s) || String(s).indexOf(":") !== -1 }

// What a connection offers as a rule origin.
function origin(conn) {
  var m = conn.metadata || {}
  return {
    host: m.host || m.sniffHost || "",
    ip: m.destinationIP || "",
    process: m.process || "",
    process_path: m.processPath || "",
    port: m.destinationPort || "",
    network: m.network || "",
    route: (conn.chains || []).slice().reverse().join(" → "),
    rule: conn.rule ? conn.rule + (conn.rulePayload ? "(" + conn.rulePayload + ")" : "") : ""
  }
}

// Host for a route check of a rule, if it names one.
function probeHost(kind, value, origin) {
  if (origin && origin.host) return origin.host
  if (kind === "domain" || kind === "domain-suffix") return value
  if (kind === "ip-cidr") return String(value).split("/")[0]
  return ""
}

// Lifetime choices: key → params for rules.save.
function lifetime(key, now) {
  if (key === "hour") return { expires: now + 3600 * 1000, until_reboot: false }
  if (key === "day") return { expires: now + 24 * 3600 * 1000, until_reboot: false }
  if (key === "boot") return { expires: null, until_reboot: true }
  return { expires: null, until_reboot: false }
}

function lifetimeKey(rule) {
  if (!rule) return "always"
  if (rule.boot) return "boot"
  if (rule.expires) return "until"
  return "always"
}

// Suggestion sections in display order.
var sources = ["common", "conn", "window", "running", "log", "problem", "user", "db", "provider", "catalog", "known"]

// Rows for the suggestion list: section headers + items.
function rows(items) {
  var out = []
  for (var s = 0; s < sources.length; s++) {
    var group = items.filter(function(i) { return i.source === sources[s] })
    if (group.length === 0) continue
    out.push({ header: true, source: sources[s], count: group.length })
    for (var i = 0; i < group.length; i++) out.push(group[i])
  }
  return out
}

// Visible target chips: DIRECT, the chosen one, then groups up to `max`;
// the rest goes to the "more" menu.
function chips(targets, chosen, max) {
  var visible = targets.slice(0, max)
  if (chosen && !visible.some(function(t) { return t.name === chosen })) {
    var picked = targets.filter(function(t) { return t.name === chosen })[0]
    if (picked) visible[visible.length - 1] = picked
  }
  var names = visible.map(function(t) { return t.name })
  return { visible: visible, more: targets.filter(function(t) { return names.indexOf(t.name) === -1 }) }
}

// Same short list as the daemon's `registrable` (omm-config rules.rs).
var secondLevel = ["co.uk", "org.uk", "ac.uk", "gov.uk", "me.uk", "com.au", "net.au", "org.au", "co.jp", "ne.jp", "or.jp", "com.br", "com.cn", "net.cn", "org.cn",
  "com.tr", "com.ua", "org.ua", "net.ua", "pp.ua", "in.ua", "kiev.ua", "com.ru", "net.ru", "org.ru", "pp.ru", "msk.ru", "spb.ru", "com.kz", "org.kz",
  "co.kr", "co.in", "co.id", "co.nz", "co.za", "com.mx", "com.ar", "com.sg", "com.hk", "com.tw", "com.by", "github.io", "gitlab.io", "pages.dev",
  "workers.dev", "vercel.app", "netlify.app", "herokuapp.com", "appspot.com", "cloudfront.net", "azurewebsites.net", "blogspot.com"]

// Registrable domain (eTLD+1): rr3.googlevideo.com → googlevideo.com.
function registrable(host) {
  var h = String(host).toLowerCase().replace(/\.$/, "")
  var labels = h.split(".")
  if (labels.length <= 2 || isIp(h)) return h
  var keep = secondLevel.indexOf(labels.slice(-2).join(".")) !== -1 ? 3 : 2
  return labels.slice(-keep).join(".")
}

// Quick rules a connection offers in its menu: [{kind, value, title}].
function quickScopes(o) {
  var out = []
  if (o.host && !isIp(o.host)) {
    out.push({ kind: "domain", value: o.host, title: o.host })
    out.push({ kind: "domain-suffix", value: registrable(o.host), title: "*." + registrable(o.host) })
  }
  if (o.process) out.push({ kind: "process-name", value: o.process, title: o.process })
  var ip = o.ip || (isIp(o.host) ? o.host : "")
  if (ip) out.push({ kind: "ip-cidr", value: ip, title: ip + (ip.indexOf(":") !== -1 ? "/128" : "/32") })
  return out
}

// The rule R creates at once: by domain, else the address.
function quickDefault(o) {
  var s = quickScopes(o)
  for (var i = 0; i < s.length; i++) if (s[i].kind === "domain-suffix") return s[i]
  for (var j = 0; j < s.length; j++) if (s[j].kind === "ip-cidr") return s[j]
  return s[0] || null
}
