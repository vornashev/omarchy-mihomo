import QtQuick
import qs.Commons

// Segmented control. `model` is [{key, label}]; emits picked(key).
Rectangle {
  id: root
  property var model: []
  property string current: ""
  signal picked(string key)

  implicitWidth: row.implicitWidth + Style.space(4)
  implicitHeight: row.implicitHeight + Style.space(4)
  radius: Style.cornerRadius
  color: Util.alpha(Color.popups.text, 0.04)

  Row {
    id: row
    anchors.centerIn: parent
    spacing: Style.space(2)
    Repeater {
      model: root.model
      Rectangle {
        required property var modelData
        readonly property bool on: modelData.key === root.current
        implicitWidth: t.implicitWidth + Style.space(18)
        implicitHeight: t.implicitHeight + Style.space(9)
        radius: Style.cornerRadius
        color: on ? Style.selectedFillFor(Color.popups.text, Color.accent) : (ma.containsMouse ? Style.hoverFillFor(Color.popups.text, Color.accent) : "transparent")
        Txt { id: t; anchors.centerIn: parent; text: modelData.label; dim: !parent.on; bold: parent.on }
        MouseArea { id: ma; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.picked(modelData.key) }
      }
    }
  }
}
