import QtQuick
import qs.Commons
import qs.Commons as Commons

// Segmented control. `model` is [{key, label}]; emits picked(key).
Rectangle {
  id: root
  property var model: []
  property string current: ""
  // Options share the given width equally instead of fitting their labels.
  property bool stretch: false
  signal picked(string key)

  implicitWidth: row.implicitWidth + Style.space(4)
  implicitHeight: row.implicitHeight + Style.space(4)
  radius: Style.cornerRadius
  color: Util.alpha(Commons.Color.popups.text, 0.04)

  Row {
    id: row
    anchors.centerIn: parent
    spacing: Style.space(2)
    Repeater {
      model: root.model
      Rectangle {
        required property var modelData
        readonly property bool on: modelData.key === root.current
        implicitWidth: root.stretch ? (root.width - Style.space(4) - row.spacing * (root.model.length - 1)) / root.model.length : t.implicitWidth + Style.space(18)
        implicitHeight: t.implicitHeight + Style.space(9)
        radius: Style.cornerRadius
        color: on ? Style.selectedFillFor(Commons.Color.popups.text, Commons.Color.accent) : (ma.containsMouse ? Style.hoverFillFor(Commons.Color.popups.text, Commons.Color.accent) : "transparent")
        Txt { id: t; anchors.centerIn: parent; text: modelData.label; dim: !parent.on; bold: parent.on }
        MouseArea { id: ma; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.picked(modelData.key) }
      }
    }
  }
}
