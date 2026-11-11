import QtQuick
import qs.Commons
import qs.Commons as Commons

Rectangle {
  id: root
  property bool checked: false
  property bool compact: false
  readonly property real inset: Style.space(compact ? 2 : 3)
  signal toggled()
  implicitWidth: Style.space(compact ? 26 : 32)
  implicitHeight: Style.space(compact ? 14 : 18)
  radius: height / 2
  color: checked ? Commons.Color.accent : Util.alpha(Commons.Color.popups.text, 0.18)
  Rectangle {
    width: parent.height - 2 * root.inset; height: width; radius: width / 2
    anchors.verticalCenter: parent.verticalCenter
    x: root.checked ? parent.width - width - root.inset : root.inset
    color: Commons.Color.background
    Behavior on x { NumberAnimation { duration: 120 } }
  }
  MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.toggled() }
}
