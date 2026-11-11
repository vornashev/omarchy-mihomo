import QtQuick
import qs.Commons
import qs.Commons as Commons
import "TrafficModels.js" as TM

// Stacked areas over time: download above the axis, upload mirrored below
// (on its own scale). Hover shows the values at a point; dragging selects
// a span to zoom into; the wheel zooms around the pointer and Shift+wheel
// (or a sideways swipe) pans. Points sit on a time axis, so a new window
// shows at once with the buckets at hand while finer ones load.
Item {
  id: root
  property var svc
  // [{key, label, color, upColor?, up: [], down: []}], bottom of the stack first.
  property var series: []
  property double from: 0
  property double step: 60000
  property string metric: "both"
  property var hidden: ({})
  // Formats a bucket start for the axis and the tooltip.
  property var timeLabel: function(ms, long) { return "" }
  // Key of the series to bring forward; the rest fade.
  property string focusKey: ""
  // A slice drawn over the stack, aligned to the same buckets:
  // {label, color, up: [], down: []}, e.g. "through AI".
  property var overlay: null
  // Weekday (Monday 0) and hour to shade, from the heatmap; -1 for none.
  property int bandDay: -1
  property int bandHour: -1
  // Bucket start under the pointer, or -1.
  readonly property double hoverMs: hover >= 0 && hover < count ? from + hover * step : -1
  // Series under the pointer.
  property string hoveredKey: ""
  // Time window to show, ms; 0 for the data's own extent.
  property double viewFrom: 0
  property double viewTo: 0
  readonly property double vFrom: viewTo > viewFrom ? viewFrom : from
  readonly property double vTo: viewTo > viewFrom ? viewTo : from + Math.max(1, count - 1) * step
  signal zoomed(double from, double to)

  readonly property int count: series.length ? series[0].down.length : 0
  readonly property real axisWidth: Style.space(72)
  readonly property real plotX: axisWidth
  readonly property real plotW: Math.max(1, width - axisWidth)
  readonly property real plotH: Math.max(1, height - Style.space(18))
  readonly property bool showDown: metric !== "up"
  readonly property bool showUp: metric !== "down"
  // Share of the height above the axis.
  readonly property real split: showDown && showUp ? 0.72 : (showDown ? 1 : 0)
  readonly property real baseY: plotH * split
  readonly property var downStack: TM.stack(series, "down", hidden)
  readonly property var upStack: TM.stack(series, "up", hidden)
  readonly property real downMax: TM.niceMax(TM.rate(downStack.length ? TM.maxOf(downStack[downStack.length - 1].top) : 0, step) * 1.05)
  readonly property real upMax: TM.niceMax(TM.rate(upStack.length ? TM.maxOf(upStack[upStack.length - 1].top) : 0, step) * 1.05)
  property int hover: -1
  property real dragFrom: -1
  property real dragTo: -1

  function xOf(ms) { return plotX + (ms - vFrom) * plotW / Math.max(1, vTo - vFrom) }
  function timeAt(x) { return vFrom + (x - plotX) * (vTo - vFrom) / plotW }
  function xAt(i) { return count > 1 || viewTo > viewFrom ? xOf(from + i * step) : plotX + plotW / 2 }
  // Bucket under `x`; -1 off the data unless `clamp`.
  function indexAt(x, clamp) {
    if (count === 0) return -1
    var i = Math.round((timeAt(x) - from) / step)
    if (clamp) return Math.max(0, Math.min(count - 1, i))
    return i >= 0 && i < count ? i : -1
  }

  // One wheel event: zoom around the pointer, or pan with Shift or a
  // sideways swipe. Emits the new window.
  function wheel(x, dy, dx, shift) {
    var span = vTo - vFrom
    var pan = shift ? (dy || dx) : dx
    var w = TM.zoomWindow(vFrom, vTo, timeAt(x), shift || dy === 0 ? 1 : Math.pow(1.25, -dy / 120), -pan / 120 * span * 0.1, Date.now())
    if (w.from !== Math.round(vFrom) || w.to !== Math.round(vTo)) zoomed(w.from, w.to)
  }
  function yDown(bytes) { return baseY - (baseY - Style.space(4)) * TM.rate(bytes, step) / downMax }
  function yUp(bytes) { return baseY + (plotH - baseY - Style.space(2)) * TM.rate(bytes, step) / upMax }

  onSeriesChanged: canvas.requestPaint()
  onHiddenChanged: canvas.requestPaint()
  onMetricChanged: canvas.requestPaint()
  onWidthChanged: canvas.requestPaint()
  onHeightChanged: canvas.requestPaint()
  onHoverChanged: canvas.requestPaint()
  onFocusKeyChanged: canvas.requestPaint()
  onOverlayChanged: canvas.requestPaint()
  onBandDayChanged: canvas.requestPaint()
  onBandHourChanged: canvas.requestPaint()
  onVFromChanged: canvas.requestPaint()
  onVToChanged: canvas.requestPaint()

  // The stacked series at height `y` of bucket `i`.
  function keyAt(i, y) {
    if (i < 0 || i >= count) return ""
    for (var s = series.length - 1; s >= 0; s--) {
      if (hidden[series[s].key]) continue
      if (showDown && y <= baseY) {
        var top = yDown(downStack[s].top[i]), bottom = yDown(downStack[s].bottom[i])
        if (y >= top - 1 && y <= bottom && bottom - top > 0.5) return series[s].key
      } else if (showUp && y > baseY) {
        var t2 = yUp(upStack[s].bottom[i]), b2 = yUp(upStack[s].top[i])
        if (y >= t2 && y <= b2 + 1 && b2 - t2 > 0.5) return series[s].key
      }
    }
    return ""
  }

  // Buckets that cover the heatmap's weekday and hour.
  function inBand(i) {
    if (bandDay < 0) return false
    for (var t = from + i * step; t < from + (i + 1) * step; t += 3600000) {
      var d = new Date(t)
      if ((d.getDay() + 6) % 7 === bandDay && d.getHours() === bandHour) return true
    }
    return false
  }

  Canvas {
    id: canvas
    anchors.fill: parent
    renderStrategy: Canvas.Cooperative
    onPaint: {
      var ctx = getContext("2d")
      ctx.reset()
      var text = Commons.Color.popups.text
      var grid = Qt.rgba(text.r, text.g, text.b, 0.07)
      ctx.font = Style.font.bodySmall + "px \"" + Style.font.family + "\""
      ctx.textBaseline = "middle"
      ctx.textAlign = "right"
      ctx.fillStyle = Qt.rgba(text.r, text.g, text.b, 0.42)
      // Grid and the axis scale.
      if (root.showDown) {
        for (var g = 0; g <= 2; g++) {
          var y = root.baseY - (root.baseY - Style.space(4)) * g / 2
          ctx.fillStyle = grid
          ctx.fillRect(root.plotX, Math.round(y), root.plotW, 1)
          ctx.fillStyle = Qt.rgba(text.r, text.g, text.b, 0.42)
          if (g > 0) ctx.fillText((root.showUp ? "↓ " : "") + root.svc.formatRate(root.downMax * g / 2), root.plotX - Style.space(6), Math.max(y, Style.font.bodySmall))
        }
      }
      if (root.showUp) {
        var yu = root.yUp(root.upMax * root.step / 1000)
        ctx.fillStyle = grid
        ctx.fillRect(root.plotX, Math.round(yu), root.plotW, 1)
        ctx.fillStyle = Qt.rgba(text.r, text.g, text.b, 0.42)
        ctx.fillText("↑ " + root.svc.formatRate(root.upMax), root.plotX - Style.space(6), Math.min(yu, root.plotH - Style.font.bodySmall / 2))
      }
      ctx.fillStyle = Qt.rgba(text.r, text.g, text.b, 0.25)
      ctx.fillRect(root.plotX, Math.round(root.baseY), root.plotW, 1)
      if (root.count === 0) return
      // A window wider than the data leaves the rest blank, not smeared.
      ctx.save()
      ctx.beginPath()
      ctx.rect(root.plotX, 0, root.plotW, root.plotH + 1)
      ctx.clip()

      // Heatmap hour, every day it falls on.
      if (root.bandDay >= 0 && root.count > 1) {
        var half = root.step * root.plotW / Math.max(1, root.vTo - root.vFrom) / 2
        ctx.fillStyle = Qt.rgba(text.r, text.g, text.b, 0.08)
        for (var bi = 0; bi < root.count; bi++) if (root.inBand(bi)) ctx.fillRect(root.xAt(bi) - half, 0, half * 2, root.plotH)
      }

      function area(stack, color, yOf, key) {
        var faded = root.focusKey !== "" && root.focusKey !== key
        var lifted = root.focusKey !== "" && root.focusKey === key
        ctx.beginPath()
        ctx.moveTo(root.xAt(0), yOf(stack.top[0]))
        for (var i = 1; i < root.count; i++) ctx.lineTo(root.xAt(i), yOf(stack.top[i]))
        for (var j = root.count - 1; j >= 0; j--) ctx.lineTo(root.xAt(j), yOf(stack.bottom[j]))
        ctx.closePath()
        ctx.fillStyle = Qt.rgba(color.r, color.g, color.b, faded ? 0.1 : (lifted ? 0.75 : (root.overlay ? 0.25 : 0.5)))
        ctx.fill()
        ctx.beginPath()
        ctx.moveTo(root.xAt(0), yOf(stack.top[0]))
        for (var k = 1; k < root.count; k++) ctx.lineTo(root.xAt(k), yOf(stack.top[k]))
        ctx.strokeStyle = Qt.rgba(color.r, color.g, color.b, faded ? 0.25 : 0.95)
        ctx.lineWidth = lifted ? 2 : 1.2
        ctx.stroke()
      }
      for (var s = 0; s < root.series.length; s++) {
        if (root.hidden[root.series[s].key]) continue
        if (root.showDown) area(root.downStack[s], root.series[s].color, root.yDown, root.series[s].key)
        if (root.showUp) area(root.upStack[s], root.series[s].upColor || Qt.darker(root.series[s].color, root.showDown ? 1.25 : 1), root.yUp, root.series[s].key)
      }

      // The probed slice: how much of the stack it accounts for.
      function slice(values, yOf, color) {
        ctx.beginPath()
        ctx.moveTo(root.xAt(0), root.baseY)
        for (var i = 0; i < root.count; i++) ctx.lineTo(root.xAt(i), yOf(values[i] || 0))
        ctx.lineTo(root.xAt(root.count - 1), root.baseY)
        ctx.closePath()
        ctx.fillStyle = Qt.rgba(color.r, color.g, color.b, 0.35)
        ctx.fill()
        ctx.beginPath()
        for (var j = 0; j < root.count; j++) { if (j) ctx.lineTo(root.xAt(j), yOf(values[j] || 0)); else ctx.moveTo(root.xAt(j), yOf(values[j] || 0)) }
        ctx.strokeStyle = color
        ctx.lineWidth = 2
        ctx.stroke()
      }
      if (root.overlay && root.overlay.down && root.overlay.down.length === root.count) {
        if (root.showDown) slice(root.overlay.down, root.yDown, root.overlay.color)
        if (root.showUp) slice(root.overlay.up, root.yUp, root.overlay.color)
      }

      if (root.dragFrom >= 0 && root.dragTo >= 0) {
        var a = Math.min(root.dragFrom, root.dragTo), b = Math.max(root.dragFrom, root.dragTo)
        ctx.fillStyle = Qt.rgba(Commons.Color.accent.r, Commons.Color.accent.g, Commons.Color.accent.b, 0.15)
        ctx.fillRect(a, 0, b - a, root.plotH)
      }
      if (root.hover >= 0 && root.hover < root.count) {
        var hx = Math.round(root.xAt(root.hover))
        ctx.fillStyle = Qt.rgba(text.r, text.g, text.b, 0.6)
        ctx.fillRect(hx, 0, 1, root.plotH)
        for (var d = 0; d < root.series.length; d++) {
          if (root.hidden[root.series[d].key] || !root.showDown) continue
          var top = root.downStack[d].top[root.hover]
          if (top === root.downStack[d].bottom[root.hover]) continue
          ctx.beginPath()
          ctx.arc(hx, root.yDown(top), 3, 0, Math.PI * 2)
          ctx.fillStyle = root.series[d].color
          ctx.fill()
          ctx.lineWidth = 1.5
          ctx.strokeStyle = Commons.Color.popups.background
          ctx.stroke()
        }
      }
      ctx.restore()
    }
  }

  // Time axis: about six labels.
  Repeater {
    model: root.count > 1 ? 6 : 0
    Txt {
      required property int index
      readonly property double at: root.vFrom + index * (root.vTo - root.vFrom) / 5
      x: Math.max(root.plotX, Math.min(root.width - width, root.xOf(at) - width / 2))
      y: root.plotH + Style.space(4)
      text: root.timeLabel(at, false)
      faint: true; font.pixelSize: Style.font.bodySmall
    }
  }

  MouseArea {
    id: area
    x: root.plotX; width: root.plotW; height: root.plotH
    hoverEnabled: true
    cursorShape: Qt.CrossCursor
    onPositionChanged: function(mouse) {
      root.hover = root.indexAt(mouse.x + root.plotX, false)
      root.hoveredKey = pressed ? "" : root.keyAt(root.hover, mouse.y)
      if (pressed) { root.dragTo = mouse.x + root.plotX; canvas.requestPaint() }
    }
    onExited: { root.hover = -1; root.hoveredKey = "" }
    onWheel: function(wheel) {
      root.wheel(wheel.x + root.plotX, wheel.angleDelta.y, wheel.angleDelta.x, (wheel.modifiers & Qt.ShiftModifier) !== 0)
      wheel.accepted = true
    }
    onPressed: function(mouse) { root.dragFrom = mouse.x + root.plotX; root.dragTo = -1 }
    onReleased: function(mouse) {
      var a = root.dragFrom, b = mouse.x + root.plotX
      root.dragFrom = -1; root.dragTo = -1
      canvas.requestPaint()
      if (Math.abs(b - a) < Style.space(8)) return
      var i = root.indexAt(Math.min(a, b), true), j = root.indexAt(Math.max(a, b), true)
      if (j > i) root.zoomed(root.from + i * root.step, root.from + (j + 1) * root.step)
    }
  }

  Rectangle {
    id: tip
    objectName: "trafficTooltip"
    visible: root.hover >= 0 && root.hover < root.count && !area.pressed
    readonly property real anchorX: root.xAt(Math.max(0, root.hover))
    x: anchorX + Style.space(12) + width > root.width ? anchorX - width - Style.space(12) : anchorX + Style.space(12)
    y: Style.space(2)
    width: Style.space(260)
    height: tipCol.implicitHeight + Style.space(12)
    radius: 4
    color: Qt.lighter(Commons.Color.popups.background, 1.35)
    border.width: 1
    border.color: Util.alpha(Commons.Color.popups.text, 0.15)
    Column {
      id: tipCol
      x: Style.space(9); y: Style.space(6)
      width: tip.width - Style.space(18)
      spacing: Style.space(3)
      Txt { id: tipTime; width: parent.width; text: root.hover >= 0 ? root.timeLabel(root.from + root.hover * root.step, true) : ""; bold: true; font.pixelSize: Style.font.body }
      Txt {
        width: parent.width
        text: {
          if (root.hover < 0) return ""
          var d = 0, u = 0
          for (var s = 0; s < root.series.length; s++) if (!root.hidden[root.series[s].key]) { d += root.series[s].down[root.hover] || 0; u += root.series[s].up[root.hover] || 0 }
          return (root.showDown ? "↓ " + root.svc.formatRate(TM.rate(d, root.step)) : "") + (root.showDown && root.showUp ? "   " : "") + (root.showUp ? "↑ " + root.svc.formatRate(TM.rate(u, root.step)) : "")
        }
        font.pixelSize: Style.font.body
      }
      Item {
        visible: root.overlay !== null && root.hover >= 0
        width: tipCol.width; height: visible ? ol.implicitHeight : 0
        Rectangle { width: Style.space(6); height: 2; color: root.overlay ? root.overlay.color : "transparent"; anchors.verticalCenter: parent.verticalCenter }
        Txt { id: ol; x: Style.space(12); width: parent.width - x - olv.implicitWidth - Style.space(8); text: root.overlay ? root.overlay.label : ""; bold: true; font.pixelSize: Style.font.bodySmall }
        Txt {
          id: olv
          anchors.right: parent.right
          text: root.overlay && root.hover >= 0 ? (root.showDown ? root.svc.formatRate(TM.rate(root.overlay.down[root.hover] || 0, root.step)) : "") + (root.showDown && root.showUp ? " · " : "") + (root.showUp ? root.svc.formatRate(TM.rate(root.overlay.up[root.hover] || 0, root.step)) : "") : ""
          bold: true; font.pixelSize: Style.font.bodySmall
        }
      }
      Repeater {
        model: root.hover >= 0 ? root.series.slice().reverse().filter(function(s) { return !root.hidden[s.key] }) : []
        Item {
          required property var modelData
          width: tipCol.width; height: tn.implicitHeight
          Rectangle { width: Style.space(6); height: width; radius: 1; color: modelData.color; anchors.verticalCenter: parent.verticalCenter }
          Txt { id: tn; x: Style.space(12); width: parent.width - x - tv.implicitWidth - Style.space(8); text: modelData.label; dim: modelData.key !== root.focusKey; bold: modelData.key === root.focusKey; font.pixelSize: Style.font.bodySmall }
          Txt {
            id: tv
            anchors.right: parent.right
            text: (root.showDown ? root.svc.formatRate(TM.rate(modelData.down[root.hover] || 0, root.step)) : "") + (root.showDown && root.showUp ? " · " : "") + (root.showUp ? root.svc.formatRate(TM.rate(modelData.up[root.hover] || 0, root.step)) : "")
            font.pixelSize: Style.font.bodySmall
          }
        }
      }
      Txt {
        objectName: "trafficZoomHint"
        width: tipCol.width
        topPadding: Style.space(2)
        text: MihomoI18n.t("trafficZoomHint")
        faint: true; wrapMode: Text.WordWrap; font.pixelSize: Style.font.caption
      }
    }
  }
}
