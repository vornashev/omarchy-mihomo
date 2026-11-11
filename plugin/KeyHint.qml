import QtQuick
import qs.Commons
import qs.Commons as Commons

// A key cap with what it does: "esc Закрыть".
Row {
  id: root
  property string key: ""
  property string label: ""
  spacing: Style.space(5)
  Rectangle {
    anchors.verticalCenter: parent.verticalCenter
    width: k.implicitWidth + Style.space(8); height: k.implicitHeight + Style.space(2)
    radius: Style.space(3)
    color: "transparent"
    border.width: 1
    border.color: Util.alpha(Commons.Color.popups.text, 0.18)
    Txt { id: k; anchors.centerIn: parent; text: root.key; dim: true; font.pixelSize: Style.font.bodySmall }
  }
  Txt { visible: root.label !== ""; anchors.verticalCenter: parent.verticalCenter; text: root.label; faint: true; font.pixelSize: Style.font.bodySmall }
}
