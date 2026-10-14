import QtQuick
import qs.Commons

// Text with the panel's font and three emphasis levels.
Text {
  property bool dim: false
  property bool faint: false
  property bool bold: false
  property bool mono: true
  color: faint ? Util.alpha(Color.popups.text, 0.42) : (dim ? Util.alpha(Color.popups.text, 0.68) : Color.popups.text)
  font.family: Style.font.family
  font.pixelSize: Style.font.body
  font.bold: bold
  elide: Text.ElideRight
  verticalAlignment: Text.AlignVCenter
}
