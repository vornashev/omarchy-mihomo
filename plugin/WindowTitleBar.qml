import QtQuick
import qs.Commons
import qs.Commons as Commons
import "SearchModels.js" as SM

// Title bar of the large window: the core, a search across sections,
// problem hosts, active connections and own rules, and applying the config.
Rectangle {
  id: root
  property var svc
  property var panel

  readonly property bool typing: search.input.activeFocus
  property var hosts: []
  property var conns: []
  property var rules: []
  property int cursor: 0
  readonly property var results: SM.results(search.text, panel ? panel.pages : [], hosts, conns, rules, 8)
  property bool applying: false

  function focusSearch() {
    search.input.forceActiveFocus()
    search.input.selectAll()
  }

  function done() {
    search.text = ""
    search.input.focus = false
    panel.forceKeys()
  }

  function pick(item) {
    if (!item) return
    done()
    if (item.kind === "page") panel.showPage(item.page)
    else if (item.kind === "host") panel.openEditor({ problem: item.problem })
    else if (item.kind === "conn") panel.showConnections(item.query)
    else panel.openRule({ from: "rules", rule: item.rule })
  }

  function apply() {
    applying = true
    svc.request("config.apply", null, function(r, err) {
      root.applying = false
      root.panel.notify(err ? err.message : MihomoI18n.t("configApplied"))
    })
  }

  implicitHeight: Style.space(44)
  color: Util.alpha(Commons.Color.popups.background, 0.6)
  z: 2

  Row {
    anchors.left: parent.left
    anchors.leftMargin: Style.space(16)
    anchors.verticalCenter: parent.verticalCenter
    spacing: Style.space(12)
    Txt { text: "mihomo"; bold: true; anchors.verticalCenter: parent.verticalCenter }
    Txt {
      anchors.verticalCenter: parent.verticalCenter
      text: root.svc.coreRunning ? (root.svc.status.core.version || "") + " · mixed-port " + root.svc.status.core.mixed_port
        : (root.svc.connected ? MihomoI18n.t("coreStopped") : MihomoI18n.t("noDaemon"))
      faint: true; font.pixelSize: Style.font.bodySmall
    }
  }

  Field {
    id: search
    objectName: "titleSearch"
    anchors.centerIn: parent
    width: Math.min(Style.space(460), parent.width - Style.space(720))
    icon: Icons.search
    placeholder: MihomoI18n.t("titleSearch")
    onTextChanged: { root.cursor = 0; hostQuery.restart() }
    onAccepted: root.pick(root.results[root.cursor])
    onEscaped: root.done()
    // A one-line input leaves Up and Down to its parent.
    Keys.onDownPressed: root.cursor = Math.min(root.results.length - 1, root.cursor + 1)
    Keys.onUpPressed: root.cursor = Math.max(0, root.cursor - 1)
  }

  Connections {
    target: search.input
    function onActiveFocusChanged() {
      if (search.input.activeFocus && !root.rules.length) root.svc.request("rules.list", null, function(r) { if (r) root.rules = r.items })
    }
  }

  // Hosts come from the daemon, which searches every problem it keeps;
  // connections are the live list, filtered here.
  Timer {
    id: hostQuery
    interval: 150
    onTriggered: {
      var q = search.text.trim()
      if (!q) { root.hosts = []; root.conns = []; return }
      root.svc.request("problems.list", { search: q, limit: 5 }, function(r) { if (r && search.text.trim() === q) root.hosts = r.items })
      root.svc.request("connections.list", null, function(r) { if (r && search.text.trim() === q) root.conns = r.connections || [] })
    }
  }

  Row {
    anchors.right: parent.right
    anchors.rightMargin: Style.space(14)
    anchors.verticalCenter: parent.verticalCenter
    spacing: Style.space(12)
    Btn {
      objectName: "titleApply"
      anchors.verticalCenter: parent.verticalCenter
      icon: Icons.refresh
      text: MihomoI18n.t("applyConfig")
      busy: root.applying
      enabled: root.svc.connected
      onClicked: root.apply()
    }
    KeyHint { key: "esc"; anchors.verticalCenter: parent.verticalCenter }
  }

  Rectangle {
    anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom
    height: 1
    color: Util.alpha(Commons.Color.popups.text, 0.1)
  }

  Rectangle {
    objectName: "titleResults"
    visible: root.typing && search.text.trim() !== ""
    x: search.x
    y: search.y + search.height + Style.space(4)
    width: search.width
    height: list.implicitHeight + Style.space(8)
    radius: Style.cornerRadius
    color: Commons.Color.popups.background
    border.width: 1
    border.color: Util.alpha(Commons.Color.popups.text, 0.18)

    Column {
      id: list
      x: Style.space(4); y: Style.space(4)
      width: parent.width - Style.space(8)
      Txt {
        visible: root.results.length === 0
        width: parent.width
        leftPadding: Style.space(8)
        height: Style.space(30)
        text: MihomoI18n.t("titleSearchEmpty"); faint: true; font.pixelSize: Style.font.body
      }
      Repeater {
        model: root.results
        Rectangle {
          required property var modelData
          required property int index
          width: list.width
          height: Style.space(30)
          radius: Style.cornerRadius
          color: index === root.cursor ? Style.hoverFillFor(Commons.Color.popups.text, Commons.Color.accent) : "transparent"
          Row {
            anchors.left: parent.left; anchors.leftMargin: Style.space(8)
            anchors.right: kindText.left; anchors.rightMargin: Style.space(8)
            anchors.verticalCenter: parent.verticalCenter
            spacing: Style.space(8)
            Txt {
              text: modelData.kind === "page" ? modelData.icon : ({ host: Icons.problems, conn: Icons.connections, rule: Icons.rules })[modelData.kind]
              faint: true; anchors.verticalCenter: parent.verticalCenter
            }
            Txt { text: modelData.label; anchors.verticalCenter: parent.verticalCenter }
            Txt { text: modelData.detail; dim: true; font.pixelSize: Style.font.bodySmall; anchors.verticalCenter: parent.verticalCenter }
          }
          Txt {
            id: kindText
            anchors.right: parent.right; anchors.rightMargin: Style.space(8)
            anchors.verticalCenter: parent.verticalCenter
            text: MihomoI18n.t({ page: "searchPage", host: "searchHost", conn: "searchConn", rule: "searchRule" }[modelData.kind])
            faint: true; font.pixelSize: Style.font.bodySmall
          }
          MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onEntered: root.cursor = index
            onClicked: root.pick(modelData)
          }
        }
      }
    }
  }
}
