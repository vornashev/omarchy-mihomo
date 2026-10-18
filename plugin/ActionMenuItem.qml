import QtQuick
import QtQuick.Controls
import qs.Commons

MenuItem {
  id: root
  property string glyph: ""
  property string keyHint: ""
  property string hint: ""
  property bool dangerous: false
  property bool radio: false
  property bool marked: false
  property string detail: ""
  property color detailColor: Util.alpha(Color.popups.text, 0.68)
  implicitHeight: Style.space(28) + (hint ? hintLabel.implicitHeight + Style.space(4) : 0)
  implicitWidth: Style.space(220)
  leftPadding: Style.space(8)
  rightPadding: Style.space(8)
  topPadding: 0
  bottomPadding: 0
  hoverEnabled: true
  opacity: enabled ? 1 : 0.55
  indicator: null
  arrow: null
  contentItem: Item {
    Item {
      id: marker
      width: Style.space(13)
      height: Style.space(28)
      Txt {
        anchors.centerIn: parent
        visible: !root.radio
        text: root.marked ? Icons.check : root.glyph
        font.pixelSize: Style.space(13)
        color: root.dangerous ? Color.urgent : root.marked ? Color.accent : Util.alpha(Color.popups.text, 0.68)
      }
      Rectangle {
        visible: root.radio
        anchors.centerIn: parent
        width: Style.space(14); height: width; radius: width / 2
        color: root.marked ? Color.popups.background : "transparent"
        border.width: root.marked ? Style.space(4) : 1
        border.color: root.marked ? Color.accent : Util.alpha(Color.popups.text, 0.4)
      }
    }
    Txt {
      anchors.left: marker.right
      anchors.leftMargin: Style.space(8)
      anchors.right: suffix.left
      anchors.rightMargin: Style.space(8)
      height: Style.space(28)
      text: root.text
      textFormat: Text.PlainText
      font.pixelSize: Style.font.body
      bold: root.marked && !root.radio
      color: root.dangerous ? Color.urgent : Color.popups.text
    }
    Txt {
      id: suffix
      anchors.right: parent.right
      height: Style.space(28)
      text: root.detail || root.keyHint
      textFormat: Text.PlainText
      font.pixelSize: Style.font.caption
      color: root.detail ? root.detailColor : Util.alpha(Color.popups.text, 0.42)
    }
    Txt {
      id: hintLabel
      x: Style.space(21); y: Style.space(28)
      width: parent.width - x
      visible: root.hint !== ""
      text: root.hint
      textFormat: Text.PlainText
      font.pixelSize: Style.space(9)
      faint: true
      wrapMode: Text.Wrap
    }
  }
  background: Rectangle {
    radius: Style.space(4)
    color: root.enabled && (root.highlighted || root.hovered) ? Util.alpha(Color.popups.text, 0.18) : "transparent"
  }
}
