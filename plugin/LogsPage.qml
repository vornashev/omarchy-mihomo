import QtQuick
import QtQuick.Controls
import qs.Commons
import qs.Commons as Commons

// Recent core logs, followed by live events while this page is open.
Item {
  id: root
  property var svc
  property var panel

  property string level: "all"   // all | warning | error
  property bool follow: true
  property string search: ""
  readonly property bool typing: filter.input.activeFocus
  readonly property string emptyText: !svc.connected ? MihomoI18n.t("daemonDown")
    : !svc.coreRunning ? MihomoI18n.t("logsCoreStopped")
    : svc.logLines.length > 0 ? MihomoI18n.t("logsNoMatches") : MihomoI18n.t("logsWaiting")

  function timestamp(ms) {
    return ms ? Qt.formatDateTime(new Date(ms), "hh:mm:ss") + " " : ""
  }
  function scrollToEnd() { if (follow) list.positionViewAtEnd() }

  readonly property var shown: svc.logLines.filter(function(l) {
    if (level === "warning" && l.type !== "warning" && l.type !== "error") return false
    if (level === "error" && l.type !== "error") return false
    return search === "" || l.payload.toLowerCase().indexOf(search.toLowerCase()) !== -1
  })

  function textKey(t) {
    if (t === "/") filter.input.forceActiveFocus()
    else if (t === "f") follow = !follow
    else if (t === "c") svc.logLines = []
  }

  Component.onCompleted: svc.want("logs", true)
  Component.onDestruction: svc.want("logs", false)
  onShownChanged: Qt.callLater(scrollToEnd)
  onFollowChanged: Qt.callLater(scrollToEnd)

  Row {
    id: head
    width: parent.width
    spacing: Style.space(10)
    Column {
      width: parent.width - seg.width - filter.width - clearBtn.width - Style.space(30)
      spacing: Style.space(3)
      Txt { text: MihomoI18n.t("logsTitle"); bold: true; font.pixelSize: Style.font.heading }
      Txt { text: MihomoI18n.t("logsSubtitle"); dim: true; font.pixelSize: Style.font.body }
    }
    Seg {
      id: seg
      anchors.verticalCenter: parent.verticalCenter
      current: root.level
      model: [{ key: "all", label: MihomoI18n.t("lvlAll") }, { key: "warning", label: MihomoI18n.t("lvlWarnShort") }, { key: "error", label: MihomoI18n.t("lvlError") }]
      onPicked: function(k) { root.level = k }
    }
    Field {
      id: filter
      width: Style.space(130)
      anchors.verticalCenter: parent.verticalCenter
      icon: Icons.search
      placeholder: MihomoI18n.t("filter")
      onTextChanged: root.search = text
      onEscaped: { text = ""; input.focus = false; root.panel.forceKeys() }
      Component.onCompleted: if (root.panel && root.panel.logFilter) { text = root.panel.logFilter; root.panel.logFilter = "" }
    }
    Btn { id: clearBtn; objectName: "clearLogs"; anchors.verticalCenter: parent.verticalCenter; ghost: true; icon: Icons.remove; tooltip: MihomoI18n.t("logsClear"); keyHint: "c"; onClicked: root.svc.logLines = [] }
  }

  Rectangle {
    anchors.top: head.bottom
    anchors.topMargin: Style.space(12)
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: footer.top
    anchors.bottomMargin: Style.space(12)
    radius: Style.cornerRadius
    color: Util.alpha(Commons.Color.popups.text, 0.03)
    ListView {
      id: list
      objectName: "logList"
      anchors.left: parent.left
      anchors.right: parent.right
      anchors.bottom: parent.bottom
      height: Math.min(contentHeight, parent.height - Style.space(24))
      anchors.margins: Style.space(12)
      clip: true
      model: root.shown
      spacing: Style.space(3)
      onContentHeightChanged: Qt.callLater(root.scrollToEnd)
      ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
      onMovementStarted: root.follow = false
      delegate: Txt {
        required property var modelData
        width: list.width - Style.space(8)
        text: root.timestamp(modelData.timestamp) + modelData.payload
        textFormat: Text.PlainText
        wrapMode: Text.WrapAnywhere
        elide: Text.ElideNone
        lineHeight: 1.45
        font.pixelSize: Style.font.bodySmall
        color: modelData.type === "error" ? root.svc.red : (modelData.type === "warning" ? root.svc.yellow : Util.alpha(Commons.Color.popups.text, 0.68))
      }
    }
    Txt { visible: root.shown.length === 0; anchors.centerIn: parent; width: parent.width - Style.space(24); horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap; text: root.emptyText; faint: true }
  }

  Row {
    id: footer
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    height: Style.space(24)
    Btn {
      id: followBtn
      objectName: "followLogs"
      ghost: true
      icon: root.follow ? Icons.check : Icons.down
      text: MihomoI18n.t(root.follow ? "logsFollowing" : "logsFollow")
      onClicked: root.follow = !root.follow
    }
    Item { width: Math.max(0, footer.width - followBtn.width - shortcuts.width); height: 1 }
    Txt {
      id: shortcuts
      anchors.verticalCenter: parent.verticalCenter
      text: MihomoI18n.t("logsShortcuts")
      faint: true
      font.pixelSize: Style.font.bodySmall
    }
  }
}
