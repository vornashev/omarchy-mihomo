pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import qs.Commons
import qs.Commons as Commons
import "ProxyModels.js" as Models

Popup {
  id: root
  objectName: "proxyQuickSwitch"
  property string groupName: ""
  property var members: []
  property string current: ""
  property bool selectable: true
  property bool editable: true
  property bool testing: false
  property var svc
  signal selected(string name)
  signal testRequested()
  signal editRequested()
  popupType: Popup.Item
  width: Style.space(240)
  padding: Style.space(4)
  margins: Style.space(8)
  focus: true
  closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
  implicitHeight: Style.space(8 + 30 + (editable ? 38 : 0) + (selectable ? 24 : 0)) + Math.min(options.contentHeight, Style.space(224))
  background: MenuSurface {}
  enter: Transition {}
  exit: Transition {}
  function choose() {
    var member = members[options.currentIndex];
    if (!selectable || !member) return;
    selected(member.name);
    close();
  }
  function move(step) {
    if (!members.length) return;
    options.currentIndex = (options.currentIndex + step + members.length) % members.length;
    options.positionViewAtIndex(options.currentIndex, ListView.Contain);
  }
  onAboutToShow: options.currentIndex = Math.max(0, members.findIndex(function(m) { return m.name === root.current; }))
  onOpened: options.forceActiveFocus()
  contentItem: Item {
    Keys.onDownPressed: root.move(1)
    Keys.onUpPressed: root.move(-1)
    Keys.onReturnPressed: root.choose()
    Keys.onEnterPressed: root.choose()
    Keys.onEscapePressed: root.close()
    Item {
      id: header
      width: parent.width
      height: Style.space(30)
      Txt {
        x: Style.space(8)
        anchors.right: testButton.left
        anchors.rightMargin: Style.space(6)
        anchors.verticalCenter: parent.verticalCenter
        text: MihomoI18n.t("proxyQuickCount").arg(root.groupName).arg(root.members.length)
        textFormat: Text.PlainText
        font.pixelSize: Style.font.bodySmall
        faint: true
      }
      ToolButton {
        id: testButton
        objectName: "quickSwitchTest"
        anchors.right: parent.right
        anchors.rightMargin: Style.space(8)
        anchors.verticalCenter: parent.verticalCenter
        width: testLabel.implicitWidth
        padding: 0
        height: Style.space(24)
        enabled: !root.testing
        contentItem: Txt {
          id: testLabel
          text: (root.testing ? Icons.refresh : Icons.test) + " " + MihomoI18n.t("proxyQuickCheck")
          font.pixelSize: Style.font.bodySmall
          color: Commons.Color.accent
        }
        background: Rectangle {
          radius: Style.space(4)
          color: testButton.hovered || testButton.activeFocus ? Util.alpha(Commons.Color.popups.text, 0.18) : "transparent"
        }
        onClicked: root.testRequested()
      }
    }
    ListView {
      id: options
      objectName: "quickSwitchOptions"
      anchors.top: header.bottom
      anchors.bottom: footer.top
      width: parent.width
      clip: true
      spacing: Style.space(1)
      keyNavigationEnabled: false
      model: root.members
      boundsBehavior: Flickable.StopAtBounds
      ScrollIndicator.vertical: ScrollIndicator {}
      delegate: ActionMenuItem {
        required property var modelData
        required property int index
        width: options.width
        text: modelData.name
        radio: true
        marked: modelData.name === root.current
        enabled: root.selectable
        highlighted: options.currentIndex === index
        detail: modelData.delay === 0 ? MihomoI18n.t("proxyNoResponse") : Models.delayText(modelData.delay, MihomoI18n.t("ms"))
        detailColor: modelData.delay === 0 ? (root.svc ? root.svc.red : Commons.Color.urgent)
          : modelData.delay == null ? Util.alpha(Commons.Color.popups.text, 0.42)
          : modelData.delay < 150 ? (root.svc ? root.svc.green : Commons.Color.accent) : (root.svc ? root.svc.yellow : Commons.Color.accent)
        onTriggered: { options.currentIndex = index; root.choose(); }
      }
    }
    Column {
      id: footer
      anchors.bottom: parent.bottom
      width: parent.width
      ActionMenuSeparator { width: parent.width; visible: root.editable }
      ActionMenuItem {
        objectName: "quickSwitchEdit"
        visible: root.editable
        width: parent.width
        glyph: Icons.edit
        text: MihomoI18n.t("proxyChangeMembers")
        onTriggered: { root.close(); root.editRequested(); }
      }
      Row {
        visible: root.selectable
        height: Style.space(24)
        leftPadding: Style.space(8)
        spacing: Style.space(10)
        Repeater {
          model: [{ key: "↑↓", label: MihomoI18n.t("proxyQuickChoose") }, { key: "Enter", label: MihomoI18n.t("proxyQuickSwitch") }]
          Row {
            id: keyRow
            required property var modelData
            anchors.verticalCenter: parent.verticalCenter
            spacing: Style.space(4)
            Rectangle {
              width: keyLabel.implicitWidth + Style.space(8)
              height: keyLabel.implicitHeight
              radius: Style.space(3)
              color: Util.alpha(Commons.Color.popups.text, 0.08)
              Txt { id: keyLabel; anchors.centerIn: parent; text: keyRow.modelData.key; font.pixelSize: Style.font.caption; dim: true }
            }
            Txt { text: keyRow.modelData.label; font.pixelSize: Style.font.caption; faint: true }
          }
        }
      }
    }
  }
}
