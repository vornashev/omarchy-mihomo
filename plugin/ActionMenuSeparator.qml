import QtQuick
import QtQuick.Controls
import qs.Commons

MenuSeparator {
  padding: 0
  leftPadding: Style.space(4)
  rightPadding: Style.space(4)
  implicitHeight: Style.space(9)
  contentItem: Item {
    Rectangle {
      anchors.verticalCenter: parent.verticalCenter
      width: parent.width
      height: 1
      color: Util.alpha(Color.popups.text, 0.15)
    }
  }
}
