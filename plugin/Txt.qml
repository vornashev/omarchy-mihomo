import QtQuick
import qs.Commons
import qs.Commons as Commons

// Text with the panel's font and three emphasis levels.
Text {
  property bool dim: false
  property bool faint: false
  property bool bold: false
  property bool mono: true
  color: faint ? Util.alpha(Commons.Color.popups.text, 0.42) : (dim ? Util.alpha(Commons.Color.popups.text, 0.68) : Commons.Color.popups.text)
  font.family: Style.font.family
  font.pixelSize: Style.font.body
  font.bold: bold
  elide: Text.ElideRight
  verticalAlignment: Text.AlignVCenter
}
