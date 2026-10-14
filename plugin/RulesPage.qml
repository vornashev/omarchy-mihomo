import QtQuick
import QtQuick.Controls
import qs.Commons

// The user's own rules: they sit above the profile's rules and survive
// profile updates. Journal + rollback for every change.
Item {
  id: root
  property var svc
  property var panel

  property string tab: "all"   // all | direct | proxy | off
  property var rules: []
  property var history: []
  property bool showHistory: false
  property int cursor: 0
  readonly property bool typing: false

  readonly property var shown: rules.filter(function(r) {
    if (tab === "direct") return r.enabled && r.target === "DIRECT"
    if (tab === "proxy") return r.enabled && r.target !== "DIRECT"
    if (tab === "off") return !r.enabled
    return true
  })
  readonly property var orphanRules: rules.filter(function(r) { return r.orphan })

  function reload() {
    svc.request("rules.list", null, function(r, err) {
      if (err || !r) return
      rules = r.items
      cursor = Math.min(cursor, Math.max(0, shown.length - 1))
    })
    svc.request("rules.history", { limit: 30 }, function(h) { if (h) history = h })
  }

  function after(r, err) {
    if (err) { panel.notify(err.message); reload(); return }
    if (r && r.closed > 0) panel.notify(I18n.t("closedConns").arg(r.closed))
    reload()
  }
  function toggle(rule) { svc.request("rules.update", { id: rule.id, enabled: !rule.enabled }, after) }
  function del(rule) { svc.request("rules.delete", { id: rule.id }, function(r, err) { if (!err) panel.notify(I18n.t("deletedToast").arg(rule.value)); after(r, err) }) }
  function undoLast() {
    if (history.length === 0) return
    svc.request("rules.rollback", { to: history[0].id - 1 }, function(r, err) { if (!err) panel.notify(I18n.t("undoneToast")); after(null, err) })
  }
  function rollbackTo(change) {
    svc.request("rules.rollback", { to: change.id }, function(r, err) { if (!err) panel.notify(I18n.t("rolledBack").arg(r.undone)); after(null, err) })
  }

  function move(dx, dy) {
    if (dx !== 0) {
      var tabs = ["all", "direct", "proxy", "off"]
      tab = tabs[(tabs.indexOf(tab) + dx + tabs.length) % tabs.length]
      return
    }
    cursor = Math.max(0, Math.min(shown.length - 1, cursor + dy))
    list.positionViewAtIndex(cursor, ListView.Contain)
  }
  function activate() { if (shown[cursor]) panel.openEditor({ rule: shown[cursor] }) }
  function remove() { if (shown[cursor]) del(shown[cursor]) }
  function textKey(t) {
    if (t === " " || t === "o") { if (shown[cursor]) toggle(shown[cursor]) }
    else if (t === "u") undoLast()
    else if (t === "H") showHistory = !showHistory
  }

  onTabChanged: cursor = 0
  Component.onCompleted: reload()
  Connections {
    target: root.svc
    function onStatusChanged() { reloadTimer.restart() }
  }
  Timer { id: reloadTimer; interval: 300; onTriggered: root.reload() }

  Column {
    id: head
    anchors.left: parent.left
    anchors.right: parent.right
    spacing: Style.space(12)

    Row {
      width: parent.width
      spacing: Style.space(10)
      Column {
        width: parent.width - histBtn.width - Style.space(10)
        spacing: Style.space(3)
        Txt { text: I18n.t("rulesTitle"); bold: true; font.pixelSize: Style.font.heading }
        Txt { text: I18n.t("rulesSubtitle").arg(root.rules.length); dim: true; font.pixelSize: Style.font.bodySmall }
      }
      Btn { id: histBtn; anchors.verticalCenter: parent.verticalCenter; icon: Icons.history; text: I18n.t("history"); onClicked: root.showHistory = !root.showHistory }
    }

    Seg {
      current: root.tab
      model: [
        { key: "all", label: I18n.t("tabAll") + " " + root.rules.length },
        { key: "direct", label: "DIRECT " + root.rules.filter(function(r) { return r.enabled && r.target === "DIRECT" }).length },
        { key: "proxy", label: I18n.t("tabProxy") + " " + root.rules.filter(function(r) { return r.enabled && r.target !== "DIRECT" }).length },
        { key: "off", label: I18n.t("tabOff") + " " + root.rules.filter(function(r) { return !r.enabled }).length }
      ]
      onPicked: function(k) { root.tab = k }
    }

    Banner {
      visible: root.orphanRules.length > 0
      width: parent.width
      tone: "warn"
      toneColor: root.svc.yellow
      icon: Icons.orphan
      text: I18n.t("orphanBanner").arg(root.orphanRules.map(function(r) { return r.target }).filter(function(v, i, a) { return a.indexOf(v) === i }).join(", ")).arg(root.orphanRules.length)
      action: I18n.t("fix")
      onActionClicked: root.panel.openEditor({ rule: root.orphanRules[0] })
    }
  }

  ListView {
    id: list
    visible: !root.showHistory
    anchors.top: head.bottom
    anchors.topMargin: Style.space(10)
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: footer.top
    anchors.bottomMargin: Style.space(8)
    clip: true
    spacing: Style.space(2)
    model: root.shown
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
    delegate: RuleRow {
      required property var modelData
      required property int index
      width: list.width - Style.space(6)
      rule: modelData
      svc: root.svc
      selected: index === root.cursor
      onClicked: root.cursor = index
      onToggled: root.toggle(modelData)
      onEditClicked: root.panel.openEditor({ rule: modelData })
      onRemoveClicked: root.del(modelData)
    }
    Column {
      visible: root.shown.length === 0
      anchors.centerIn: parent
      spacing: Style.space(6)
      Txt { anchors.horizontalCenter: parent.horizontalCenter; text: I18n.t("noRules"); dim: true }
      Txt { anchors.horizontalCenter: parent.horizontalCenter; text: I18n.t("noRulesHint"); faint: true; font.pixelSize: Style.font.bodySmall }
    }
  }

  ListView {
    id: historyList
    visible: root.showHistory
    anchors.fill: list
    clip: true
    spacing: Style.space(2)
    model: root.history
    ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
    delegate: Rectangle {
      required property var modelData
      width: historyList.width - Style.space(6)
      height: Style.space(34)
      radius: Style.cornerRadius
      color: hm.containsMouse ? Util.alpha(Color.popups.text, 0.04) : "transparent"
      function describe(s) {
        if (!s) return "—"
        return s.value + " → " + s.target + (s.enabled === false ? " (" + I18n.t("off") + ")" : "")
      }
      Row {
        anchors.left: parent.left; anchors.leftMargin: Style.space(10)
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(10)
        Txt { text: "#" + modelData.id; faint: true; width: Style.space(36) }
        Txt { text: I18n.action(modelData.action); bold: true; width: Style.space(90) }
        Txt { text: root.svc.ago(modelData.at); dim: true; width: Style.space(90); font.pixelSize: Style.font.bodySmall }
        Txt { text: parent.parent.describe(modelData.before) + "  ⇒  " + parent.parent.describe(modelData.after); width: historyList.width - Style.space(380); font.pixelSize: Style.font.bodySmall }
      }
      Btn {
        anchors.right: parent.right; anchors.rightMargin: Style.space(6)
        anchors.verticalCenter: parent.verticalCenter
        visible: hm.containsMouse
        icon: Icons.undo
        text: I18n.t("rollbackHere")
        onClicked: root.rollbackTo(modelData)
      }
      MouseArea { id: hm; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
    }
  }

  Row {
    id: footer
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    spacing: Style.space(10)
    Txt { text: Icons.history; faint: true; anchors.verticalCenter: parent.verticalCenter }
    Txt {
      anchors.verticalCenter: parent.verticalCenter
      width: parent.width - undoBtn.width - Style.space(40)
      text: root.history.length ? I18n.t("lastChange").arg(I18n.action(root.history[0].action)).arg((root.history[0].after || root.history[0].before || {}).value || "").arg(root.svc.ago(root.history[0].at)) : I18n.t("noChanges")
      dim: true; font.pixelSize: Style.font.bodySmall
    }
    Btn { id: undoBtn; icon: Icons.undo; text: I18n.t("undo"); enabled: root.history.length > 0; onClicked: root.undoLast() }
  }
}
