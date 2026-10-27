import QtQuick
import qs.Commons

Rectangle {
  id: root
  property string icon: ""
  property string label: ""
  property string keyHint: ""
  property int badge: 0
  property bool selected: false
  signal clicked()
  Hint {
    visible: ma.containsMouse && !ma.pressed
    text: root.label
    keyHint: root.keyHint
  }

  implicitHeight: Style.space(30)
  radius: Style.cornerRadius
  color: selected ? Style.selectedFillFor(Color.popups.text, Color.accent) : (ma.containsMouse ? Style.hoverFillFor(Color.popups.text, Color.accent) : "transparent")

  Row {
    anchors.fill: parent
    anchors.leftMargin: Style.space(10)
    anchors.rightMargin: Style.space(10)
    spacing: Style.space(10)
    Txt { anchors.verticalCenter: parent.verticalCenter; text: root.icon; dim: !root.selected; font.pixelSize: Style.font.icon; width: Style.space(16) }
    Txt {
      anchors.verticalCenter: parent.verticalCenter
      text: root.label; dim: !root.selected; bold: root.selected
      width: parent.width - Style.space(16) - Style.space(20) - badgeBox.width
    }
    Item {
      id: badgeBox
      anchors.verticalCenter: parent.verticalCenter
      width: Math.max(badgeRect.width, Style.space(10)); height: parent.height
      Rectangle {
        id: badgeRect
        visible: root.badge > 0
        anchors.centerIn: parent
        width: Math.max(height, badgeText.implicitWidth + Style.space(8)); height: badgeText.implicitHeight + Style.space(2)
        radius: height / 2
        color: Color.urgent
        Txt { id: badgeText; anchors.centerIn: parent; text: root.badge > 99 ? "99+" : String(root.badge); color: Color.background; bold: true; font.pixelSize: Style.font.caption }
      }
      Txt { visible: root.badge <= 0; anchors.centerIn: parent; text: root.keyHint; faint: true; font.pixelSize: Style.font.caption }
    }
  }
  MouseArea { id: ma; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.clicked() }
}
