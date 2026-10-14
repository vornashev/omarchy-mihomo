import QtQuick
import QtQuick.Controls
import qs.Commons

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
  readonly property bool typing: searchField.input.activeFocus

  readonly property var kindFilters: ["", "timeout", "reset", "refused", "dns_fail", "dns_stub", "tls", "proxy", "other"]

  function reload() {
    loading = true
    var q = { only_new: tab === "new", hidden: tab === "hidden", search: search || null, kind: kind || null, limit: 300 }
    svc.request("problems.list", q, function(r, err) {
      loading = false
      if (err || !r) return
      items = r.items
      counts = r.counts
      cursor = Math.min(cursor, Math.max(0, items.length - 1))
    })
    if (tab === "hidden") svc.request("problems.ignores", null, function(r) { if (r) ignores = r })
  }

  function move(dx, dy) {
    if (dx !== 0) {
      var tabs = ["new", "all", "hidden"]
      tab = tabs[(tabs.indexOf(tab) + dx + tabs.length) % tabs.length]
      return
    }
    cursor = Math.max(0, Math.min(items.length - 1, cursor + dy))
    list.positionViewAtIndex(cursor, ListView.Contain)
  }
  function activate() { if (items[cursor]) panel.openEditor({ problem: items[cursor] }) }
  function remove() { if (items[cursor]) hide(items[cursor]) }
  function textKey(t) {
    if (t === "/") searchField.input.forceActiveFocus()
    else if (t === "s") markSeen()
    else if (t === "r") reload()
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

  onTabChanged: { cursor = 0; reload() }
  onKindChanged: reload()
  Component.onCompleted: reload()
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
      Item { width: 1; height: 1 }
      Repeater {
        model: root.kindFilters.slice(1, 6)
        Chip {
          required property string modelData
          anchors.verticalCenter: parent.verticalCenter
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

  ListView {
    id: list
    anchors.top: head.bottom
    anchors.topMargin: Style.space(10)
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: footer.top
    anchors.bottomMargin: Style.space(8)
    clip: true
    spacing: Style.space(2)
    model: root.items
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
    delegate: ProblemRow {
      required property var modelData
      required property int index
      width: list.width - Style.space(6)
      problem: modelData
      svc: root.svc
      selected: index === root.cursor
      onClicked: root.cursor = index
      onRouteClicked: root.panel.openEditor({ problem: modelData })
      onHideClicked: root.hide(modelData)
    }

    Column {
      visible: root.items.length === 0 && !root.loading
      anchors.centerIn: parent
      spacing: Style.space(6)
      Txt { anchors.horizontalCenter: parent.horizontalCenter; text: Icons.check; color: root.svc.green; font.pixelSize: Style.font.display }
      Txt { anchors.horizontalCenter: parent.horizontalCenter; text: root.tab === "new" ? I18n.t("noNewProblems") : I18n.t("noProblems"); dim: true }
    }
  }

  Row {
    id: footer
    anchors.left: parent.left
    anchors.bottom: parent.bottom
    spacing: Style.space(14)
    Repeater {
      model: [["j/k", I18n.t("keyNav")], ["Enter", I18n.t("keyRoute")], ["x", I18n.t("keyHide")], ["/", I18n.t("keySearch")], ["s", I18n.t("keySeen")]]
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
