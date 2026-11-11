.pragma library

// Site checks: sorting, latency classes and what a new site's form fills in.

var SLOW_MS = 300

// "idle" | "testing" | "good" | "slow" | "fail"
function state(result, testing) {
  if (testing) return "testing"
  if (!result) return "idle"
  if (!result.ok) return "fail"
  return result.delay < SLOW_MS ? "good" : "slow"
}

function host(url) {
  var m = String(url || "").trim().match(/^(?:[a-z][a-z0-9+.-]*:\/\/)?([^\/?#:]+)/i)
  return m ? m[1].toLowerCase() : ""
}

// "youtube.com" for https://www.youtube.com/ — what the card shows under the name.
function shortHost(url) {
  return host(url).replace(/^www\./, "")
}

// The name a new site gets when the field is left empty.
function defaultName(url) {
  return shortHost(url)
}

// The letter on the tile when there is no favicon.
function letter(site) {
  var s = String((site && (site.name || shortHost(site.url))) || "?")
  return s.charAt(0).toUpperCase()
}

// A stable tile colour per host, picked from `palette`.
function tint(url, palette) {
  var h = shortHost(url), n = 0
  for (var i = 0; i < h.length; i++) n = (n * 31 + h.charCodeAt(i)) % 9973
  return palette[n % palette.length]
}

// `custom` keeps the user's order; `delay` puts working sites first, fastest
// first, then failures, then unchecked; `name` is alphabetical.
function sorted(sites, results, mode) {
  var list = (sites || []).slice()
  results = results || {}
  if (mode === "delay") {
    var rank = function(s) {
      var r = results[s.id]
      if (!r) return [2, 0]
      return r.ok ? [0, r.delay] : [1, 0]
    }
    list.sort(function(a, b) {
      var x = rank(a), y = rank(b)
      return x[0] - y[0] || x[1] - y[1] || sites.indexOf(a) - sites.indexOf(b)
    })
  } else if (mode === "name") {
    list.sort(function(a, b) { return String(a.name).toLowerCase().localeCompare(String(b.name).toLowerCase()) })
  }
  return list
}

// Ids after moving `id` by `step` places in the user's order.
function moved(sites, id, step) {
  var ids = (sites || []).map(function(s) { return s.id })
  var from = ids.indexOf(id)
  if (from < 0) return ids
  var to = Math.max(0, Math.min(ids.length - 1, from + step))
  ids.splice(from, 1)
  ids.splice(to, 0, id)
  return ids
}

// { ok, slow, fail } over the sites that have a result.
function counts(sites, results) {
  var c = { ok: 0, slow: 0, fail: 0 }
  ;(sites || []).forEach(function(s) {
    var st = state(results ? results[s.id] : null, false)
    if (st === "good") c.ok++
    else if (st === "slow") c.slow++
    else if (st === "fail") c.fail++
  })
  return c
}

// "Europe · NL-02": the top group and the node that carried it.
function route(result) {
  var chain = result && result.chain ? result.chain : []
  if (chain.length === 0) return ""
  if (chain.length === 1) return chain[0]
  return chain[0] + " · " + chain[chain.length - 1]
}

// The newest check time among the results, 0 when none.
function lastChecked(results) {
  var t = 0
  for (var k in results || {}) t = Math.max(t, results[k].at || 0)
  return t
}

// "auto" | "url" | "letter" for the form, from a saved `icon`.
function iconMode(icon) {
  if (!icon) return "auto"
  return icon === "letter" ? "letter" : "url"
}

// The saved `icon` for a form state.
function iconValue(mode, url) {
  if (mode === "letter") return "letter"
  if (mode === "url" && String(url || "").trim() !== "") return String(url).trim()
  return null
}
