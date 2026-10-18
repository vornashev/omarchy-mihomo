import QtQuick
import QtQuick.Controls
import QtQuick.Effects
import qs.Commons

// Tooltip · yDqLL. Popup.Item keeps hints above clipped list delegates.
ToolTip {
  id: root
  objectName: "pluginHint"
  property string keyHint: ""
  readonly property real gap: Style.space(6)
  readonly property var overlay: Overlay.overlay
  readonly property point origin: parent && overlay ? parent.mapToItem(overlay, 0, 0) : Qt.point(0, 0)
  popupType: Popup.Item
  delay: 500
  timeout: -1
  margins: Style.space(8)
  padding: Style.space(5)
  leftPadding: Style.space(8)
  rightPadding: Style.space(8)
  implicitWidth: Math.min(Style.space(240), overlay ? Math.max(1, overlay.width - 2 * margins) : Style.space(240),
    measure.implicitWidth + leftPadding + rightPadding + (keyHint ? keyBadge.implicitWidth + Style.space(8) : 0))
  implicitHeight: Math.max(label.implicitHeight, keyBadge.visible ? keyBadge.implicitHeight : 0) + topPadding + bottomPadding
  x: parent ? Math.max(margins - origin.x, Math.min((parent.width - width) / 2,
    overlay ? overlay.width - margins - origin.x - width : (parent.width - width) / 2)) : 0
  y: parent ? (overlay && origin.y + parent.height + gap + height > overlay.height - margins
    ? -height - gap : parent.height + gap) : 0
  enter: Transition {}
  exit: Transition {}

  Text {
    id: measure
    visible: false
    text: root.text
    textFormat: Text.PlainText
    font.family: Style.font.family
    font.pixelSize: Style.space(11)
  }
  contentItem: Item {
    Text {
      id: label
      width: parent.width - (keyBadge.visible ? keyBadge.width + Style.space(8) : 0)
      anchors.verticalCenter: parent.verticalCenter
      text: root.text
      textFormat: Text.PlainText
      font: measure.font
      color: Color.popups.text
      wrapMode: Text.Wrap
      lineHeight: lineCount > 1 ? 1.35 : 1
    }
    Rectangle {
      id: keyBadge
      visible: root.keyHint !== ""
      anchors.right: parent.right
      anchors.verticalCenter: parent.verticalCenter
      implicitWidth: keyLabel.implicitWidth + Style.space(8)
      implicitHeight: keyLabel.implicitHeight
      radius: Style.space(3)
      color: Util.alpha(Color.popups.text, 0.08)
      Text {
        id: keyLabel
        anchors.centerIn: parent
        text: root.keyHint
        textFormat: Text.PlainText
        font.family: Style.font.family
        font.pixelSize: Style.space(10)
        color: Util.alpha(Color.popups.text, 0.8)
      }
    }
  }
  background: Item {
    RectangularShadow {
      anchors.fill: surface
      radius: surface.radius
      blur: Style.space(12)
      offset: Qt.vector2d(0, Style.space(4))
      color: "#66000000"
    }
    Rectangle {
      id: surface
      anchors.fill: parent
      radius: Style.space(4)
      color: Qt.tint(Color.popups.background, Util.alpha(Color.popups.text, 0.11))
      border.width: 1
      border.color: Util.alpha(Color.popups.text, 0.15)
    }
  }
}
