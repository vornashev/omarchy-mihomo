import QtQuick
import QtQuick.Controls
import qs.Commons

// Core log tail; the stream is open only while this page is.
Item {
  id: root
  property var svc
  property var panel

  property string level: "warning"   // all | warning | error
  property bool follow: true
  property string search: ""
  readonly property bool typing: filter.input.activeFocus

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
  onShownChanged: if (follow) Qt.callLater(function() { list.positionViewAtEnd() })

  Row {
    id: head
    width: parent.width
    spacing: Style.space(10)
    Column {
      width: parent.width - seg.width - filter.width - clearBtn.width - Style.space(30)
      spacing: Style.space(3)
      Txt { text: I18n.t("logsTitle"); bold: true; font.pixelSize: Style.font.heading }
      Txt { text: I18n.t("logsSubtitle"); dim: true; font.pixelSize: Style.font.bodySmall }
    }
    Seg {
      id: seg
      anchors.verticalCenter: parent.verticalCenter
      current: root.level
      model: [{ key: "all", label: I18n.t("lvlAll") }, { key: "warning", label: I18n.t("lvlWarn") }, { key: "error", label: I18n.t("lvlError") }]
      onPicked: function(k) { root.level = k }
    }
    Field {
      id: filter
      width: Style.space(160)
      anchors.verticalCenter: parent.verticalCenter
      icon: Icons.search
      placeholder: I18n.t("filter")
      onTextChanged: root.search = text
      onEscaped: { text = ""; input.focus = false; root.panel.forceKeys() }
    }
    Btn { id: clearBtn; anchors.verticalCenter: parent.verticalCenter; ghost: true; icon: Icons.remove; onClicked: root.svc.logLines = [] }
  }

  Rectangle {
    anchors.top: head.bottom
    anchors.topMargin: Style.space(12)
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    radius: Style.cornerRadius
    color: Util.alpha(Color.popups.text, 0.03)
    ListView {
      id: list
      anchors.fill: parent
      anchors.margins: Style.space(8)
      clip: true
      model: root.shown
      ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
      onMovementStarted: root.follow = false
      delegate: Txt {
        required property var modelData
        width: list.width - Style.space(8)
        text: modelData.payload
        wrapMode: Text.WrapAnywhere
        maximumLineCount: 3
        font.pixelSize: Style.font.caption
        color: modelData.type === "error" ? root.svc.red : (modelData.type === "warning" ? root.svc.yellow : Util.alpha(Color.popups.text, 0.68))
      }
      Txt { visible: root.shown.length === 0; anchors.centerIn: parent; text: I18n.t("noLogs"); faint: true }
    }
  }
}
