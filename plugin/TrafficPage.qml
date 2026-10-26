import QtQuick
import QtQml
import QtQuick.Controls
import qs.Commons
import "TrafficModels.js" as TM

// Traffic history: how much went where, by app, domain, address, country
// and route. Clicking a row filters by it and breaks it down further.
Item {
  id: root
  property var svc
  property var panel

  property string range: "24h"
  property string metric: "both"
  property string by: "app"
  property var filters: []
  property string search: ""
  // A dragged span of the chart, {from, to}; null follows "now".
  property var zoom: null
  property var hidden: ({})
  property int cursor: 0

  property var view: null
  property var routes: null
  property var side: null
  property var status: null
  property bool loading: false
  property int generation: 0
  readonly property bool typing: searchField.input.activeFocus || rowMenu.visible
  readonly property bool longRange: isLong(range, zoom)
  function isLong(r, z) { return z ? z.to - z.from > 2 * TM.DAY : TM.span(r) > 2 * TM.DAY }

  readonly property var colors: [Color.accent, svc.peach, svc.mauve, svc.green, svc.yellow, svc.themeColor("cyan", svc.red)]
  readonly property color otherColor: Util.alpha(Color.popups.text, 0.45)

  // Series for the chart: top keys, then the rest as "other".
  readonly property var chartSeries: {
    if (!view) return []
    var out = view.series.map(function(s, i) {
      return { key: s.key, label: TM.label(by, s, I18n.t), color: colors[i % colors.length], up: s.up, down: s.down }
    })
    if (TM.maxOf(view.other.down) + TM.maxOf(view.other.up) > 0)
      out.push({ key: "\u0000other", label: I18n.t("trafficOther"), color: otherColor, up: view.other.up, down: view.other.down })
    return out
  }
  readonly property var rows: view ? view.rows : []
  readonly property var colorOf: {
    var m = {}
    for (var i = 0; i < chartSeries.length; i++) m[chartSeries[i].key] = chartSeries[i].color
    return m
  }
  readonly property double totalBytes: view ? view.total.up + view.total.down : 0

  function query(extra) {
    var now = Date.now()
    var q = {
      from: zoom ? zoom.from : now - TM.span(range),
      by: by,
      filters: filters.map(function(f) { return { dim: f.dim, value: f.value, exclude: f.exclude } }),
      search: search
    }
    if (zoom) q.to = zoom.to
    for (var k in extra) q[k] = extra[k]
    return q
  }

  function reload() {
    var gen = ++generation
    loading = true
    svc.request("traffic.query", query({ limit: 6, rows: 60 }), function(r, err) {
      if (gen !== generation) return
      loading = false
      if (r) {
        view = r
        cursor = Math.min(cursor, Math.max(0, r.rows.length - 1))
        hoverCache = {}
        if (probe || litKey !== "") loadHover()
      }
      else if (err) panel.notify(err.message)
    })
    svc.request("traffic.query", query({ by: "group", limit: 0, rows: 5 }), function(r) { if (gen === generation && r) routes = r })
    var sideQuery = isLong(range, zoom) ? query({ by: "app", limit: 0, rows: 0, step: TM.HOUR }) : query({ by: "country", limit: 0, rows: 5 })
    svc.request("traffic.query", sideQuery, function(r) { if (gen === generation && r) side = r })
  }

  function refreshStatus() { svc.request("traffic.status", null, function(r) { if (r) status = r }) }

  onRangeChanged: { zoom = null; reload() }
  onByChanged: { cursor = 0; hidden = {}; focusKey = ""; probe = null; reload() }
  onFiltersChanged: { cursor = 0; reload() }
  onZoomChanged: reload()
  onSearchChanged: searchDelay.restart()
  Timer { id: searchDelay; interval: 300; onTriggered: root.reload() }
  Component.onCompleted: {
    if (panel && panel.trafficFilter) {
      filters = panel.trafficFilter
      by = "domain"
      range = "1h"
      panel.trafficFilter = null
    }
    reload(); refreshStatus()
  }
  Timer { interval: 5000; running: root.zoom === null && TM.span(root.range) <= TM.DAY; repeat: true; onTriggered: root.reload() }

  // -------------------------------------------------------------- highlight
  // Hovering a row, a legend entry or a band of the chart lights one key of
  // the current breakdown everywhere: the chart lifts it, the row and the
  // legend mark it, the side card shows its routes and countries. Hovering
  // a group or a country in the side card overlays that slice instead: on
  // the chart as an outline, in the rows as part of each share bar.
  property string focusKey: ""
  readonly property string litKey: focusKey !== "" ? focusKey : chart.hoveredKey
  // {dim, value, color}
  property var probe: null
  property var probeView: null
  property var focusGroups: null
  property var focusCountries: null
  property var hoverCache: ({})
  readonly property var probeRows: {
    var m = {}
    if (probeView) for (var i = 0; i < probeView.rows.length; i++) m[probeView.rows[i].key] = probeView.rows[i].up + probeView.rows[i].down
    return m
  }
  // The probed slice, apart from every series color.
  readonly property color probeColor: Color.popups.text
  // Heatmap cell under the pointer → a band on the chart.
  property int heatDay: -1
  property int heatHour: -1

  function setFocus(key, on) {
    if (on) focusKey = key
    else if (focusKey === key) focusKey = ""
  }
  function setProbe(p, on) {
    if (p.dim === by) { setFocus(p.value, on); return }
    if (on) probe = p
    else if (probe && probe.dim === p.dim && probe.value === p.value) probe = null
  }
  onLitKeyChanged: hoverDelay.restart()
  onProbeChanged: { if (!probe) probeView = null; hoverDelay.restart() }
  Timer { id: hoverDelay; interval: 120; onTriggered: root.loadHover() }

  // A query over exactly the shown buckets, plus one more filter.
  function hoverQuery(extra, filter) {
    var q = query(extra)
    q.from = view.from; q.to = view.to; q.step = view.step
    q.filters = q.filters.concat([{ dim: filter.dim, value: filter.value, exclude: false }])
    return q
  }
  function cached(key, params, done) {
    if (hoverCache[key]) { done(hoverCache[key]); return }
    var gen = generation
    svc.request("traffic.query", params, function(r) {
      if (!r || gen !== generation) return
      hoverCache[key] = r
      done(r)
    })
  }
  function loadHover() {
    if (!view) return
    if (probe) {
      var p = probe
      cached("p|" + p.dim + "|" + p.value, hoverQuery({ limit: 0, rows: 200 }, p), function(r) { if (root.probe === p) root.probeView = r })
    }
    var key = litKey
    if (key !== "" && key.charAt(0) !== "\u0000") {
      var f = { dim: by, value: key }
      cached("g|" + key, hoverQuery({ by: "group", limit: 0, rows: 4 }, f), function(r) { if (root.litKey === key) root.focusGroups = r })
      if (!longRange) cached("c|" + key, hoverQuery({ by: "country", limit: 0, rows: 5 }, f), function(r) { if (root.litKey === key) root.focusCountries = r })
    } else {
      focusGroups = null
      focusCountries = null
    }
  }

  // ---------------------------------------------------------------- actions
  function filterBy(row, exclude) {
    filters = TM.addFilter(filters, { dim: by, value: row.key, exclude: exclude, label: row.label })
  }
  function drill(row) {
    if (!row) return
    var next = TM.next(by)
    filterBy(row, false)
    if (next) by = next
  }
  function removeFilter(i) { var f = filters.slice(); f.splice(i, 1); filters = f }
  function invertFilter(i) {
    var f = filters.slice()
    f[i] = Object.assign({}, f[i], { exclude: !f[i].exclude })
    filters = f
  }
  function resetAll() { filters = []; zoom = null; search = ""; searchField.text = "" }
  function toggleSeries(key) {
    var h = Object.assign({}, hidden)
    if (h[key]) delete h[key]; else h[key] = true
    hidden = h
  }
  function soloSeries(key) {
    var h = {}
    chartSeries.forEach(function(s) { if (s.key !== key) h[s.key] = true })
    hidden = Object.keys(hidden).length === chartSeries.length - 1 && !hidden[key] ? {} : h
  }

  readonly property var ruleKinds: ({ app: "process-name", domain: "domain-suffix", host: "domain", ip: "ip-cidr" })
  function canRule(row) { return !!row && !!row.key && ruleKinds[by] !== undefined && !(by === "app" && TM.isService(row.key)) }
  function ruleFor(row) {
    if (!canRule(row)) return
    var origin = { host: by === "domain" || by === "host" ? row.key : "", ip: by === "ip" ? row.key : "", process: by === "app" ? row.key : "", process_path: "", port: "", network: "", route: "", rule: "" }
    panel.openRule({ from: "traffic", origin: origin, kind: ruleKinds[by], value: row.key })
  }
  function copy(row) {
    clipboard.text = row.key
    clipboard.selectAll(); clipboard.copy(); clipboard.deselect()
    panel.notify(I18n.t("copiedToast").arg(row.key))
  }
  function openMenu(row, anchor) {
    rowMenu.row = row
    if (anchor) rowMenu.popup(anchor, 0, anchor.height)
    else rowMenu.popup()
  }

  // Keyboard, from the panel.
  function move(dx, dy) {
    if (dx !== 0 && dy === 0) {
      var i = Math.max(0, Math.min(TM.DIMS.length - 1, TM.DIMS.indexOf(by) + dx))
      by = TM.DIMS[i]
      return
    }
    cursor = Math.max(0, Math.min(rows.length - 1, cursor + dy))
    list.positionViewAtIndex(cursor, ListView.Contain)
  }
  function activate() { drill(rows[cursor]) }
  function remove() { if (filters.length) removeFilter(filters.length - 1); else if (zoom) zoom = null }
  function textKey(t) {
    var row = rows[cursor]
    if (t === "/") searchField.input.forceActiveFocus()
    else if (t === "x" && row) filterBy(row, true)
    else if (t === "o" && row) filterBy(row, false)
    else if (t === "r" && row) ruleFor(row)
    else if (t === "m" && row) openMenu(row, null)
    else if (t === "t") { var r = TM.RANGES.map(function(x) { return x.key }); range = r[(r.indexOf(range) + 1) % r.length] }
    else if (t === "d") metric = metric === "both" ? "down" : (metric === "down" ? "up" : "both")
  }

  function timeLabel(ms, long) {
    var d = new Date(ms)
    var loc = Qt.locale(I18n.language === "en" ? "en_US" : "ru_RU")
    var step = view ? view.step : TM.MINUTE
    var span = view ? view.to - view.from : TM.DAY
    if (long) return d.toLocaleString(loc, span > TM.DAY ? "ddd d MMM, HH:mm" : "HH:mm") + (step > TM.MINUTE ? "–" + new Date(ms + step).toLocaleString(loc, "HH:mm") : "")
    if (span > 2 * TM.DAY) return d.toLocaleString(loc, step >= TM.DAY ? "d MMM" : "ddd d")
    return d.toLocaleString(loc, "HH:mm")
  }

  TextEdit { id: clipboard; visible: false }

  // ----------------------------------------------------------------- layout
  readonly property real gap: Style.space(10)
  readonly property real bottomHeight: Math.max(Style.space(250), height * 0.38)

  Column {
    id: head
    width: parent.width
    spacing: root.gap

    Item {
      width: parent.width
      height: Math.max(titles.implicitHeight, rangeSeg.implicitHeight)
      Column {
        id: titles
        width: parent.width - rangeSeg.width - zoomBtn.width - Style.space(16)
        spacing: Style.space(3)
        Txt { text: I18n.t("navTraffic"); bold: true; font.pixelSize: Style.font.heading }
        Txt {
          width: parent.width
          text: root.zoom ? I18n.t("trafficSubZoom").arg(root.timeLabel(root.zoom.from, true)).arg(root.timeLabel(root.zoom.to, true))
            : I18n.t("trafficSub").arg(I18n.t("trafficRangeLong_" + root.range)) + (root.status && !root.status.enabled ? " · " + I18n.t("trafficPaused") : "")
          dim: true; font.pixelSize: Style.font.bodySmall
        }
      }
      Btn {
        id: zoomBtn
        objectName: "trafficResetZoom"
        visible: root.zoom !== null
        width: visible ? implicitWidth : 0
        anchors.right: rangeSeg.left; anchors.rightMargin: Style.space(8)
        anchors.verticalCenter: parent.verticalCenter
        icon: Icons.back; text: I18n.t("trafficResetZoom")
        onClicked: root.zoom = null
      }
      Seg {
        id: rangeSeg
        objectName: "trafficRange"
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        current: root.zoom ? "" : root.range
        model: TM.RANGES.map(function(r) { return { key: r.key, label: I18n.t("trafficRange_" + r.key) } })
        onPicked: function(k) { if (k === root.range) root.zoom = null; else root.range = k }
      }
    }

    Item {
      width: parent.width
      height: Style.space(28)
      Row {
        id: filterRow
        height: parent.height
        width: parent.width - metricSeg.width - Style.space(8)
        spacing: Style.space(8)
        clip: true
        Field {
          id: searchField
          width: Style.space(250)
          icon: Icons.search
          placeholder: I18n.t("trafficSearch")
          onTextChanged: root.search = text
          onEscaped: { text = ""; input.focus = false; root.panel.forceKeys() }
        }
        Rectangle {
          visible: root.filters.length > 0
          width: crumb.implicitWidth + Style.space(12); height: parent.height
          radius: Style.cornerRadius
          color: crumbMa.containsMouse ? Style.hoverFillFor(Color.popups.text, Color.accent) : "transparent"
          Txt { id: crumb; anchors.centerIn: parent; text: Icons.back + " " + I18n.t("trafficAll"); color: Color.accent; font.pixelSize: Style.font.bodySmall }
          MouseArea { id: crumbMa; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.resetAll() }
        }
        Repeater {
          model: root.filters
          Rectangle {
            required property var modelData
            required property int index
            objectName: "trafficChip"
            readonly property var chip: TM.chipLabel(modelData, I18n.t)
            readonly property color tone: modelData.exclude ? root.svc.red : Color.accent
            anchors.verticalCenter: parent.verticalCenter
            width: chipRow.implicitWidth + Style.space(14)
            height: Style.space(24)
            radius: Style.cornerRadius
            color: Util.alpha(tone, 0.12)
            border.width: 1
            border.color: Util.alpha(tone, 0.45)
            Row {
              id: chipRow
              anchors.centerIn: parent
              spacing: Style.space(6)
              Txt { text: chip.dim; dim: true; font.pixelSize: Style.font.caption; anchors.verticalCenter: parent.verticalCenter }
              Txt { text: chip.value; bold: true; color: modelData.exclude ? root.svc.red : Color.popups.text; font.pixelSize: Style.font.caption; anchors.verticalCenter: parent.verticalCenter }
              Txt {
                text: Icons.close; dim: true; font.pixelSize: Style.font.caption; anchors.verticalCenter: parent.verticalCenter
                MouseArea { anchors.fill: parent; anchors.margins: -Style.space(3); cursorShape: Qt.PointingHandCursor; onClicked: root.removeFilter(index) }
              }
            }
            MouseArea { anchors.fill: parent; anchors.rightMargin: Style.space(18); cursorShape: Qt.PointingHandCursor; onClicked: root.invertFilter(index) }
            HoverHandler { id: chipHover }
            Hint { visible: chipHover.hovered; text: I18n.t("trafficChipHint") }
          }
        }
      }
      Seg {
        id: metricSeg
        objectName: "trafficMetric"
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        current: root.metric
        model: [{ key: "down", label: I18n.t("trafficMetric_down") }, { key: "up", label: I18n.t("trafficMetric_up") }, { key: "both", label: I18n.t("trafficMetric_both") }]
        onPicked: function(k) { root.metric = k }
      }
    }

    Row {
      width: parent.width
      spacing: root.gap
      Repeater {
        model: {
          var v = root.view
          if (!v) return [["", "—", "", ""], ["", "—", "", ""], ["", "—", "", ""], ["", "—", "", ""]]
          var dc = TM.change(v.total.down, v.previous.down), uc = TM.change(v.total.up, v.previous.up)
          return [
            [I18n.t("trafficStatDown"), root.svc.formatBytes(v.total.down), dc ? I18n.t("trafficVsPrev").arg(dc) : "", dc.indexOf("▲") === 0 ? "warn" : ""],
            [I18n.t("trafficStatUp"), root.svc.formatBytes(v.total.up), uc ? I18n.t("trafficVsPrev").arg(uc) : "", uc.indexOf("▲") === 0 ? "warn" : ""],
            [I18n.t("trafficStatPeak"), root.svc.formatRate(v.peak), v.peak ? root.timeLabel(v.peak_at, true) : "", ""],
            [I18n.t("trafficStatConns"), String(v.total.conns), I18n.t("trafficAppsHosts").arg(v.total.apps).arg(v.total.hosts), ""]
          ]
        }
        Rectangle {
          required property var modelData
          width: (parent.width - root.gap * 3) / 4
          height: Style.space(58)
          radius: Style.cornerRadius
          color: Util.alpha(Color.popups.text, 0.04)
          border.width: 1
          border.color: Util.alpha(Color.popups.text, 0.12)
          Column {
            anchors.fill: parent
            anchors.margins: Style.space(10)
            anchors.leftMargin: Style.space(12)
            spacing: Style.space(3)
            Txt { text: modelData[0]; faint: true; font.pixelSize: Style.font.caption }
            Row {
              width: parent.width
              spacing: Style.space(8)
              Txt { id: kv; text: modelData[1]; bold: true; font.pixelSize: Style.font.heading }
              Txt {
                width: parent.width - kv.width - Style.space(8)
                anchors.baseline: kv.baseline
                text: modelData[2]
                color: modelData[3] === "warn" ? root.svc.peach : Util.alpha(Color.popups.text, 0.68)
                font.pixelSize: Style.font.caption
              }
            }
          }
        }
      }
    }
  }

  // Chart card.
  Rectangle {
    id: chartCard
    anchors.top: head.bottom; anchors.topMargin: root.gap
    anchors.bottom: bottomRow.top; anchors.bottomMargin: root.gap
    width: parent.width
    radius: Style.cornerRadius
    color: Util.alpha(Color.popups.text, 0.04)
    border.width: 1
    border.color: Util.alpha(Color.popups.text, 0.12)

    Item {
      id: chartHead
      x: Style.space(14); y: Style.space(12)
      width: parent.width - Style.space(28)
      height: Style.space(16)
      Txt { id: chartTitle; text: I18n.t("trafficByTime") + " · " + I18n.t("trafficDims_" + root.by).toUpperCase(); faint: true; bold: true; font.pixelSize: Style.font.caption; font.letterSpacing: 1 }
      Row {
        anchors.right: parent.right
        width: Math.min(implicitWidth, parent.width - chartTitle.width - Style.space(16))
        clip: true
        spacing: Style.space(12)
        layoutDirection: Qt.RightToLeft
        Repeater {
          model: root.chartSeries.slice().reverse()
          Row {
            required property var modelData
            objectName: "trafficLegend"
            spacing: Style.space(5)
            opacity: root.hidden[modelData.key] ? 0.35 : (root.litKey !== "" && root.litKey !== modelData.key ? 0.45 : 1)
            Rectangle { width: Style.space(8); height: width; radius: 2; color: modelData.color; anchors.verticalCenter: parent.verticalCenter }
            Txt { text: modelData.label; dim: root.litKey !== modelData.key; bold: root.litKey === modelData.key; font.pixelSize: Style.font.bodySmall; font.strikeout: !!root.hidden[modelData.key] }
            TapHandler { onTapped: root.toggleSeries(modelData.key); onDoubleTapped: root.soloSeries(modelData.key) }
            HoverHandler { onHoveredChanged: root.setFocus(modelData.key, hovered) }
          }
        }
      }
    }
    TrafficChart {
      id: chart
      objectName: "trafficChart"
      anchors.top: chartHead.bottom; anchors.topMargin: Style.space(10)
      anchors.left: parent.left; anchors.leftMargin: Style.space(8)
      anchors.right: parent.right; anchors.rightMargin: Style.space(14)
      anchors.bottom: parent.bottom; anchors.bottomMargin: Style.space(8)
      svc: root.svc
      series: root.chartSeries
      from: root.view ? root.view.from : 0
      step: root.view ? root.view.step : TM.MINUTE
      metric: root.metric
      hidden: root.hidden
      visible: root.totalBytes > 0
      timeLabel: root.timeLabel
      focusKey: root.litKey
      bandDay: root.heatDay
      bandHour: root.heatHour
      overlay: root.probeView && root.probe ? { label: I18n.t("trafficThrough").arg(root.probe.label || root.probe.value), color: root.probeColor, up: root.probeView.all.up, down: root.probeView.all.down } : null
      onZoomed: function(a, b) { root.zoom = { from: a, to: b } }
    }
    Txt {
      visible: root.view !== null && root.totalBytes === 0
      anchors.centerIn: parent
      width: parent.width * 0.7
      horizontalAlignment: Text.AlignHCenter
      wrapMode: Text.WordWrap
      text: root.status && !root.status.enabled ? I18n.t("trafficDisabled") : (root.filters.length || root.search ? I18n.t("trafficNoMatch") : I18n.t("trafficEmpty"))
      dim: true
    }
  }

  // Ranking and the side card.
  Item {
    id: bottomRow
    anchors.bottom: parent.bottom
    width: parent.width
    height: root.bottomHeight

    Rectangle {
      id: breakdown
      width: parent.width - sideCard.width - root.gap
      height: parent.height
      radius: Style.cornerRadius
      color: Util.alpha(Color.popups.text, 0.04)
      border.width: 1
      border.color: Util.alpha(Color.popups.text, 0.12)

      Row {
        id: dimRow
        x: Style.space(12); y: Style.space(10)
        spacing: Style.space(10)
        Txt { text: I18n.t("trafficBy"); faint: true; bold: true; font.pixelSize: Style.font.caption; font.letterSpacing: 1; anchors.verticalCenter: parent.verticalCenter }
        Seg {
          id: dimSeg
          objectName: "trafficDims"
          current: root.by
          model: TM.DIMS.map(function(d) { return { key: d, label: I18n.t("trafficDims_" + d) } })
          onPicked: function(k) { root.by = k }
        }
      }

      // Column widths shared by the header and rows.
      readonly property real wBar: Math.max(Style.space(60), Math.min(Style.space(130), width * 0.17))
      readonly property real wNum: Style.space(64)
      readonly property real wPct: Style.space(40)
      readonly property real wSpark: Style.space(64)

      Item {
        id: colHead
        x: Style.space(12); y: dimRow.y + dimRow.height + Style.space(8)
        width: parent.width - Style.space(24) - Style.space(6)
        height: Style.space(14)
        Txt { x: Style.space(26); text: I18n.t("trafficDim_" + root.by); faint: true; font.pixelSize: Style.font.caption }
        Row {
          anchors.right: parent.right; anchors.rightMargin: Style.space(8)
          spacing: Style.space(10)
          Txt { width: breakdown.wBar; text: I18n.t("trafficShare"); faint: true; font.pixelSize: Style.font.caption }
          Txt { width: breakdown.wNum; horizontalAlignment: Text.AlignRight; text: "↓"; faint: true; font.pixelSize: Style.font.caption }
          Txt { width: breakdown.wNum; horizontalAlignment: Text.AlignRight; text: "↑"; faint: true; font.pixelSize: Style.font.caption }
          Txt { width: breakdown.wPct; horizontalAlignment: Text.AlignRight; text: "%"; faint: true; font.pixelSize: Style.font.caption }
          Txt { width: breakdown.wSpark; text: I18n.t("trafficRange_" + root.range); faint: true; font.pixelSize: Style.font.caption }
        }
      }

      ListView {
        id: list
        objectName: "trafficRows"
        anchors.top: colHead.bottom; anchors.topMargin: Style.space(4)
        anchors.left: parent.left; anchors.leftMargin: Style.space(12)
        anchors.right: parent.right; anchors.rightMargin: Style.space(6)
        anchors.bottom: hints.top; anchors.bottomMargin: Style.space(4)
        clip: true
        spacing: Style.space(1)
        model: root.rows
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
        delegate: Rectangle {
          id: rowItem
          required property var modelData
          required property int index
          readonly property color tone: root.colorOf[modelData.key] !== undefined ? root.colorOf[modelData.key] : root.otherColor
          readonly property double maxShare: root.rows.length ? root.rows[0].share : 1
          readonly property bool lit: root.litKey === modelData.key
          // Part of this row inside the probed group or country.
          readonly property double probed: root.probeView ? (root.probeRows[modelData.key] || 0) / Math.max(modelData.up + modelData.down, 1) : -1
          opacity: probed === 0 ? 0.35 : 1
          border.width: lit ? 1 : 0
          border.color: Util.alpha(rowItem.tone, 0.8)
          width: list.width - Style.space(6)
          height: Style.space(26)
          radius: 4
          color: index === root.cursor ? Style.hoverFillFor(Color.popups.text, Color.accent) : (rma.containsMouse ? Util.alpha(Color.popups.text, 0.04) : "transparent")
          MouseArea {
            id: rma
            anchors.fill: parent
            hoverEnabled: true
            onContainsMouseChanged: root.setFocus(modelData.key, containsMouse)
            cursorShape: Qt.PointingHandCursor
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onClicked: function(mouse) {
              root.cursor = index
              if (mouse.button === Qt.RightButton) root.openMenu(modelData, null)
              else root.drill(modelData)
            }
          }
          Rectangle { x: Style.space(8); width: Style.space(10); height: width; radius: 2; color: rowItem.tone; anchors.verticalCenter: parent.verticalCenter }
          Row {
            x: Style.space(26)
            width: nums.x - x - Style.space(10)
            anchors.verticalCenter: parent.verticalCenter
            spacing: Style.space(8)
            clip: true
            Txt { id: rn; text: TM.label(root.by, modelData, I18n.t); bold: index === root.cursor; width: Math.min(implicitWidth, parent.width); font.pixelSize: Style.font.bodySmall }
            Txt { width: parent.width - rn.width - Style.space(8); text: TM.detail(root.by, modelData, I18n.t); faint: true; font.pixelSize: Style.font.caption; anchors.verticalCenter: parent.verticalCenter }
          }
          Row {
            id: nums
            anchors.right: parent.right; anchors.rightMargin: Style.space(8)
            anchors.verticalCenter: parent.verticalCenter
            spacing: Style.space(10)
            Item {
              width: breakdown.wBar; height: Style.space(6)
              anchors.verticalCenter: parent.verticalCenter
              Rectangle { anchors.fill: parent; radius: 3; color: Util.alpha(Color.popups.text, 0.06) }
              Rectangle {
                id: shareBar
                width: Math.max(3, parent.width * modelData.share / Math.max(rowItem.maxShare, 0.0001)); height: parent.height; radius: 3
                color: Util.alpha(rowItem.tone, rowItem.probed >= 0 ? 0.3 : 0.8)
              }
              Rectangle {
                objectName: "probeBar"
                visible: rowItem.probed > 0
                width: Math.max(2, shareBar.width * Math.min(1, rowItem.probed)); height: parent.height; radius: 3
                color: root.probeColor
              }
            }
            Txt { width: breakdown.wNum; horizontalAlignment: Text.AlignRight; text: root.svc.formatBytes(modelData.down); font.pixelSize: Style.font.bodySmall }
            Txt { width: breakdown.wNum; horizontalAlignment: Text.AlignRight; text: root.svc.formatBytes(modelData.up); dim: true; font.pixelSize: Style.font.bodySmall }
            Txt { width: breakdown.wPct; horizontalAlignment: Text.AlignRight; text: TM.percent(modelData.up + modelData.down, root.totalBytes); dim: true; font.pixelSize: Style.font.bodySmall }
            Canvas {
              width: breakdown.wSpark; height: Style.space(16)
              anchors.verticalCenter: parent.verticalCenter
              property var points: modelData.spark
              onPointsChanged: requestPaint()
              onPaint: {
                var ctx = getContext("2d")
                ctx.reset()
                var p = points || []
                if (p.length < 2) return
                var m = TM.maxOf(p) || 1
                ctx.beginPath()
                for (var i = 0; i < p.length; i++) {
                  var x = i * (width - 2) / (p.length - 1) + 1, y = height - 1 - (height - 2) * p[i] / m
                  if (i) ctx.lineTo(x, y); else ctx.moveTo(x, y)
                }
                ctx.strokeStyle = rowItem.tone
                ctx.lineWidth = 1.2
                ctx.stroke()
              }
            }
          }
        }
        Txt { visible: root.view !== null && root.rows.length === 0; anchors.centerIn: parent; text: I18n.t("trafficNoRows"); dim: true }
      }

      Row {
        id: hints
        x: Style.space(20)
        anchors.bottom: parent.bottom; anchors.bottomMargin: Style.space(8)
        spacing: Style.space(14)
        Repeater {
          model: [["Enter", TM.next(root.by) ? I18n.t("trafficHintDrill").arg(I18n.t("trafficDims_" + TM.next(root.by)).toLowerCase()) : I18n.t("trafficHintOnly")], ["x", I18n.t("trafficHintExclude")], ["r", I18n.t("trafficHintRule")], ["m", I18n.t("trafficHintMenu")], ["t", I18n.t("trafficHintRange")]]
          Row {
            required property var modelData
            spacing: Style.space(5)
            Rectangle {
              width: kt.implicitWidth + Style.space(8); height: kt.implicitHeight + Style.space(2)
              radius: 3; color: Style.hoverFillFor(Color.popups.text, Color.accent)
              Txt { id: kt; anchors.centerIn: parent; text: modelData[0]; dim: true; font.pixelSize: Style.font.caption }
            }
            Txt { text: modelData[1]; faint: true; font.pixelSize: Style.font.caption }
          }
        }
      }
    }

    Rectangle {
      id: sideCard
      anchors.right: parent.right
      width: Style.space(290)
      height: parent.height
      radius: Style.cornerRadius
      color: Util.alpha(Color.popups.text, 0.04)
      border.width: 1
      border.color: Util.alpha(Color.popups.text, 0.12)

      readonly property var routeSource: root.focusGroups && root.litKey !== "" ? root.focusGroups : root.routes
      readonly property var routeRows: routeSource ? routeSource.rows.slice(0, 4) : []
      readonly property double routeTotal: routeSource ? routeSource.total.up + routeSource.total.down : 0
      readonly property double proxied: {
        var d = 0
        for (var i = 0; i < routeRows.length; i++) if (routeRows[i].key === "DIRECT") d += routeRows[i].up + routeRows[i].down
        return routeTotal ? 1 - d / routeTotal : 0
      }

      Column {
        x: Style.space(12); y: Style.space(10)
        width: parent.width - Style.space(24)
        spacing: Style.space(8)
        Txt {
          width: parent.width
          text: I18n.t("trafficRoute") + (sideCard.routeSource === root.focusGroups && root.focusGroups ? " · " + TM.label(root.by, { key: root.litKey }, I18n.t) : "")
          faint: true; bold: true; font.pixelSize: Style.font.caption; font.letterSpacing: 1
        }
        Row {
          width: parent.width
          spacing: Style.space(14)
          Item {
            width: Style.space(76); height: width
            Canvas {
              id: donut
              anchors.fill: parent
              property var data: sideCard.routeRows
              property var lit: root.probe && root.probe.dim === "group" ? root.probe.value : ""
              onDataChanged: requestPaint()
              onLitChanged: requestPaint()
              onPaint: {
                var ctx = getContext("2d")
                ctx.reset()
                var cx = width / 2, cy = height / 2, r = width / 2 - 1, w = Style.space(9)
                ctx.lineWidth = w
                ctx.strokeStyle = Util.alpha(Color.popups.text, 0.08)
                ctx.beginPath(); ctx.arc(cx, cy, r - w / 2, 0, Math.PI * 2); ctx.stroke()
                var a = -Math.PI / 2
                for (var i = 0; i < data.length; i++) {
                  var part = (data[i].up + data[i].down) / Math.max(sideCard.routeTotal, 1)
                  if (part <= 0) continue
                  var c = root.routeColor(i, data[i].key)
                  ctx.strokeStyle = lit !== "" && lit !== data[i].key ? Qt.rgba(c.r, c.g, c.b, 0.25) : c
                  ctx.lineWidth = lit === data[i].key ? w + Style.space(3) : w
                  ctx.beginPath(); ctx.arc(cx, cy, r - w / 2 - (lit === data[i].key ? 0 : 1), a, a + part * Math.PI * 2 - 0.03); ctx.stroke()
                  a += part * Math.PI * 2
                }
              }
            }
            Column {
              anchors.centerIn: parent
              Txt { anchors.horizontalCenter: parent.horizontalCenter; text: Math.round(sideCard.proxied * 100) + "%"; bold: true }
              Txt { anchors.horizontalCenter: parent.horizontalCenter; text: I18n.t("trafficProxied"); faint: true; font.pixelSize: Style.font.caption - 1 }
            }
          }
          Column {
            width: parent.width - Style.space(90)
            anchors.verticalCenter: parent.verticalCenter
            spacing: Style.space(4)
            Repeater {
              model: sideCard.routeRows
              Item {
                required property var modelData
                required property int index
                width: parent.width; height: rl.implicitHeight
                Rectangle { width: Style.space(8); height: width; radius: 2; color: root.routeColor(index, modelData.key); anchors.verticalCenter: parent.verticalCenter }
                readonly property bool lit: root.probe !== null && root.probe.dim === "group" && root.probe.value === modelData.key
                Txt { id: rl; x: Style.space(14); width: parent.width - x - rp.width - Style.space(6); text: modelData.key; dim: !parent.lit; bold: parent.lit; font.pixelSize: Style.font.caption }
                Txt { id: rp; anchors.right: parent.right; text: TM.percent(modelData.up + modelData.down, sideCard.routeTotal); font.pixelSize: Style.font.caption }
                MouseArea {
                  objectName: "trafficRouteItem"
                  anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor
                  onContainsMouseChanged: root.setProbe({ dim: "group", value: modelData.key, label: modelData.key, color: root.routeColor(index, modelData.key) }, containsMouse)
                  onClicked: root.filters = TM.addFilter(root.filters, { dim: "group", value: modelData.key, exclude: false })
                }
              }
            }
          }
        }
        Rectangle { width: parent.width; height: 1; color: Util.alpha(Color.popups.text, 0.12) }

        // Short ranges: top countries. Long ones: weekday × hour.
        Item {
          width: parent.width; height: sideHead.implicitHeight
          Txt { id: sideHead; text: root.longRange ? I18n.t("trafficHeat") : I18n.t("trafficCountries"); faint: true; bold: true; font.pixelSize: Style.font.caption; font.letterSpacing: 1 }
          Txt {
            anchors.right: parent.right
            visible: !root.longRange && root.status !== null && !root.status.country
            text: I18n.t("trafficNoCountryDb"); faint: true; font.pixelSize: Style.font.caption
          }
          Txt {
            anchors.right: parent.right
            visible: root.longRange && heat.hovered >= 0
            text: heat.hovered < 0 ? "" : Qt.locale(I18n.language === "en" ? "en_US" : "ru_RU").dayName((Math.floor(heat.hovered / 24) + 1) % 7, Locale.ShortFormat)
              + " " + (heat.hovered % 24) + ":00 · " + root.svc.formatBytes(heat.grid[Math.floor(heat.hovered / 24)][heat.hovered % 24])
            dim: true; font.pixelSize: Style.font.caption
          }
        }
        Column {
          id: countryCol
          visible: !root.longRange
          width: parent.width
          spacing: Style.space(5)
          readonly property var list: root.longRange ? [] : (root.focusCountries && root.litKey !== "" ? root.focusCountries.rows : (root.side ? root.side.rows : []))
          Repeater {
            model: countryCol.list
            Item {
              required property var modelData
              readonly property double best: countryCol.list.length ? countryCol.list[0].up + countryCol.list[0].down : 1
              width: parent.width; height: Style.space(16)
              Rectangle {
                width: Style.space(24); height: Style.space(14); radius: 3
                color: Style.hoverFillFor(Color.popups.text, Color.accent)
                anchors.verticalCenter: parent.verticalCenter
                Txt { anchors.centerIn: parent; text: modelData.key || "—"; dim: true; bold: true; font.pixelSize: Style.font.caption - 1 }
              }
              Item {
                x: Style.space(32); width: parent.width - x - cv.width - Style.space(8); height: Style.space(5)
                anchors.verticalCenter: parent.verticalCenter
                Rectangle { anchors.fill: parent; radius: 3; color: Util.alpha(Color.popups.text, 0.06) }
                Rectangle { width: Math.max(3, parent.width * (modelData.up + modelData.down) / Math.max(parent.parent.best, 1)); height: parent.height; radius: 3; color: Util.alpha(Color.accent, 0.7) }
              }
              Txt { id: cv; anchors.right: parent.right; width: Style.space(58); horizontalAlignment: Text.AlignRight; text: root.svc.formatBytes(modelData.up + modelData.down); font.pixelSize: Style.font.caption }
              MouseArea {
                anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor
                onContainsMouseChanged: root.setProbe({ dim: "country", value: modelData.key, label: modelData.key || "—", color: Color.accent }, containsMouse)
                onClicked: root.filters = TM.addFilter(root.filters, { dim: "country", value: modelData.key, exclude: false })
              }
            }
          }
        }
        Column {
          id: heat
          objectName: "trafficHeatmap"
          visible: root.longRange
          spacing: 1
          readonly property var grid: root.longRange && root.side ? TM.heatmap(root.side.all, root.side.from, root.side.step) : []
          readonly property double peak: {
            var m = 0
            for (var d = 0; d < grid.length; d++) m = Math.max(m, TM.maxOf(grid[d]))
            return m || 1
          }
          property int hovered: -1
          // The chart's hovered bucket, as a cell.
          readonly property int chartCell: {
            if (chart.hoverMs < 0) return -1
            var d = new Date(chart.hoverMs)
            return ((d.getDay() + 6) % 7) * 24 + d.getHours()
          }
          readonly property real cell: Math.floor((parent.width - Style.space(20)) / 24) - 1
          Repeater {
            model: heat.grid.length
            Row {
              required property int index
              readonly property int day: index
              spacing: 1
              Txt { width: Style.space(20); text: Qt.locale(I18n.language === "en" ? "en_US" : "ru_RU").dayName((day + 1) % 7, Locale.ShortFormat); faint: true; font.pixelSize: Style.font.caption - 1 }
              Repeater {
                model: 24
                Rectangle {
                  required property int index
                  readonly property double v: heat.grid[parent.day][index]
                  width: heat.cell; height: heat.cell; radius: 2
                  color: Util.alpha(root.svc.peach, 0.06 + 0.9 * v / heat.peak)
                  border.width: heat.hovered === parent.day * 24 + index || heat.chartCell === parent.day * 24 + index ? 1 : 0
                  border.color: Color.popups.text
                  HoverHandler {
                    onHoveredChanged: {
                      var cell = parent.parent.day * 24 + index
                      if (hovered) { heat.hovered = cell; root.heatDay = parent.parent.day; root.heatHour = index }
                      else if (heat.hovered === cell) { heat.hovered = -1; root.heatDay = -1; root.heatHour = -1 }
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }

  function routeColor(i, key) {
    if (key === "DIRECT") return svc.green
    if (routes) for (var j = 0; j < routes.rows.length; j++) if (routes.rows[j].key === key) { i = j; break }
    var c = [Color.accent, svc.peach, svc.mauve, svc.yellow]
    return c[i % c.length]
  }

  ActionMenu {
    id: rowMenu
    objectName: "trafficMenu"
    property var row: null
    ActionMenuItem {
      enabled: rowMenu.row !== null && TM.next(root.by) !== ""
      glyph: Icons.search
      text: I18n.t("trafficMenuDrill").arg(I18n.t("trafficDims_" + (TM.next(root.by) || root.by)).toLowerCase())
      keyHint: "Enter"
      onTriggered: root.drill(rowMenu.row)
    }
    ActionMenuItem { glyph: Icons.check; text: I18n.t("trafficMenuOnly"); keyHint: "O"; onTriggered: root.filterBy(rowMenu.row, false) }
    ActionMenuItem { glyph: Icons.hide; text: I18n.t("trafficMenuExclude"); keyHint: "X"; onTriggered: root.filterBy(rowMenu.row, true) }
    ActionMenuSeparator {}
    ActionMenuItem { enabled: root.canRule(rowMenu.row); glyph: Icons.ruleAdd; text: I18n.t("trafficMenuRule"); keyHint: "R"; onTriggered: root.ruleFor(rowMenu.row) }
    ActionMenuItem { enabled: rowMenu.row !== null && rowMenu.row.key !== ""; glyph: Icons.copy; text: I18n.t("trafficMenuCopy"); onTriggered: root.copy(rowMenu.row) }
  }
}
