import QtQuick
import QtQuick.Controls
import qs.Commons

// Button: `primary` filled with the accent, `ghost` without a border.
Rectangle {
  id: root
  property string text: ""
  property string icon: ""
  property bool primary: false
  property bool ghost: false
  property bool busy: false
  property bool hasCursor: false
  property string tooltip: text
  property string disabledTooltip: ""
  property string keyHint: ""
  HoverHandler { id: hintHover; enabled: true }
  Hint {
    visible: hintHover.hovered && !ma.pressed && root.visible && text !== ""
    text: !root.enabled && root.disabledTooltip ? root.disabledTooltip : root.tooltip
    keyHint: root.keyHint
  }
  Accessible.role: Accessible.Button
  Accessible.name: tooltip || text
  Accessible.onPressAction: if (enabled && !busy) clicked()
  signal clicked()
  activeFocusOnTab: true
  Keys.onReturnPressed: if (enabled && !busy) clicked()
  Keys.onSpacePressed: if (enabled && !busy) clicked()

  readonly property color fg: primary ? Color.background : Color.popups.text
  readonly property bool hot: (ma.containsMouse || hasCursor || activeFocus) && enabled

  implicitWidth: row.implicitWidth + Style.space(text === "" ? 12 : 22)
  implicitHeight: Style.space(28)
  radius: Style.cornerRadius
  opacity: enabled ? 1 : 0.45
  color: primary ? (hot ? Qt.lighter(Color.accent, 1.1) : Color.accent) : (hot ? Style.hoverFillFor(Color.popups.text, Color.accent) : (ghost ? "transparent" : Util.alpha(Color.popups.text, 0.04)))
  border.width: primary || ghost ? 0 : 1
  border.color: hot ? Util.alpha(Color.popups.text, 0.5) : Util.alpha(Color.popups.text, 0.3)

  Row {
    id: row
    anchors.centerIn: parent
    spacing: Style.space(6)
    Txt {
      visible: root.icon !== ""
      text: root.busy ? Icons.refresh : root.icon
      color: root.fg
      font.pixelSize: Style.font.icon
      RotationAnimation on rotation { running: root.busy; loops: Animation.Infinite; from: 0; to: 360; duration: 900 }
      onVisibleChanged: if (!root.busy) rotation = 0
    }
    Txt { visible: root.text !== ""; text: root.text; color: root.fg; bold: root.primary }
  }
  MouseArea {
    id: ma
    anchors.fill: parent
    hoverEnabled: true
    cursorShape: root.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
    onClicked: if (root.enabled && !root.busy) root.clicked()
  }
}
