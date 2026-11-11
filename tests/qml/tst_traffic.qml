import QtQuick
import QtTest
import qs.Commons
import "../../plugin" as Plugin
import "../../plugin/TrafficModels.js" as TM
import "../../plugin/Notices.js" as Notices

Rectangle {
  id: scene
  width: 854; height: 684
  color: "#1e1e2e"

  QtObject {
    id: service
    property var calls: []
    property bool empty: false
    property color red: "#f38ba8"
    property color yellow: "#f9e2af"
    property color green: "#a6e3a1"
    property color peach: "#fab387"
    property color mauve: "#cba6f7"
    function themeColor(name, fallback) { return name === "cyan" ? "#94e2d5" : fallback }
    function formatBytes(n) {
      n = Number(n) || 0
      var u = ["Б", "КБ", "МБ", "ГБ"], i = 0
      while (n >= 1024 && i < u.length - 1) { n /= 1024; i++ }
      return (i === 0 ? n.toFixed(0) : n.toFixed(1)) + " " + u[i]
    }
    function formatRate(n) { return formatBytes(n) + "/с" }

    // A day of evening-heavy traffic, 96 quarter hours.
    function curve(k, i) {
      var h = (17.75 + i * 0.25) % 24
      function b(c, w) { var d = Math.abs(h - c); d = Math.min(d, 24 - d); return Math.exp(-d * d / (2 * w * w)) }
      var f = [0.25 + 2.2 * b(21, 2) + 1.7 * b(13, 2.6), 6.3 * b(22, 0.7), 0.12 + 0.5 * b(19, 3) + 0.45 * b(11, 2), 1.1 * b(14.5, 2.2), 0.45 * b(12, 3)][k]
      return Math.round(f * 900 * 1048576 * (1 + 0.12 * Math.sin(i * 1.7 + k * 2.3)))
    }
    function view(params) {
      var n = 96, step = 15 * 60000, from = Date.now() - n * step
      from -= from % step
      var names = params.by === "group" ? ["DIRECT", "Proxy", "AI"] : (params.by === "country" ? ["RU", "NL", "US", "DE"] : (params.by === "domain" ? ["googlevideo.com", "github.com", "reddit.com", "wikipedia.org", "anthropic.com"] : ["firefox", "steam", "Telegram", "code", "spotify"]))
      if (empty) names = []
      var series = [], rows = [], total = { up: 0, down: 0, conns: 0, apps: 27, hosts: 1904 }
      var all = { key: "", label: "", up: [], down: [] }
      for (var j = 0; j < n; j++) { all.up.push(0); all.down.push(0) }
      // A group or country filter keeps part of some keys, none of others.
      var sliced = (params.filters || []).some(function(f) { return (f.dim === "group" || f.dim === "country") && params.by !== f.dim })
      for (var k = 0; k < names.length; k++) {
        var s = { key: names[k], label: "", up: [], down: [] }
        var part = sliced ? [0.6, 0, 0.3, 0, 1][k % 5] : 1
        for (var i = 0; i < n; i++) { var d = Math.round(curve(k % 5, i) * part); s.down.push(d); s.up.push(Math.round(d * 0.06)); all.down[i] += d; all.up[i] += Math.round(d * 0.06) }
        if (sliced && part === 0) continue
        var sd = s.down.reduce(function(a, b) { return a + b }, 0), su = s.up.reduce(function(a, b) { return a + b }, 0)
        total.down += sd; total.up += su; total.conns += 100 * (k + 1)
        rows.push({ key: names[k], label: "", up: su, down: sd, conns: 100 * (k + 1), share: 0, org: "", country: "", app: "", hosts: 10 * (5 - k), spark: s.down.filter(function(_, i) { return i % 4 === 0 }) })
        if (k < (params.limit === undefined ? 6 : params.limit)) series.push(s)
      }
      rows.forEach(function(r) { r.share = (r.up + r.down) / Math.max(1, total.up + total.down) })
      rows.sort(function(a, b) { return b.up + b.down - a.up - a.down })
      var other = { key: "", label: "", up: all.up.map(function() { return 0 }), down: all.down.map(function() { return 0 }) }
      return { by: params.by, from: from, to: from + n * step, step: step, series: series, other: other, all: all, rows: rows,
        total: total, previous: { up: total.up * 1.04, down: total.down * 0.88 }, peak: 12373196, peak_at: from + 16 * step, since: from }
    }
    function request(method, params, callback) {
      calls.push({ method: method, params: params })
      if (method === "traffic.query") callback(view(params), null)
      else if (method === "traffic.status") callback({ enabled: true, rows: 1200, since: 0, bytes: 4096, asn: true, country: true }, null)
      else if (callback) callback(null, null)
    }
    function queries(by) { return calls.filter(function(c) { return c.method === "traffic.query" && (!by || c.params.by === by) }) }
  }
  QtObject {
    id: panelStub
    property var opened: null
    property string toast: ""
    function openRule(spec) { opened = spec }
    function notify(t) { toast = t }
    function forceKeys() {}
  }

  Plugin.TrafficPage { id: page; anchors.fill: parent; svc: service; panel: panelStub }

  TestCase {
    name: "Traffic"
    when: windowShown

    function init() {
      service.empty = false
      page.zoom = null
      page.filters = []
      page.by = "app"
      page.range = "24h"
      page.metric = "both"
      page.hidden = {}
      page.probe = null
      page.focusKey = ""
      service.calls = []
    }

    function test_models() {
      compare(TM.next("app"), "domain")
      compare(TM.next("ip"), "")
      compare(TM.label("app", { key: "" }, Plugin.MihomoI18n.t), Plugin.MihomoI18n.t("trafficSystem"))
      compare(TM.label("asn", { key: "15169", label: "GOOGLE" }, Plugin.MihomoI18n.t), "AS15169 GOOGLE")
      var st = TM.stack([{ key: "a", down: [1, 2] }, { key: "b", down: [3, 4] }], "down", { a: true })
      compare(st[1].top, [3, 4])
      compare(TM.niceMax(730), 1024)
      compare(TM.niceMax(3.1 * 1048576), 5 * 1048576)
      compare(TM.change(110, 100), "▲ 10%")
      var grid = TM.heatmap({ up: [1, 0], down: [2, 5] }, new Date(2026, 9, 26, 21).getTime(), TM.HOUR)
      compare(grid[0][21], 3)
      compare(grid[0][22], 5)
      var f = TM.addFilter([{ dim: "app", value: "x", exclude: false }], { dim: "app", value: "x", exclude: true })
      compare(f.length, 1); verify(f[0].exclude)
    }

    function test_service_apps_and_notices() {
      compare(TM.label("app", { key: "[alpm]" }, Plugin.MihomoI18n.t), "alpm")
      verify(TM.detail("app", { key: "[alpm]", hosts: 0 }, Plugin.MihomoI18n.t).length > 0)
      page.by = "app"
      verify(!page.canRule({ key: "[root]" }))
      verify(page.canRule({ key: "firefox" }))
      var spike = Notices.text({ kind: "traffic_spike", data: { app: "steam", bytes: 3 * 1073741824, usual_per_hour: 100 * 1048576, new: false, domains: [["steamcontent.com", 1]], groups: [] } }, Plugin.MihomoI18n.t, function() { return "" })
      compare(spike.title, "steam: 3.0 ГБ за час")
      verify(spike.body.indexOf("steamcontent.com") !== -1)
      verify(spike.body.indexOf("31") !== -1, spike.body)
      var route = Notices.text({ kind: "traffic_route", data: { app: "curl", proxied: 150 * 1048576, groups: [["Proxy", 1]], domains: [] } }, Plugin.MihomoI18n.t, function() { return "" })
      compare(route.title, "curl пошёл через прокси: 150.0 МБ за час")
      verify(route.body.indexOf("Proxy") !== -1)
    }

    function test_drill_down_adds_filter_and_changes_dimension() {
      var rows = findChild(page, "trafficRows")
      tryVerify(function() { return page.rows.length === 5 })
      var picked = page.rows[1].key
      page.cursor = 1
      page.activate()
      compare(page.filters.length, 1)
      compare(page.filters[0].dim, "app")
      compare(page.filters[0].value, picked)
      compare(page.by, "domain")
      var last = service.queries("domain")
      verify(last.length > 0)
      compare(last[last.length - 1].params.filters[0].value, picked)
      // Backspace-like removal pops the filter.
      page.remove()
      compare(page.filters.length, 0)
    }

    function test_exclude_and_invert_chip() {
      tryVerify(function() { return page.rows.length > 0 })
      page.cursor = 0
      page.textKey("x")
      compare(page.filters.length, 1)
      verify(page.filters[0].exclude)
      page.invertFilter(0)
      verify(!page.filters[0].exclude)
      compare(page.by, "app", "excluding keeps the breakdown")
    }

    function test_range_and_zoom() {
      page.textKey("t")
      compare(page.range, "7d")
      verify(page.longRange)
      var side = service.calls.filter(function(c) { return c.params && c.params.step === TM.HOUR })
      verify(side.length > 0, "heatmap asks for hourly buckets")
      page.zoom = { from: 1000, to: 2000 }
      tryVerify(function() { var q = service.queries(); return q[q.length - 1].params.to === 2000 })
      page.remove()
      compare(page.zoom, null)
    }

    function test_zoom_window_math() {
      var now = 100 * TM.DAY
      // Zoom in ×0.8 around the middle keeps the middle in place.
      var w = TM.zoomWindow(now - TM.DAY, now, now - TM.DAY / 2, 0.8, 0, now)
      compare(w.to - w.from, Math.round(TM.DAY * 0.8))
      compare((w.from + w.to) / 2, now - TM.DAY / 2)
      // Never past now, never shorter than the minimum.
      w = TM.zoomWindow(now - TM.HOUR, now, now, 0.01, 0, now)
      compare(w.to, now); compare(w.to - w.from, TM.MIN_SPAN)
      w = TM.zoomWindow(now - TM.HOUR, now, now, 1, TM.DAY, now)
      compare(w.to, now)
      // Never longer than 30 days or older than the history.
      w = TM.zoomWindow(now - 20 * TM.DAY, now, now - 10 * TM.DAY, 10, 0, now)
      compare(w.to - w.from, TM.MAX_SPAN); compare(w.from, now - TM.MAX_SPAN)
      verify(TM.coversRange({ from: now - TM.DAY, to: now }, TM.DAY, now, TM.MINUTE))
      verify(!TM.coversRange({ from: now - TM.DAY, to: now - TM.HOUR }, TM.DAY, now, TM.MINUTE))
    }

    function test_wheel_zooms_around_pointer_and_pans() {
      tryVerify(function() { return page.rows.length > 0 })
      var chart = findChild(page, "trafficChart")
      var span0 = chart.vTo - chart.vFrom
      var at = chart.timeAt(chart.plotX + chart.plotW / 2)
      chart.wheel(chart.plotX + chart.plotW / 2, 120, 0, false)
      verify(page.zoom !== null)
      fuzzyCompare(page.zoom.to - page.zoom.from, span0 * 0.8, 1000)
      fuzzyCompare(chart.timeAt(chart.plotX + chart.plotW / 2), at, 1000)
      // The chart follows before the data comes back.
      compare(chart.vFrom, page.zoom.from)
      var from = page.zoom.from
      chart.wheel(chart.plotX + 10, -120, 0, true)
      verify(page.zoom.from > from, "Shift+wheel down moves later")
      // Wheel through the zoom-out back past the whole range: live again.
      for (var i = 0; i < 6 && page.zoom; i++) chart.wheel(chart.plotX + chart.plotW, -240, 0, false)
      if (page.zoom) verify(page.zoom.to - page.zoom.from > TM.DAY)
    }

    function test_rule_from_row() {
      tryVerify(function() { return page.rows.length > 0 })
      page.cursor = 0
      page.textKey("r")
      compare(panelStub.opened.kind, "process-name")
      compare(panelStub.opened.value, page.rows[0].key)
      compare(panelStub.opened.from, "traffic")
      page.by = "country"
      verify(!page.canRule(page.rows[0]))
    }

    function test_legend_hides_series() {
      tryVerify(function() { return page.chartSeries.length === 5 })
      page.toggleSeries("steam")
      verify(page.hidden.steam)
      page.soloSeries("firefox")
      compare(Object.keys(page.hidden).length, 4)
      verify(!page.hidden.firefox)
    }

    function test_hover_links_rows_chart_and_side() {
      tryVerify(function() { return page.rows.length === 5 })
      var chart = findChild(page, "trafficChart")
      page.setFocus(page.rows[0].key, true)
      compare(chart.focusKey, page.rows[0].key)
      tryVerify(function() { return page.focusGroups !== null })
      var g = service.queries("group")
      var last = g[g.length - 1].params
      compare(last.filters[last.filters.length - 1].value, page.rows[0].key)
      compare(last.from, page.view.from)
      page.setFocus(page.rows[0].key, false)
      compare(chart.focusKey, "")

      page.setProbe({ dim: "group", value: "AI", label: "AI", color: "#fab387" }, true)
      tryVerify(function() { return chart.overlay !== null })
      tryVerify(function() { return findChild(page, "probeBar") !== null })
      verify(Object.keys(page.probeRows).length < page.rows.length, "some rows are outside the group")
      page.setProbe({ dim: "group", value: "AI" }, false)
      compare(page.probe, null)
      compare(chart.overlay, null)

      // Same dimension as the breakdown: a focus, not an overlay.
      page.by = "group"
      page.setProbe({ dim: "group", value: "DIRECT" }, true)
      compare(page.focusKey, "DIRECT")
      compare(page.probe, null)
    }

    function test_heatmap_and_chart_bands() {
      page.range = "7d"
      var chart = findChild(page, "trafficChart")
      page.heatDay = 2; page.heatHour = 21
      compare(chart.bandDay, 2)
      verify(chart.inBand !== undefined)
      page.heatDay = -1; page.heatHour = -1
    }

    function test_screenshots() {
      tryVerify(function() { return page.rows.length > 0 })
      var chart = findChild(page, "trafficChart")
      wait(50)
      mouseMove(chart, chart.width * 0.2, chart.height * 0.4)
      tryVerify(function() { return findChild(page, "trafficTooltip").visible })
      wait(50)
      grabImage(scene).save("target/qml-shots/traffic-24h.png")
      mouseMove(scene, 1, 1)
      page.setProbe({ dim: "group", value: "AI", label: "AI", color: service.peach }, true)
      tryVerify(function() { return chart.overlay !== null })
      page.setFocus(page.rows[1].key, true)
      tryVerify(function() { return page.focusGroups !== null })
      wait(50)
      grabImage(scene).save("target/qml-shots/traffic-hover.png")
      page.setFocus(page.rows[1].key, false)
      page.setProbe({ dim: "group", value: "AI" }, false)
      mouseMove(scene, 1, 1)
      page.filters = [{ dim: "app", value: "firefox", exclude: false }]
      page.by = "domain"
      page.range = "7d"
      wait(50)
      grabImage(scene).save("target/qml-shots/traffic-7d-firefox.png")
      service.empty = true
      page.filters = []
      wait(50)
      grabImage(scene).save("target/qml-shots/traffic-empty.png")
    }
  }
}
