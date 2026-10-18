.pragma library

var basics = {
  ss: [["name"],["server","port"],["password","cipher"]],
  ssr: [["name"],["server","port"],["password","cipher"],["obfs","protocol"]],
  socks5: [["name"],["server","port"],["username","password"]],
  http: [["name"],["server","port"],["username","password"]],
  vless: [["name"],["server","port"],["uuid","flow"]],
  vmess: [["name"],["server","port"],["uuid","cipher","alterId"]],
  snell: [["name"],["server","port"],["psk","version"]],
  trojan: [["name"],["server","port"],["password"]],
  hysteria: [["name"],["server","port"],["auth-str"],["up","down"]],
  hysteria2: [["name"],["server","port"],["password"],["up","down"]],
  wireguard: [["name"],["server","port"],["private-key","public-key"],["ip","ipv6"],["allowed-ips"]],
  tuic: [["name"],["server","port"],["uuid","password"],["token"]],
  shadowquic: [["name"],["server","port"],["username","password"]],
  "gost-relay": [["name"],["server","port"],["username","password"]],
  direct: [["name"]], dns: [["name"]], reject: [["name"]],
  rematch: [["name"],["target-rematch-name","target-sub-rule"]],
  ssh: [["name"],["server","port"],["username","password"],["private-key"]],
  mieru: [["name"],["server","port"],["username","password"],["transport"]],
  anytls: [["name"],["server","port"],["password"]],
  sudoku: [["name"],["server","port"],["key","aead-method"]],
  masque: [["name"],["server","port"],["private-key","public-key"],["ip","ipv6"]],
  trusttunnel: [["name"],["server","port"],["username","password"]],
  openvpn: [["name"],["server","port","proto"],["username","password"],["ca"]],
  tailscale: [["name"],["hostname","auth-key"],["exit-node"]],
  zerotier: [["name"],["network"],["identity-secret"]],
  easytier: [["name"],["network-name","network-secret"],["peers"]]
}
var security = ["tls","sni","servername","reality-opts","alpn","client-fingerprint","skip-cert-verify","name-cert-verify","fingerprint","certificate","ech-opts","shadow-tls-opts","restls-opts","jls-opts","tlsmirror-opts"]
var transport = ["network","ws-opts","grpc-opts","xhttp-opts","h2-opts","http-opts","mkcp-opts","mekya-opts","ws-headers","plugin","plugin-opts","obfs-opts"]
function service(type) { return ["direct","dns","reject","rematch"].indexOf(type) >= 0 }
function basicKeys(type) { return (basics[type] || [["name"],["server","port"]]).reduce(function(a,r) { return a.concat(r) },[]) }
function section(field,type) {
  var k = field.key
  if (basicKeys(type).indexOf(k) >= 0) return "basic"
  if (type === "wireguard" && k === "peers") return "peers"
  if (type === "wireguard" && k === "amnezia-wg-option") return "amnezia"
  if (security.indexOf(k) >= 0) return "security"
  if (transport.indexOf(k) >= 0 && !(type === "zerotier" && k === "network")) return "transport"
  return "advanced"
}
function sections(schema,type) {
  var keys = ["basic","security","transport","peers","amnezia","advanced"]
  return keys.filter(function(k) { return k === "basic" || k === "advanced" || schema.fields.some(function(f) { return section(f,type) === k }) })
}
function field(schema,key) { return schema.fields.filter(function(f) { return f.key === key })[0] }
function tlsMode(d) { return d["reality-opts"] ? "reality" : d.tls === false ? "none" : d.tls || ["trojan","hysteria","hysteria2","tuic","anytls","masque","shadowquic","trusttunnel"].indexOf(d.type)>=0 ? "tls" : "none" }
function summary(d,section) {
  if(section === "security") return tlsMode(d).toUpperCase() + (d.servername || d.sni ? " · " + (d.servername || d.sni) : "") + (d["client-fingerprint"] ? " · " + d["client-fingerprint"] : "")
  if(section === "transport") return d.network || (["hysteria","hysteria2","tuic","shadowquic","masque"].indexOf(d.type)>=0 ? "QUIC" : d.transport || "TCP")
  return ""
}
function diff(a,b,path) {
  a = a || {}; b = b || {}; path = path || ""
  var out = []
  Object.keys(a).concat(Object.keys(b)).filter(function(k,i,l) { return l.indexOf(k) === i }).forEach(function(k) {
    if (JSON.stringify(a[k]) === JSON.stringify(b[k])) return
    var key = path ? path + "." + k : k
    if (a[k] && b[k] && typeof a[k] === "object" && typeof b[k] === "object" && !Array.isArray(a[k]) && !Array.isArray(b[k])) out = out.concat(diff(a[k],b[k],key))
    else out.push({key:key,before:a[k],after:b[k]})
  })
  return out
}
function display(value,key) {
  if(value === undefined || value === null) return "—"
  if(/password|private.key|secret|token|auth.key|psk/i.test(key)) return "••••••••"
  return typeof value === "object" ? JSON.stringify(value) : String(value)
}
