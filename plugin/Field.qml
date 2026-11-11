import QtQuick
import qs.Commons
import qs.Commons as Commons

// Single-line input with an optional leading icon and placeholder.
Rectangle {
  id: root
  property alias text: input.text
  property alias input: input
  property string placeholder: ""
  property string icon: ""
  signal accepted()
  signal escaped()

  implicitHeight: Style.space(28)
  radius: Style.cornerRadius
  color: Util.alpha(Commons.Color.popups.text, 0.04)
  border.width: 1
  border.color: input.activeFocus ? Util.alpha(Commons.Color.accent, 0.8) : Util.alpha(Commons.Color.popups.text, 0.15)

  Txt {
    id: ic
    visible: root.icon !== ""
    anchors.left: parent.left; anchors.leftMargin: Style.space(9)
    anchors.verticalCenter: parent.verticalCenter
    text: root.icon; faint: true; font.pixelSize: Style.font.icon
  }
  TextInput {
    id: input
    activeFocusOnTab: true
    anchors.left: ic.visible ? ic.right : parent.left
    anchors.leftMargin: Style.space(ic.visible ? 7 : 9)
    anchors.right: parent.right; anchors.rightMargin: Style.space(9)
    anchors.verticalCenter: parent.verticalCenter
    color: Commons.Color.popups.text
    selectionColor: Style.selectionFillFor(Commons.Color.popups.text, Commons.Color.accent)
    font.family: Style.font.family
    font.pixelSize: Style.font.body
    clip: true
    onAccepted: root.accepted()
    Keys.onEscapePressed: function(e) { root.escaped(); e.accepted = true }
  }
  Txt {
    visible: input.text === "" && !input.activeFocus
    anchors.fill: input
    text: root.placeholder; faint: true
  }
  MouseArea { anchors.fill: parent; cursorShape: Qt.IBeamCursor; onClicked: input.forceActiveFocus(); propagateComposedEvents: true }
}
