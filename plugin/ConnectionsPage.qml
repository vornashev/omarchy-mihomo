import QtQuick
import QtQuick.Controls
import qs.Commons

// Live connections, polled only while this page is open.
Item {
  id: root
  property var svc
  property var panel

  property var conns: []
  property string search: ""
  property int cursor: 0
  readonly property bool typing: filter.input.activeFocus

  readonly property var shown: conns.filter(function(c) {
    if (search === "") return true
    var m = c.metadata, s = search.toLowerCase()
    return (m.host + " " + m.sniffHost + " " + m.destinationIP + " " + m.process + " " + c.chains.join(" ")).toLowerCase().indexOf(s) !== -1
  })

  function reload() {
    svc.request("connections.list", null, function(r) {
      if (!r) return
      var list = r.connections || []
      list.sort(function(a, b) { return (b.download + b.upload) - (a.download + a.upload) })
      conns = list
      cursor = Math.min(cursor, Math.max(0, shown.length - 1))
    })
  }
  function closeConn(c) { svc.request("connections.close", { id: c.id }, function() { reload() }) }

  function move(dx, dy) { cursor = Math.max(0, Math.min(shown.length - 1, cursor + dy)); list.positionViewAtIndex(cursor, ListView.Contain) }
  function remove() { if (shown[cursor]) closeConn(shown[cursor]) }
  function textKey(t) { if (t === "/") filter.input.forceActiveFocus() }

  Component.onCompleted: reload()
  Timer { interval: 2000; running: true; repeat: true; onTriggered: root.reload() }

  Row {
    id: head
    width: parent.width
    Column {
      width: parent.width - filter.width - closeAll.width - Style.space(8)
      spacing: Style.space(3)
      Txt { text: I18n.t("connectionsTitle"); bold: true; font.pixelSize: Style.font.heading }
      Txt { text: I18n.t("connectionsSubtitle").arg(root.conns.length); dim: true; font.pixelSize: Style.font.bodySmall }
    }
    Field {
      id: filter
      width: Style.space(180)
      anchors.verticalCenter: parent.verticalCenter
      icon: Icons.search
      placeholder: I18n.t("filter")
      onTextChanged: root.search = text
      onEscaped: { text = ""; input.focus = false; root.panel.forceKeys() }
    }
    Item { width: Style.space(8); height: 1 }
    Btn { id: closeAll; anchors.verticalCenter: parent.verticalCenter; icon: Icons.close; text: I18n.t("closeAll"); onClicked: root.svc.request("connections.close", null, function() { root.reload() }) }
  }

  ListView {
    id: list
    anchors.top: head.bottom
    anchors.topMargin: Style.space(12)
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    clip: true
    spacing: Style.space(1)
    model: root.shown
    ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
    delegate: Rectangle {
      required property var modelData
      required property int index
      readonly property var m: modelData.metadata
      width: list.width - Style.space(6)
      height: Style.space(40)
      radius: Style.cornerRadius
      color: index === root.cursor ? Style.hoverFillFor(Color.popups.text, Color.accent) : (cma.containsMouse ? Util.alpha(Color.popups.text, 0.04) : "transparent")
      MouseArea { id: cma; anchors.fill: parent; hoverEnabled: true; onClicked: root.cursor = index }
      Column {
        anchors.left: parent.left; anchors.leftMargin: Style.space(10)
        anchors.right: right.left; anchors.rightMargin: Style.space(10)
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(2)
        Txt { width: parent.width; text: (m.host || m.sniffHost || m.destinationIP) + ":" + m.destinationPort; bold: true; font.pixelSize: Style.font.bodySmall }
        Txt {
          width: parent.width
          text: (m.process ? Icons.app + " " + m.process + "  ·  " : "") + Icons.route + " " + modelData.chains.slice().reverse().join(" → ") + "  ·  " + modelData.rule + (modelData.rulePayload ? "(" + modelData.rulePayload + ")" : "")
          dim: true; font.pixelSize: Style.font.caption
        }
      }
      Row {
        id: right
        anchors.right: parent.right; anchors.rightMargin: Style.space(6)
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(10)
        Txt { anchors.verticalCenter: parent.verticalCenter; text: Icons.down + " " + root.svc.formatBytes(modelData.download) + "  " + Icons.up + " " + root.svc.formatBytes(modelData.upload); faint: true; font.pixelSize: Style.font.caption }
        Btn { ghost: true; icon: Icons.close; onClicked: root.closeConn(modelData) }
      }
    }
    Txt { visible: root.shown.length === 0; anchors.centerIn: parent; text: I18n.t("noConnections"); dim: true }
  }
}
