import QtQuick
import qs.Commons
import qs.Commons as Commons

// Small tag: colored dot + label.
Rectangle {
  id: root
  property string text: ""
  property color dot: Commons.Color.muted
  property bool showDot: true
  implicitWidth: row.implicitWidth + Style.space(12)
  implicitHeight: label.implicitHeight + Style.space(4)
  radius: Style.cornerRadius
  color: Util.alpha(Commons.Color.popups.text, 0.04)
  border.width: 1
  border.color: Util.alpha(Commons.Color.popups.text, 0.15)

  Row {
    id: row
    anchors.centerIn: parent
    spacing: Style.space(5)
    Rectangle {
      visible: root.showDot
      width: Style.space(6); height: width; radius: width / 2
      anchors.verticalCenter: parent.verticalCenter
      color: root.dot
    }
    Txt { id: label; text: root.text; dim: true; font.pixelSize: Style.font.bodySmall }
  }
}
