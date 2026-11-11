import QtQuick
import QtQuick.Controls
import qs.Commons
import qs.Commons as Commons

// Proxy groups of the current profile; pick nodes and test latency.
Item {
  id: root
  property var svc
  property var panel

  property var groups: []
  property string open: ""
  property int cursor: 0          // group index
  property int member: -1         // member index inside the open group, -1 = on the group row
  property var testing: ({})
  readonly property bool typing: false

  function reload() {
    svc.request("proxies.list", null, function(r, err) {
      if (err || !r) return
      groups = r
      if (open === "" && r.length) open = r[0].name
    })
  }

  function select(group, name) {
    svc.request("proxies.select", { group: group.name, name: name }, function(r, err) {
      if (err) panel.notify(err.message)
      reload()
    })
  }

  function testGroup(group) {
    var t = Object.assign({}, testing); t[group.name] = true; testing = t
    svc.request("group.delay", { name: group.name }, function(r, err) {
      var n = Object.assign({}, testing); delete n[group.name]; testing = n
      if (err) panel.notify(err.message)
      reload()
    })
  }

  function testAllGroups() {
    for (var i = 0; i < groups.length; i++) if (groups[i].name !== "GLOBAL") testGroup(groups[i])
  }

  function move(dx, dy) {
    var g = groups[cursor]
    if (g && g.name === open && dx !== 0 && member >= 0) {
      member = Math.max(0, Math.min(g.members.length - 1, member + dx))
      return
    }
    if (g && g.name === open && dy > 0 && member < 0 && g.members.length) { member = 0; return }
    if (member >= 0 && dy !== 0) {
      var cols = 3
      var next = member + dy * cols
      if (next >= 0 && next < g.members.length) { member = next; return }
      if (dy < 0) { member = -1; return }
    }
    member = -1
    cursor = Math.max(0, Math.min(groups.length - 1, cursor + dy))
    list.positionViewAtIndex(cursor, ListView.Contain)
  }
  function activate() {
    var g = groups[cursor]
    if (!g) return
    if (member >= 0 && (g.kind === "Selector" || g.kind === "Fallback")) select(g, g.members[member].name)
    else { open = open === g.name ? "" : g.name; member = -1 }
  }
  function textKey(t) {
    if (t === "t" && groups[cursor]) testGroup(groups[cursor])
    else if (t === "r") reload()
  }

  Component.onCompleted: if (svc) reload()

  Row {
    id: head
    width: parent.width
    Column {
      width: parent.width - testAll.width
      spacing: Style.space(3)
      Txt { text: MihomoI18n.t("proxiesTitle"); bold: true; font.pixelSize: Style.font.heading }
      Txt {
        text: MihomoI18n.t("proxiesSubtitle").arg(root.svc.profileName || "—").arg(root.groups.length)
        dim: true; font.pixelSize: Style.font.body
      }
    }
    Btn { id: testAll; icon: Icons.test; text: MihomoI18n.t("testAll"); onClicked: root.testAllGroups() }
  }

  ListView {
    id: list
    anchors.top: head.bottom
    anchors.topMargin: Style.space(12)
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    clip: true
    spacing: Style.space(6)
    model: root.groups
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
    delegate: Rectangle {
      id: groupBox
      required property var modelData
      required property int index
      readonly property bool expanded: root.open === modelData.name
      readonly property bool selectable: modelData.kind === "Selector" || modelData.kind === "Fallback"
      readonly property bool hot: index === root.cursor && root.member < 0
      readonly property var nowMember: modelData.members.filter(function(m) { return m.name === modelData.now })[0]
      width: list.width - Style.space(6)
      height: headRow.height + (expanded ? grid.height + Style.space(10) : 0) + Style.space(16)
      radius: Style.cornerRadius
      color: expanded ? Util.alpha(Commons.Color.popups.text, 0.04) : (hot ? Style.hoverFillFor(Commons.Color.popups.text, Commons.Color.accent) : "transparent")
      border.width: expanded || hot ? 1 : 0
      border.color: Util.alpha(Commons.Color.popups.text, 0.14)

      Item {
        id: headRow
        x: Style.space(12); y: Style.space(8)
        width: parent.width - Style.space(24); height: Style.space(22)
        Row {
          anchors.verticalCenter: parent.verticalCenter
          spacing: Style.space(10)
          Txt { text: groupBox.expanded ? Icons.expanded : Icons.collapsed; faint: true }
          Txt { text: groupBox.modelData.name; bold: true }
          Txt { text: groupBox.modelData.kind; color: root.svc.mauve; font.pixelSize: Style.font.bodySmall }
          Txt { text: "→ " + (groupBox.modelData.now || "—"); dim: true; font.pixelSize: Style.font.body }
        }
        Row {
          anchors.right: parent.right
          anchors.verticalCenter: parent.verticalCenter
          spacing: Style.space(8)
          Txt {
            readonly property var d: groupBox.nowMember ? groupBox.nowMember.delay : null
            text: d === null || d === undefined ? "—" : (d === 0 ? MihomoI18n.t("timeout") : d + " " + MihomoI18n.t("ms"))
            color: d === null || d === undefined ? Util.alpha(Commons.Color.popups.text, 0.42) : (d === 0 ? root.svc.red : (d < 400 ? root.svc.green : root.svc.yellow))
            font.pixelSize: Style.font.body
            anchors.verticalCenter: parent.verticalCenter
          }
          Btn { ghost: true; icon: Icons.test; tooltip: MihomoI18n.t("hintTestGroup"); busy: !!root.testing[groupBox.modelData.name]; onClicked: root.testGroup(groupBox.modelData) }
        }
        MouseArea {
          anchors.fill: parent
          anchors.rightMargin: Style.space(80)
          cursorShape: Qt.PointingHandCursor
          onClicked: { root.cursor = groupBox.index; root.member = -1; root.open = groupBox.expanded ? "" : groupBox.modelData.name }
        }
      }

      Grid {
        id: grid
        visible: groupBox.expanded
        x: Style.space(12)
        anchors.top: headRow.bottom
        anchors.topMargin: Style.space(10)
        width: parent.width - Style.space(24)
        columns: 3
        spacing: Style.space(6)
        Repeater {
          model: groupBox.expanded ? groupBox.modelData.members : []
          Rectangle {
            required property var modelData
            required property int index
            readonly property bool isNow: modelData.name === groupBox.modelData.now
            readonly property bool hot: groupBox.index === root.cursor && root.member === index
            readonly property var d: modelData.delay
            width: (grid.width - Style.space(12)) / 3
            height: Style.space(32)
            radius: Style.cornerRadius
            color: isNow ? Style.selectedFillFor(Commons.Color.popups.text, Commons.Color.accent) : (hot || cm.containsMouse ? Style.hoverFillFor(Commons.Color.popups.text, Commons.Color.accent) : Util.alpha(Commons.Color.popups.text, 0.04))
            border.width: 1
            border.color: isNow ? Commons.Color.accent : Util.alpha(Commons.Color.popups.text, hot ? 0.4 : 0.14)
            Row {
              anchors.left: parent.left; anchors.leftMargin: Style.space(10)
              anchors.right: delayText.left; anchors.rightMargin: Style.space(6)
              anchors.verticalCenter: parent.verticalCenter
              spacing: Style.space(8)
              Rectangle {
                width: Style.space(6); height: width; radius: width / 2
                anchors.verticalCenter: parent.verticalCenter
                color: d === null || d === undefined ? Commons.Color.muted : (d === 0 ? root.svc.red : (d < 400 ? root.svc.green : root.svc.yellow))
              }
              Txt { text: modelData.name; bold: isNow; width: parent.width - Style.space(14) }
            }
            Txt {
              id: delayText
              anchors.right: parent.right; anchors.rightMargin: Style.space(10)
              anchors.verticalCenter: parent.verticalCenter
              text: d === null || d === undefined ? "" : (d === 0 ? "✕" : d)
              faint: true; font.pixelSize: Style.font.bodySmall
            }
            MouseArea {
              id: cm
              anchors.fill: parent
              hoverEnabled: true
              cursorShape: groupBox.selectable ? Qt.PointingHandCursor : Qt.ArrowCursor
              onClicked: if (groupBox.selectable) root.select(groupBox.modelData, modelData.name)
            }
          }
        }
      }
    }
  }
}
