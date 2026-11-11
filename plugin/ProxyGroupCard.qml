pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons
import qs.Commons as Commons
import "ProxyModels.js" as Models

Rectangle {
  id: root
  required property var entry
  property var runtime: ({})
  property var svc
  // Bytes through this group over the last week, {up, down}, or null.
  property var traffic: null
  // Delay history per node, from latency.query.
  property var latency: ({})
  property bool testing: false
  property bool draggable: false
  readonly property bool popupOpen: quickSwitch.visible
  readonly property bool selectable: entry.type === "select" || entry.type === "fallback"
  signal editRequested
  signal testRequested
  signal menuRequested(var anchor)
  signal selected(string name)
  // Scene coordinates, so the list can map them whatever scrolls in between.
  signal dragMoved(point scene)
  signal dragEnded
  readonly property var members: runtime.members || (entry.members || []).map(function (n) {
    return {
      name: n
    };
  })
  readonly property string current: runtime.now || entry.now || ""
  width: parent.width
  implicitHeight: body.implicitHeight + Style.space(20)
  radius: Style.cornerRadius
  color: Util.alpha(Commons.Color.popups.text, 0.04)
  border.width: 1
  border.color: Util.alpha(Commons.Color.popups.text, 0.15)
  Column {
    id: body
    x: Style.space(10)
    y: Style.space(10)
    width: parent.width - Style.space(20)
    spacing: Style.space(8)
    Row {
      id: header
      width: parent.width
      spacing: Style.space(6)
      Txt {
        id: dragHandle
        visible: root.draggable
        text: "⠿"
        faint: true
        width: Style.space(12)
        height: groupTitle.height
        horizontalAlignment: Text.AlignHCenter
        MouseArea {
          objectName: "proxyGroupDragHandle"
          anchors.fill: parent
          anchors.margins: -Style.space(4)
          preventStealing: true
          cursorShape: pressed ? Qt.ClosedHandCursor : Qt.OpenHandCursor
          onPositionChanged: function (m) {
            root.dragMoved(mapToItem(null, m.x, m.y));
          }
          onReleased: root.dragEnded()
          onCanceled: root.dragEnded()
        }
      }
      Txt {
        id: typeIcon
        text: root.entry.type === "select" ? Icons.proxies : root.entry.type === "fallback" ? Icons.shield : root.entry.type === "url-test" ? Icons.test : Icons.connections
        color: Commons.Color.accent
        width: Style.space(16)
        height: groupTitle.height
        horizontalAlignment: Text.AlignHCenter
      }
      Column {
        width: header.width - typeIcon.width - actions.width - header.spacing * 2
          - (dragHandle.visible ? dragHandle.width + header.spacing : 0)
        spacing: Style.space(4)
        Txt {
          id: groupTitle
          width: parent.width
          text: root.entry.name + (root.entry.source === "modified" ? " · " + MihomoI18n.t("proxySourceModified") : "")
          bold: true
          elide: Text.ElideRight
        }
        Txt {
          width: parent.width
          text: (root.entry.name === "GLOBAL" ? MihomoI18n.t("proxyBuiltin") : MihomoI18n.t("proxyType_" + root.entry.type) + " · " + MihomoI18n.t("proxyTypeHint_" + root.entry.type))
            + (root.traffic && root.svc && root.svc.formatBytes ? " · " + MihomoI18n.t("trafficWeek").arg(root.svc.formatBytes(root.traffic.up + root.traffic.down)) : "")
          dim: true
          font.pixelSize: Style.font.bodySmall
          elide: Text.ElideRight
        }
      }
      Row {
        id: actions
        y: (groupTitle.height - height) / 2
        spacing: Style.space(2)
        Btn {
          ghost: true
          icon: Icons.test
          tooltip: MihomoI18n.t("hintTestGroup")
          busy: root.testing
          onClicked: root.testRequested()
        }
        Btn {
          ghost: true
          icon: Icons.edit
          enabled: root.entry.name !== "GLOBAL"
          tooltip: MihomoI18n.t("proxyEdit")
          onClicked: root.editRequested()
        }
        Btn {
          id: menuButton
          objectName: "proxyGroupMenuButton"
          ghost: true
          text: "⋮"
          enabled: root.entry.name !== "GLOBAL"
          tooltip: MihomoI18n.t("proxySection_advanced")
          onClicked: root.menuRequested(menuButton)
        }
      }
    }
    Txt {
      visible: root.current !== ""
      text: MihomoI18n.t("proxyNow") + "  " + root.current
      color: Commons.Color.accent
      font.pixelSize: Style.font.bodySmall
      width: parent.width
      elide: Text.ElideRight
    }
    Flow {
      width: parent.width
      spacing: Style.space(6)
      Repeater {
        model: root.members.slice(0, 3)
        Rectangle {
          id: member
          required property var modelData
          required property int index
          readonly property var history: root.latency[modelData.name] || null
          width: Math.min(memberLabel.implicitWidth + latency.implicitWidth + (spark.visible ? spark.width + Style.space(6) : 0) + Style.space(22), parent.width)
          height: Style.space(25)
          radius: Style.cornerRadius
          color: modelData.name === root.current ? Util.alpha(Commons.Color.accent, 0.12) : Util.alpha(Commons.Color.popups.text, 0.04)
          border.width: 1
          border.color: modelData.name === root.current ? Commons.Color.accent : Util.alpha(Commons.Color.popups.text, 0.15)
          Row {
            anchors.centerIn: parent
            width: parent.width - Style.space(16)
            spacing: Style.space(6)
            Txt {
              id: memberLabel
              width: Math.min(implicitWidth, parent.width - latency.width - (spark.visible ? spark.width + parent.spacing : 0) - parent.spacing)
              text: (root.entry.type === "fallback" ? (member.index + 1) + "  " : "") + member.modelData.name
              font.pixelSize: Style.font.body
              bold: member.modelData.name === root.current
              dim: !bold
            }
            Txt {
              id: latency
              text: member.modelData.delay === 0 ? MihomoI18n.t("proxyNoResponse") : Models.delayText(member.modelData.delay, MihomoI18n.t("ms"))
              font.pixelSize: Style.font.bodySmall
              color: member.modelData.delay == null ? Util.alpha(Commons.Color.popups.text, 0.42)
                : member.modelData.delay === 0 ? (root.svc ? root.svc.red : Commons.Color.urgent)
                : member.modelData.delay < 150 ? (root.svc ? root.svc.green : Commons.Color.accent) : (root.svc ? root.svc.yellow : Commons.Color.accent)
            }
            Sparkline {
              id: spark
              objectName: "memberSpark"
              visible: !!member.history
              width: Style.space(36)
              height: Style.space(11)
              anchors.verticalCenter: parent.verticalCenter
              points: member.history ? member.history.points : []
              color: Models.latencyLoss(member.history) >= 10 ? (root.svc ? root.svc.yellow : Commons.Color.accent) : (root.svc ? root.svc.green : Commons.Color.accent)
              failColor: root.svc ? root.svc.red : Commons.Color.urgent
            }
          }
          HoverHandler { id: memberHover }
          Hint { visible: memberHover.hovered && !!member.history; text: Models.latencyHint(member.history, MihomoI18n.t) }
          MouseArea {
            anchors.fill: parent
            cursorShape: root.selectable ? Qt.PointingHandCursor : Qt.ArrowCursor
            onClicked: if (root.selectable)
              root.selected(member.modelData.name)
          }
        }
      }
      Btn {
        visible: root.members.length > 3
        ghost: true
        id: moreButton
        objectName: "proxyGroupMoreButton"
        text: "+" + (root.members.length - 3)
        hasCursor: quickSwitch.visible
        onClicked: quickSwitch.visible ? quickSwitch.close() : quickSwitch.open()
        ProxyQuickSwitch {
          id: quickSwitch
          x: Math.min(0, moreButton.width - width)
          y: moreButton.height + Style.space(4)
          groupName: root.entry.name
          members: root.members
          current: root.current
          selectable: root.selectable
          editable: root.entry.name !== "GLOBAL"
          testing: root.testing
          svc: root.svc
          onSelected: function(name) { root.selected(name); }
          onTestRequested: root.testRequested()
          onEditRequested: root.editRequested()
          onClosed: moreButton.forceActiveFocus()
        }
      }
    }
  }
}
