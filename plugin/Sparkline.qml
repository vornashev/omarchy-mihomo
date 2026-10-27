import QtQuick
import qs.Commons

// A node's delay over time as thin bars: taller is slower, a full red bar
// is a period where every test failed, a gap is one without tests.
Canvas {
  id: root
  // Mean delay per period: null, 0 (failed) or ms.
  property var points: []
  property color color: Color.accent
  property color failColor: Color.urgent
  implicitWidth: Style.space(48)
  implicitHeight: Style.space(14)
  renderStrategy: Canvas.Cooperative
  onPointsChanged: requestPaint()
  onColorChanged: requestPaint()
  onPaint: {
    var ctx = getContext("2d")
    ctx.reset()
    var p = points || []
    if (!p.length) return
    var max = 0
    for (var i = 0; i < p.length; i++) if (p[i] > max) max = p[i]
    var step = width / p.length
    var bar = Math.max(1, step - 1)
    for (var j = 0; j < p.length; j++) {
      var v = p[j]
      if (v === null || v === undefined) continue
      // At least a sliver, so a fast node still shows it was measured.
      var h = v === 0 ? height : Math.max(2, height * v / Math.max(max, 1))
      ctx.fillStyle = v === 0 ? root.failColor : root.color
      ctx.globalAlpha = v === 0 ? 0.9 : 0.75
      ctx.fillRect(j * step, height - h, bar, h)
    }
  }
}
