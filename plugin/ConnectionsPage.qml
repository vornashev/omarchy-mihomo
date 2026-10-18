import QtQuick
import QtQml
import QtQuick.Controls
import qs.Commons
import "RuleModels.js" as RuleModels

// Live connections, polled only while this page is open.
Item {
  id: root
  property var svc
  property var panel

  property var conns: []
  property string search: ""
  property int cursor: 0
  readonly property bool typing: filter.input.activeFocus || connMenu.visible

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
  function textKey(t) {
    if (t === "/") filter.input.forceActiveFocus()
    else if (t === "r" && shown[cursor]) ruleFor(shown[cursor], RuleModels.quickDefault(RuleModels.origin(shown[cursor])), false)
    else if (t === "R" && shown[cursor]) ruleFor(shown[cursor], RuleModels.quickDefault(RuleModels.origin(shown[cursor])), true)
    else if (t === "m" && shown[cursor]) openMenu(shown[cursor], null)
  }

  // Rule editor for a connection, with `scope` ({kind, value}) chosen.
  function ruleFor(conn, scope, full) {
    var spec = { from: "connections", origin: RuleModels.origin(conn), full: full }
    if (scope) { spec.kind = scope.kind; spec.value = scope.value }
    panel.openRule(spec)
  }
  function openMenu(conn, anchor) {
    connMenu.conn = conn
    if (anchor) connMenu.popup(anchor, anchor.width - connMenu.width, anchor.height + Style.space(4))
    else connMenu.popup()
  }
  function copyHost(conn) {
    var o = RuleModels.origin(conn)
    clipboard.text = o.host || o.ip
    clipboard.selectAll(); clipboard.copy(); clipboard.deselect()
    panel.notify(I18n.t("copiedToast").arg(clipboard.text))
  }
  TextEdit { id: clipboard; visible: false }

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
      HoverHandler { id: rowHover }
      MouseArea {
        id: cma
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onClicked: function(mouse) {
          root.cursor = index
          if (mouse.button === Qt.RightButton) root.openMenu(modelData, null)
        }
      }
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
        Btn {
          id: ruleBtn
          objectName: "connRuleButton"
          ghost: true
          opacity: rowHover.hovered || index === root.cursor ? 1 : 0
          icon: Icons.ruleAdd
          tooltip: I18n.t("hintAddRule")
          keyHint: "R"
          onClicked: root.openMenu(modelData, ruleBtn)
        }
        Btn { ghost: true; icon: Icons.close; tooltip: I18n.t("hintCloseConnection"); onClicked: root.closeConn(modelData) }
      }
    }
    Txt { visible: root.shown.length === 0; anchors.centerIn: parent; text: I18n.t("noConnections"); dim: true }
  }

  ActionMenu {
    id: connMenu
    objectName: "connectionMenu"
    width: Style.space(276)
    property var conn: null
    readonly property var origin: conn ? RuleModels.origin(conn) : ({})
    readonly property var quick: conn ? RuleModels.quickScopes(origin) : []
    Column {
      width: connMenu.availableWidth
      leftPadding: Style.space(8); topPadding: Style.space(4); bottomPadding: Style.space(4)
      spacing: Style.space(2)
      Txt { width: parent.width - Style.space(16); text: (connMenu.origin.host || connMenu.origin.ip || "") + (connMenu.origin.port ? ":" + connMenu.origin.port : ""); bold: true; font.pixelSize: Style.font.bodySmall }
      Txt { width: parent.width - Style.space(16); text: [connMenu.origin.process, connMenu.origin.route].filter(function(s) { return s }).join(" · "); faint: true; font.pixelSize: Style.font.caption }
    }
    ActionMenuSeparator {}
    Txt { leftPadding: Style.space(8); topPadding: Style.space(3); bottomPadding: Style.space(2); text: I18n.t("connRuleSection"); faint: true; bold: true; font.pixelSize: Style.font.caption; font.letterSpacing: 1 }
    Instantiator {
      model: connMenu.quick
      delegate: ActionMenuItem {
        required property var modelData
        objectName: "quick_" + modelData.kind
        glyph: modelData.kind === "process-name" ? Icons.cpu : (modelData.kind === "ip-cidr" ? Icons.ip : Icons.globe)
        text: (modelData.kind === "process-name" ? I18n.t("processPrefix") + " " : "") + modelData.title
        keyHint: I18n.t("quick_" + modelData.kind) + (modelData.kind === RuleModels.quickDefault(connMenu.origin).kind ? "  R" : "")
        onTriggered: root.ruleFor(connMenu.conn, modelData, false)
      }
      onObjectAdded: function(index, object) { connMenu.insertItem(3 + index, object) }
      onObjectRemoved: function(index, object) { connMenu.removeItem(object) }
    }
    ActionMenuItem {
      objectName: "quick_custom"
      glyph: Icons.settings
      text: I18n.t("connRuleCustom")
      keyHint: "Shift+R"
      onTriggered: root.ruleFor(connMenu.conn, RuleModels.quickDefault(connMenu.origin), true)
    }
    ActionMenuSeparator {}
    ActionMenuItem { glyph: Icons.copy; text: I18n.t("copyHost"); keyHint: "Ctrl+C"; onTriggered: root.copyHost(connMenu.conn) }
    ActionMenuItem { glyph: Icons.logs; text: I18n.t("showInLogs"); keyHint: "L"; onTriggered: root.panel.showLogs(connMenu.origin.host || connMenu.origin.ip) }
    ActionMenuSeparator {}
    ActionMenuItem { glyph: Icons.close; dangerous: true; text: I18n.t("closeConnection"); keyHint: "Del"; onTriggered: root.closeConn(connMenu.conn) }
    Txt {
      visible: connMenu.origin.rule !== undefined && connMenu.origin.rule !== ""
      width: connMenu.availableWidth
      leftPadding: Style.space(8); topPadding: Style.space(4); bottomPadding: Style.space(3)
      text: Icons.info + " " + I18n.t("connRuleNow").arg(connMenu.origin.rule || "").arg((connMenu.conn && connMenu.conn.chains.length ? connMenu.conn.chains[connMenu.conn.chains.length - 1] : ""))
      faint: true; font.pixelSize: Style.font.caption
    }
    Shortcut { sequence: "R"; enabled: connMenu.visible; onActivated: { connMenu.close(); root.ruleFor(connMenu.conn, RuleModels.quickDefault(connMenu.origin), false) } }
    Shortcut { sequence: "Shift+R"; enabled: connMenu.visible; onActivated: { connMenu.close(); root.ruleFor(connMenu.conn, RuleModels.quickDefault(connMenu.origin), true) } }
    Shortcut { sequence: "L"; enabled: connMenu.visible; onActivated: { connMenu.close(); root.panel.showLogs(connMenu.origin.host || connMenu.origin.ip) } }
    Shortcut { sequence: "Ctrl+C"; enabled: connMenu.visible; onActivated: { connMenu.close(); root.copyHost(connMenu.conn) } }
  }
}
