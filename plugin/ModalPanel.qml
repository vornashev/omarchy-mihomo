import QtQuick
import Quickshell
import Quickshell.Wayland
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// The full interface in the middle of the screen over a dimmed desktop:
// KeyboardPanel's counterpart with the same card and keyboard focus prime,
// closed by Esc or a click outside the card. The owner moves its content
// into `slot`, so both windows show one set of pages.
PanelWindow {
  id: root

  required property Item anchorItem
  property QtObject bar: null
  property var owner: null
  property bool open: false
  property int margin: Style.gapsOut
  property int padding: 0
  // The large window from the design: 55 % × 86 % of the screen, never
  // smaller than the popup, at most 1600 wide, in steps of 8 px.
  property int minWidth: Style.space(1080)
  property int minHeight: Style.space(720)
  property int contentWidth: fittedContentWidth(Math.max(minWidth, Math.min(1600, step8(screenW * 0.55))))
  property int contentHeight: fittedContentHeight(Math.max(minHeight, step8(screenH * 0.86)) - verticalContentInset)
  property var borderSpec: Border.surfaceSpec("popups", "border", Commons.Color.popups.border, Math.max(1, Style.space(2)))
  property Item focusTarget: null
  property bool focusPrimed: false
  readonly property alias slot: holder

  readonly property var anchorWindow: anchorItem ? anchorItem.QsWindow.window : null
  readonly property var coordinatorKey: owner || root
  readonly property real screenW: screen ? screen.width : 0
  readonly property real screenH: screen ? screen.height : 0
  readonly property real verticalContentInset: padding * 2 + Border.top(borderSpec) + Border.bottom(borderSpec)

  function step8(v) { return Math.round(v / 8) * 8 }

  function fittedContentWidth(width) {
    var desired = Math.max(1, Number(width) || 1)
    return Math.round(screenW > 0 ? Math.min(desired, Math.max(120, screenW - margin * 2)) : desired)
  }

  function fittedContentHeight(implicitHeight) {
    var desired = Math.max(verticalContentInset, (Number(implicitHeight) || 0) + verticalContentInset)
    return Math.round(screenH > 0 ? Math.min(desired, Math.max(120, screenH - margin * 2)) : desired)
  }

  function close() {
    if (owner && "close" in owner) owner.close()
    else root.open = false
  }

  screen: anchorWindow ? anchorWindow.screen : null
  visible: open || dim.opacity > 0
  color: "transparent"
  exclusionMode: ExclusionMode.Ignore

  WlrLayershell.namespace: "omarchy-mihomo-modal"
  WlrLayershell.layer: WlrLayer.Overlay
  // Same as KeyboardPanel: a short Exclusive prime takes the keyboard on
  // every open, OnDemand then lets other outputs have the pointer.
  WlrLayershell.keyboardFocus: open
    ? (focusPrimed ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.Exclusive)
    : WlrKeyboardFocus.None

  anchors {
    top: true
    bottom: true
    left: true
    right: true
  }

  onBackingWindowVisibleChanged: if (open && backingWindowVisible) focusPrimeTimer.restart()

  onOpenChanged: {
    focusPrimed = false
    if (open) {
      if (backingWindowVisible) focusPrimeTimer.restart()
      // Later than the popup's own release when the owner moves here from it.
      Qt.callLater(function() {
        if (!root.open) return
        if (root.focusTarget) root.focusTarget.forceActiveFocus()
        if (root.bar) root.bar.requestPopout(root.coordinatorKey)
      })
    } else {
      focusPrimeTimer.stop()
      // An owner still open has moved to its popup, which holds the popout now.
      if (root.bar && !(root.owner && root.owner.opened)) root.bar.releasePopout(root.coordinatorKey)
    }
  }

  Timer {
    id: focusPrimeTimer
    interval: 75
    onTriggered: if (root.open) root.focusPrimed = true
  }

  Rectangle {
    id: dim
    anchors.fill: parent
    color: "black"
    opacity: root.open ? 0.5 : 0
    Behavior on opacity { NumberAnimation { duration: 140; easing.type: Easing.OutCubic } }
  }

  MouseArea {
    anchors.fill: parent
    enabled: root.open
    acceptedButtons: Qt.AllButtons
    onClicked: root.close()
  }

  // A click on another output closes the window too (see KeyboardPanel).
  Variants {
    model: root.open ? Quickshell.screens : []

    delegate: Component {
      PanelWindow {
        required property var modelData

        screen: modelData
        visible: root.open && !!root.screen && modelData.name !== root.screen.name
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore

        WlrLayershell.namespace: "omarchy-mihomo-modal-dismiss"
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

        anchors {
          top: true
          bottom: true
          left: true
          right: true
        }

        MouseArea {
          anchors.fill: parent
          acceptedButtons: Qt.AllButtons
          onPressed: root.close()
        }
      }
    }
  }

  BorderSurface {
    id: card
    anchors.centerIn: parent
    width: root.contentWidth
    height: root.contentHeight
    color: Commons.Color.popups.background
    borderSpec: root.borderSpec
    padding: root.padding
    radius: Style.cornerRadius
    opacity: root.open ? 1 : 0
    Behavior on opacity { NumberAnimation { duration: 140; easing.type: Easing.OutCubic } }

    // Clicks on the card stay off the dismissal area behind it.
    MouseArea {
      anchors.fill: parent
      acceptedButtons: Qt.AllButtons
    }

    Item {
      id: holder
      anchors.fill: parent
      anchors.topMargin: card.contentTopInset
      anchors.rightMargin: card.contentRightInset
      anchors.bottomMargin: card.contentBottomInset
      anchors.leftMargin: card.contentLeftInset
    }
  }
}
