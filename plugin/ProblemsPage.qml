import QtQuick
import QtQuick.Controls
import qs.Commons
import "ProblemModels.js" as ProblemModels

// Problem resources: failed connections merged per host.
Item {
  id: root
  property var svc
  property var panel

  property string tab: "new"     // new | all | hidden
  property string kind: ""       // error kind filter
  property string search: ""
  property var items: []
  property var counts: ({ "new": 0, "visible": 0, "hidden": 0 })
  property var ignores: []
  property int cursor: 0
  property bool loading: false
  property int requestRevision: 0
  property bool ready: false
  property string viewMode: panel ? panel.problemsView : "table"
  property var collapsedGroups: ({})
  property string selectedKey: ""
  readonly property var rows: ProblemModels.rows(items, viewMode === "groups", collapsedGroups)
  readonly property bool typing: searchField.input.activeFocus

  readonly property var kindFilters: ["", "timeout", "reset", "refused", "dns_fail", "dns_stub", "no_route", "tls", "proxy", "other"]

  function reload() {
    if (!ready || !svc) return
    var revision = ++requestRevision
    loading = true
    var q = { only_new: tab === "new", hidden: tab === "hidden", search: search || null, kind: kind || null, limit: 300 }
    svc.request("problems.list", q, function(r, err) {
      if (revision !== requestRevision) return
      loading = false
      if (err || !r) { if (err) panel.notify(err.message); return }
      items = r.items
      counts = r.counts
    })
    if (tab === "hidden") svc.request("problems.ignores", null, function(r) { if (r) ignores = r })
  }

  function move(dx, dy) {
    if (dx !== 0) {
      var tabs = ["new", "all", "hidden"]
      tab = tabs[(tabs.indexOf(tab) + dx + tabs.length) % tabs.length]
      return
    }
    select(Math.max(0, Math.min(rows.length - 1, cursor + dy)))
    list.positionViewAtIndex(cursor, ListView.Contain)
  }
  function select(index) {
    cursor = index
    selectedKey = rows[index] ? rows[index].key : ""
  }
  function restoreSelection() {
    var index = rows.findIndex(function(r) { return r.key === selectedKey })
    select(index >= 0 ? index : Math.min(cursor, Math.max(0, rows.length - 1)))
    Qt.callLater(function() { if (rows.length) list.positionViewAtIndex(cursor, ListView.Contain) })
  }
  function openGroup(group) { panel.openEditor({ group: group, problem: group.items[0] }) }
  function activate() {
    var row = rows[cursor]
    if (!row) return
    if (row.group) openGroup(row.group)
    else panel.openEditor({ problem: row.problem })
  }
  function remove() { if (rows[cursor] && rows[cursor].problem) hide(rows[cursor].problem) }
  function toggleGroup(group) {
    selectedKey = "group:" + group.key
    var next = Object.assign({}, collapsedGroups)
    next[group.key] = !next[group.key]
    collapsedGroups = next
  }
  function textKey(t) {
    if (t === "/") searchField.input.forceActiveFocus()
    else if (t === "s") markSeen()
    else if (t === "r") reload()
    else if (t === "g" && rows[cursor] && rows[cursor].group) toggleGroup(rows[cursor].group)
  }

  function hide(p) {
    if (p.hidden) { tab = "hidden"; return }
    svc.request("problems.hide", { kind: "host", value: p.host }, function(r, err) {
      if (err) panel.notify(err.message)
      else panel.notify(I18n.t("hiddenToast").arg(p.host))
      reload()
    })
  }

  function markSeen() {
    svc.request("problems.mark_seen", null, function() { reload() })
  }

  onTabChanged: { cursor = 0; selectedKey = ""; reload() }
  onKindChanged: reload()
  onRowsChanged: restoreSelection()
  Component.onCompleted: { ready = true; reload() }
  Connections {
    target: root.svc
    function onProblemsRevisionChanged() { if (!root.loading) refreshTimer.restart() }
  }
  // Coalesce bursts of new failures into one refresh.
  Timer { id: refreshTimer; interval: 800; onTriggered: root.reload() }

  Column {
    id: head
    anchors.left: parent.left
    anchors.right: parent.right
    spacing: Style.space(12)

    Row {
      width: parent.width
      spacing: Style.space(10)
      Column {
        width: parent.width - searchField.width - seenBtn.width - Style.space(20)
        spacing: Style.space(3)
        Txt { text: I18n.t("problemsTitle"); bold: true; font.pixelSize: Style.font.heading }
        Txt {
          text: I18n.t("problemsSubtitle").arg(root.counts.visible).arg(root.counts["new"]).arg(root.counts.hidden)
          dim: true; font.pixelSize: Style.font.bodySmall
        }
      }
      Field {
        id: searchField
        width: Style.space(190)
        anchors.verticalCenter: parent.verticalCenter
        icon: Icons.search
        placeholder: I18n.t("searchHint")
        onTextChanged: { root.search = text; searchTimer.restart() }
        onEscaped: { text = ""; input.focus = false; root.panel.forceKeys() }
        onAccepted: { input.focus = false; root.panel.forceKeys() }
        Timer { id: searchTimer; interval: 250; onTriggered: root.reload() }
      }
      Btn { id: seenBtn; anchors.verticalCenter: parent.verticalCenter; icon: Icons.seen; text: I18n.t("markSeen"); enabled: root.counts["new"] > 0; onClicked: root.markSeen() }
    }

    Row {
      width: parent.width
      spacing: Style.space(10)
      Seg {
        current: root.tab
        model: [
          { key: "new", label: I18n.t("tabNew") + " " + root.counts["new"] },
          { key: "all", label: I18n.t("tabAll") + " " + root.counts.visible },
          { key: "hidden", label: I18n.t("tabHidden") + " " + root.counts.hidden }
        ]
        onPicked: function(k) { root.tab = k }
      }
      Seg {
        current: root.viewMode
        model: [{ key: "table", label: I18n.t("problemTable") }, { key: "groups", label: I18n.t("problemGroups") }]
        onPicked: function(k) { root.panel.problemsView = k }
      }
    }

    Flow {
      width: parent.width
      spacing: Style.space(6)
      Repeater {
        model: root.kindFilters.slice(1)
        Chip {
          required property string modelData
          text: I18n.kind(modelData)
          dot: root.svc.kindColor(modelData)
          color: root.kind === modelData ? Style.selectedFillFor(Color.popups.text, Color.accent) : Util.alpha(Color.popups.text, 0.04)
          MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.kind = root.kind === modelData ? "" : modelData }
        }
      }
    }

    Banner {
      width: parent.width
      tone: "info"
      text: I18n.t("diagnosisHint")
    }

    // What is hidden, with a way back.
    Flow {
      visible: root.tab === "hidden" && root.ignores.length > 0
      width: parent.width
      spacing: Style.space(6)
      Repeater {
        model: root.ignores
        Chip {
          required property var modelData
          showDot: false
          text: I18n.ignoreKind(modelData.kind) + ": " + modelData.value + "  " + Icons.close
          MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: root.svc.request("problems.unhide", { id: modelData.id }, function() { root.reload() })
          }
        }
      }
    }
  }

  ProblemTableRow {
    id: tableHeader
    visible: root.viewMode === "table"
    anchors.top: head.bottom
    anchors.topMargin: Style.space(10)
    width: list.width - Style.space(6)
    height: visible ? implicitHeight : 0
    header: true
    svc: root.svc
  }

  ListView {
    id: list
    anchors.top: tableHeader.bottom
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: footer.top
    anchors.bottomMargin: Style.space(8)
    clip: true
    spacing: 0
    model: root.rows
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
    delegate: Item {
      id: entry
      required property var modelData
      required property int index
      width: list.width - Style.space(6)
      height: content.height + (modelData.last ? Style.space(10) : 0)
      Item {
        visible: root.viewMode === "groups"
        width: parent.width; height: content.height
        clip: true
        Rectangle {
          width: parent.width
          y: entry.modelData.group ? 0 : -radius
          height: parent.height + (entry.modelData.group ? 0 : radius) + (entry.modelData.last ? 0 : radius)
          radius: Style.cornerRadius
          color: Util.alpha(Color.popups.text, 0.04)
          border.width: 1
          border.color: Util.alpha(Color.popups.text, 0.15)
        }
        Rectangle {
          visible: !!entry.modelData.group && entry.index === root.cursor
          anchors.fill: parent
          radius: Style.cornerRadius
          color: Style.hoverFillFor(Color.popups.text, Color.accent)
        }
      }
      Loader {
        id: content
        width: parent.width
        sourceComponent: entry.modelData.group ? groupHeader : problemRow
      }
      Component {
        id: groupHeader
        ProblemGroupHeader {
          group: entry.modelData.group
          svc: root.svc
          collapsed: entry.modelData.closed
          onToggled: root.toggleGroup(group)
          onRouteClicked: root.openGroup(group)
        }
      }
      Component {
        id: problemRow
        ProblemTableRow {
          problem: entry.modelData.problem
          svc: root.svc
          grouped: root.viewMode === "groups"
          selected: entry.index === root.cursor
          onClicked: root.select(entry.index)
          onRouteClicked: root.panel.openEditor({ problem: problem })
          onHideClicked: root.hide(problem)
        }
      }
    }

    Column {
      visible: root.items.length === 0 && !root.loading
      anchors.centerIn: parent
      spacing: Style.space(6)
      Txt { anchors.horizontalCenter: parent.horizontalCenter; text: Icons.check; color: root.svc.green; font.pixelSize: Style.font.display }
      Txt { anchors.horizontalCenter: parent.horizontalCenter; text: root.tab === "new" ? I18n.t("noNewProblems") : I18n.t("noProblems"); dim: true }
    }
  }

  Column {
    id: footer
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    spacing: Style.space(10)
    Row {
      spacing: Style.space(5)
      Rectangle { anchors.verticalCenter: parent.verticalCenter; width: Style.space(6); height: width; radius: width / 2; color: root.svc.green }
      Txt { text: I18n.t("problemRecovered"); faint: true; font.pixelSize: Style.font.caption }
    }
    Flow {
      width: parent.width
      spacing: Style.space(14)
      Repeater {
        model: [["j/k", I18n.t("keyNav")], ["Enter", I18n.t("keyRoute")], ["x", I18n.t("keyHide")], ["/", I18n.t("keySearch")], ["s", I18n.t("keySeen")]].concat(root.viewMode === "groups" ? [["g", I18n.t("keyGroup")]] : [])
        Row {
          required property var modelData
          spacing: Style.space(5)
          Rectangle {
            width: k.implicitWidth + Style.space(8); height: k.implicitHeight + Style.space(2)
            radius: Style.space(3)
            color: "transparent"
            border.width: 1
            border.color: Util.alpha(Color.popups.text, 0.18)
            Txt { id: k; anchors.centerIn: parent; text: modelData[0]; dim: true; font.pixelSize: Style.font.caption }
          }
          Txt { text: modelData[1]; faint: true; font.pixelSize: Style.font.caption }
        }
      }
    }
  }
}
