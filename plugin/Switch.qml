import QtQuick
import qs.Commons

Rectangle {
  id: root
  property bool checked: false
  signal toggled()
  implicitWidth: Style.space(32)
  implicitHeight: Style.space(18)
  radius: height / 2
  color: checked ? Color.accent : Util.alpha(Color.popups.text, 0.18)
  Rectangle {
    width: parent.height - Style.space(6); height: width; radius: width / 2
    anchors.verticalCenter: parent.verticalCenter
    x: root.checked ? parent.width - width - Style.space(3) : Style.space(3)
    color: Color.background
    Behavior on x { NumberAnimation { duration: 120 } }
  }
  MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.toggled() }
}
