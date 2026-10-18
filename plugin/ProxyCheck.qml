import QtQuick
import QtQuick.Controls
import qs.Commons

CheckBox {
  id: control
  font.family: Style.font.family
  font.pixelSize: Style.font.body
  implicitHeight: Style.space(28)
  indicator: Rectangle {
    x: control.leftPadding
    y: (control.height - height) / 2
    width: Style.space(15)
    height: width
    radius: 3
    color: control.checked ? Color.accent : Util.alpha(Color.popups.text, 0.04)
    border.width: 1
    border.color: control.checked ? Color.accent : Util.alpha(Color.popups.text, 0.3)
    Txt {
      anchors.centerIn: parent
      text: control.checked ? "✓" : ""
      color: Color.background
      font.pixelSize: Style.font.caption
    }
  }
  contentItem: Txt {
    text: control.text
    leftPadding: control.indicator.width + Style.space(8)
    verticalAlignment: Text.AlignVCenter
    opacity: control.enabled ? 1 : 0.45
  }
}
