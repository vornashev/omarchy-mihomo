.pragma library

// Pure helpers for the traffic page: ranges, dimensions, stacking.

var MINUTE = 60000
var HOUR = 3600000
var DAY = 24 * HOUR

var RANGES = [
  { key: "15m", span: 15 * MINUTE },
  { key: "1h", span: HOUR },
  { key: "24h", span: DAY },
  { key: "7d", span: 7 * DAY },
  { key: "30d", span: 30 * DAY }
]

var DIMS = ["app", "domain", "host", "ip", "country", "asn", "group", "outbound", "rule"]

// Where a click on a row leads: the next, finer breakdown.
var NEXT = { app: "domain", domain: "host", host: "ip", ip: "", country: "asn", asn: "ip", group: "app", outbound: "app", rule: "app", network: "app" }

function span(key) {
  for (var i = 0; i < RANGES.length; i++) if (RANGES[i].key === key) return RANGES[i].span
  return DAY
}

function next(dim) { return NEXT[dim] || "" }

// `[alpm]`, `[root]`: another user's program the daemon cannot name.
function isService(app) { return app.length > 2 && app.charAt(0) === "[" && app.charAt(app.length - 1) === "]" }

// Rows from the daemon carry raw keys; empty means "no such field".
function label(dim, row, t) {
  var k = row.key
  if (dim === "app") return isService(k) ? k.slice(1, -1) : (k || t("trafficSystem"))
  if (dim === "domain" || dim === "host") return k || t("trafficUnnamed")
  if (dim === "asn") return k && k !== "0" ? "AS" + k + (row.label ? " " + row.label : "") : t("trafficUnknown")
  if (dim === "country") return k || t("trafficUnknown")
  if (dim === "rule") return k ? k.replace(",", " · ") : t("trafficUnknown")
  return k || t("trafficUnknown")
}

// Secondary text for a row.
function detail(dim, row, t) {
  var parts = []
  if (dim === "app" && isService(row.key)) parts.push(t("trafficService"))
  if (dim === "ip") {
    if (row.org) parts.push(row.org)
    if (row.country) parts.push(row.country)
  } else if (dim === "app" || dim === "domain" || dim === "country" || dim === "asn" || dim === "group" || dim === "outbound") {
    if (row.hosts > 1) parts.push(t("trafficHosts").arg(row.hosts))
  }
  if (dim !== "app" && row.app && (dim === "host" || dim === "ip")) parts.push(row.app)
  return parts.join(" · ")
}

// A filter chip's text.
function chipLabel(f, t) {
  var v = label(f.dim, { key: f.value, label: f.label || "" }, t)
  return { dim: t("trafficDim_" + f.dim), value: (f.exclude ? "≠ " : "") + v }
}

function sameFilter(a, b) { return a.dim === b.dim && a.value === b.value && a.exclude === b.exclude }

// Add a filter, replacing its opposite; returns a new list.
function addFilter(list, f) {
  var out = list.filter(function(x) { return !(x.dim === f.dim && x.value === f.value) })
  out.push(f)
  return out
}

function values(s, metric) {
  if (!s) return []
  if (metric === "up") return s.up
  if (metric === "down") return s.down
  return s.down.map(function(v, i) { return v + s.up[i] })
}

// Cumulative tops for stacked areas, bottom series first.
function stack(series, key, hidden) {
  var out = []
  var n = series.length ? series[0][key].length : 0
  var cum = []
  for (var i = 0; i < n; i++) cum.push(0)
  for (var s = 0; s < series.length; s++) {
    var bottom = cum.slice()
    if (!hidden[series[s].key]) for (var j = 0; j < n; j++) cum[j] += series[s][key][j]
    out.push({ bottom: bottom, top: cum.slice() })
  }
  return out
}

function maxOf(arr) {
  var m = 0
  for (var i = 0; i < arr.length; i++) if (arr[i] > m) m = arr[i]
  return m
}

// A round axis top above `v` bytes per second: 1, 2, 2.5, 5 × 10^n of
// the binary unit the label will use, so the axis reads "5 МБ/с".
function niceMax(v) {
  if (v <= 0) return 1024
  var unit = 1
  while (v / unit >= 1024) unit *= 1024
  var x = v / unit
  var p = Math.pow(10, Math.floor(Math.log(x) / Math.LN10))
  var steps = [1, 2, 2.5, 5, 10]
  for (var i = 0; i < steps.length; i++) if (steps[i] * p >= x) return steps[i] * p >= 1000 ? 1024 * unit : steps[i] * p * unit
  return 1024 * unit
}

// Bytes in a bucket → bytes per second.
function rate(bytes, step) { return bytes * 1000 / Math.max(step, 1) }

// Weekday × hour totals from an hourly series; Monday first, local time.
function heatmap(all, from, step) {
  var grid = []
  for (var d = 0; d < 7; d++) { var row = []; for (var h = 0; h < 24; h++) row.push(0); grid.push(row) }
  if (!all) return grid
  for (var i = 0; i < all.down.length; i++) {
    var date = new Date(from + i * step)
    var day = (date.getDay() + 6) % 7
    grid[day][date.getHours()] += all.down[i] + all.up[i]
  }
  return grid
}

function percent(part, whole) {
  if (!whole) return "0%"
  var p = part * 100 / whole
  return (p >= 10 || p === 0 ? Math.round(p) : p.toFixed(1)) + "%"
}

// "+12%" / "−4%" against the previous span, or "" without history.
function change(now, before) {
  if (!before) return ""
  var p = Math.round((now - before) * 100 / before)
  return (p > 0 ? "▲ " : (p < 0 ? "▼ " : "")) + Math.abs(p) + "%"
}

// Shortest and longest window the wheel can reach.
var MIN_SPAN = 10 * MINUTE
var MAX_SPAN = 30 * DAY

// A window scaled by `factor` around `at` (both ms), then shifted by
// `shift` ms, kept between MIN_SPAN and MAX_SPAN and inside the stored
// history: never later than `now`, never earlier than 30 days back.
function zoomWindow(from, to, at, factor, shift, now) {
  var span = Math.min(MAX_SPAN, Math.max(MIN_SPAN, (to - from) * factor))
  var a = at - (at - from) * span / (to - from) + shift
  var b = a + span
  if (b > now) { a -= b - now; b = now }
  if (a < now - MAX_SPAN) { b += now - MAX_SPAN - a; a = now - MAX_SPAN }
  return { from: Math.round(a), to: Math.round(b) }
}

// Back to the live range once the window covers it up to now.
function coversRange(win, rangeSpan, now, step) {
  return win.to >= now - Math.max(step, MINUTE) && win.to - win.from >= rangeSpan * 0.98
}

