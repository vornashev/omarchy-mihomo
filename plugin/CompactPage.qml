import QtQuick
import qs.Commons
import "." as Mihomo
import qs.Commons as Commons
import "TrafficModels.js" as TM

// The compact window: status, mode, TUN, speed, current proxies and the
// newest problems. Anything deeper opens the full interface.
Item {
  id: root
  property var svc
  property var panel

  property var groups: []
  property var recent: []
  property var hour: null
  property bool applying: false
  readonly property bool typing: false

  implicitHeight: col.implicitHeight

  function reload() {
    svc.request("problems.list", { only_new: true, limit: 3 }, function(r) { if (r) root.recent = r.items })
    svc.request("proxies.list", null, function(r) {
      if (r) root.groups = r.filter(function(g) { return g.kind === "Selector" && g.name !== "GLOBAL" }).slice(0, 3)
    })
    svc.request("traffic.query", { from: Date.now() - TM.HOUR, by: "app", limit: 0, rows: 1 }, function(r) { if (r) root.hour = r })
  }

  function apply() {
    applying = true
    svc.request("config.apply", null, function(r, err) {
      root.applying = false
      root.panel.notify(err ? err.message : MihomoI18n.t("configApplied"))
    })
  }

  function select(group, name) {
    svc.request("proxies.select", { group: group, name: name }, function(r, err) {
      if (err) root.panel.notify(err.message)
      root.reload()
    })
  }

  function activate() { panel.openFull() }

  function textKey(t) {
    if (t === "r") svc.request("mode.set", { mode: "rule" }, null)
    else if (t === "g") svc.request("mode.set", { mode: "global" }, null)
    else if (t === "d") svc.request("mode.set", { mode: "direct" }, null)
    else if (t === "t") svc.request("tun.set", { enable: !svc.tun }, null)
  }

  function delayText(d) { return d === null || d === undefined ? "—" : (d === 0 ? MihomoI18n.t("timeout") : d + " " + MihomoI18n.t("ms")) }
  function delayColor(d) { return d === null || d === undefined ? Util.alpha(Commons.Color.popups.text, 0.42) : (d === 0 ? svc.red : (d < 400 ? svc.green : svc.yellow)) }

  Component.onCompleted: reload()
  Timer { interval: 15000; running: true; repeat: true; onTriggered: root.reload() }
  Connections {
    target: root.svc
    function onSystemProxyFailed(message) { root.panel.notify(message) }
  }

  Column {
    id: col
    width: parent.width
    spacing: Style.space(12)

    Row {
      width: parent.width
      spacing: Style.space(10)
      Txt {
        anchors.verticalCenter: parent.verticalCenter
        text: Icons.shield; color: root.svc.coreRunning ? Commons.Color.accent : Commons.Color.muted; font.pixelSize: Style.font.heading
      }
      Column {
        anchors.verticalCenter: parent.verticalCenter
        width: parent.width - x - headButtons.width - Style.space(10)
        spacing: Style.space(2)
        Txt { text: "mihomo"; bold: true }
        Row {
          spacing: Style.space(6)
          Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: Style.space(6); height: width; radius: width / 2
            color: root.svc.coreRunning ? root.svc.green : root.svc.red
          }
          Txt {
            text: root.svc.coreRunning ? MihomoI18n.t("coreRunning") + " · " + (root.svc.status.core.version || "") : (root.svc.connected ? MihomoI18n.t("coreStopped") : MihomoI18n.t("noDaemon"))
            faint: true; font.pixelSize: Style.font.bodySmall
          }
        }
      }
      Row {
        id: headButtons
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(4)
        Btn { ghost: true; icon: Icons.refresh; tooltip: MihomoI18n.t("applyConfig"); busy: root.applying; onClicked: root.apply() }
        Btn { objectName: "compactSettings"; ghost: true; icon: Icons.settings; tooltip: MihomoI18n.t("navSettings"); onClicked: root.panel.showPage("settings") }
      }
    }

    Seg {
      objectName: "compactMode"
      width: parent.width
      stretch: true
      current: root.svc.mode
      model: [{ key: "rule", label: MihomoI18n.mode("rule") }, { key: "global", label: MihomoI18n.mode("global") }, { key: "direct", label: MihomoI18n.mode("direct") }]
      onPicked: function(k) { root.svc.request("mode.set", { mode: k }, null) }
    }

    Rectangle {
      width: parent.width
      height: toggles.implicitHeight
      radius: Style.cornerRadius
      color: Util.alpha(Commons.Color.popups.text, 0.04)
      border.width: 1
      border.color: Util.alpha(Commons.Color.popups.text, 0.12)
      Column {
        id: toggles
        width: parent.width
        Row {
          width: parent.width
          padding: Style.space(14)
          Column {
            width: parent.width - tunSwitch.width - 2 * parent.padding
            spacing: Style.space(2)
            Txt { text: "TUN" }
            Txt { text: MihomoI18n.t("tunHint"); dim: true; font.pixelSize: Style.font.bodySmall }
          }
          Mihomo.Switch { id: tunSwitch; anchors.verticalCenter: parent.verticalCenter; checked: root.svc.tun; onToggled: root.svc.request("tun.set", { enable: !root.svc.tun }, null) }
        }
        Rectangle { width: parent.width; height: 1; color: Util.alpha(Commons.Color.popups.text, 0.1) }
        Item {
          width: parent.width
          height: proxyControl.implicitHeight + Style.space(28)
          SystemProxyControl { id: proxyControl; x: Style.space(14); y: Style.space(14); width: parent.width - Style.space(28); svc: root.svc }
        }
      }
    }

    Row {
      width: parent.width
      spacing: Style.space(10)
      Repeater {
        model: [
          [MihomoI18n.t("statDown"), root.svc.formatRate(root.svc.traffic.down), root.hour ? MihomoI18n.t("statHour").arg(root.svc.formatBytes(root.hour.total.down)) : ""],
          [MihomoI18n.t("statUp"), root.svc.formatRate(root.svc.traffic.up), root.hour ? MihomoI18n.t("statHour").arg(root.svc.formatBytes(root.hour.total.up)) : ""]
        ]
        Rectangle {
          required property var modelData
          width: (parent.width - Style.space(10)) / 2
          height: tileCol.implicitHeight + Style.space(24)
          radius: Style.cornerRadius
          color: Util.alpha(Commons.Color.popups.text, 0.04)
          border.width: 1
          border.color: Util.alpha(Commons.Color.popups.text, 0.12)
          Column {
            id: tileCol
            x: Style.space(12); y: Style.space(12)
            width: parent.width - Style.space(24)
            spacing: Style.space(4)
            Txt { text: modelData[0]; faint: true; font.pixelSize: Style.font.bodySmall }
            Txt { text: modelData[1]; bold: true; font.pixelSize: Style.font.heading }
            Txt { visible: modelData[2] !== ""; text: modelData[2]; dim: true; font.pixelSize: Style.font.bodySmall }
          }
          MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.panel.showPage("traffic") }
        }
      }
    }

    Rectangle {
      objectName: "compactProxies"
      visible: root.groups.length > 0
      width: parent.width
      height: proxiesCol.implicitHeight + Style.space(26)
      radius: Style.cornerRadius
      color: Util.alpha(Commons.Color.popups.text, 0.04)
      border.width: 1
      border.color: Util.alpha(Commons.Color.popups.text, 0.12)
      Column {
        id: proxiesCol
        x: Style.space(14); y: Style.space(13)
        width: parent.width - Style.space(28)
        spacing: Style.space(10)
        Row {
          width: parent.width
          Txt { text: MihomoI18n.t("compactProxies"); faint: true; bold: true; font.pixelSize: Style.font.bodySmall; font.letterSpacing: 1; width: parent.width - proxiesAll.width }
          Txt {
            id: proxiesAll
            text: MihomoI18n.t("allLink"); color: Commons.Color.accent; font.pixelSize: Style.font.bodySmall
            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.panel.showPage("proxies") }
          }
        }
        Repeater {
          model: root.groups
          Item {
            id: groupRow
            required property var modelData
            readonly property var nowMember: modelData.members.filter(function(m) { return m.name === modelData.now })[0]
            width: proxiesCol.width
            height: Style.space(20)
            Txt { id: groupName; width: Style.space(70); anchors.verticalCenter: parent.verticalCenter; text: groupRow.modelData.name; faint: true; bold: true; font.pixelSize: Style.font.bodySmall }
            Txt {
              anchors.left: groupName.right; anchors.leftMargin: Style.space(10)
              anchors.right: delay.left; anchors.rightMargin: Style.space(10)
              anchors.verticalCenter: parent.verticalCenter
              text: groupRow.modelData.now || "—"; bold: true; font.pixelSize: Style.font.body
            }
            Txt {
              id: delay
              anchors.right: chevron.left; anchors.rightMargin: Style.space(8)
              anchors.verticalCenter: parent.verticalCenter
              readonly property var d: groupRow.nowMember ? groupRow.nowMember.delay : null
              text: root.delayText(d); color: root.delayColor(d); font.pixelSize: Style.font.bodySmall
            }
            Txt { id: chevron; anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; text: Icons.expanded; faint: true }
            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: quickSwitch.open() }
            ProxyQuickSwitch {
              id: quickSwitch
              x: parent.width - width
              y: parent.height + Style.space(4)
              groupName: groupRow.modelData.name
              members: groupRow.modelData.members
              current: groupRow.modelData.now || ""
              editable: false
              svc: root.svc
              onSelected: function(name) { root.select(groupRow.modelData.name, name) }
              onTestRequested: root.svc.request("group.delay", { name: groupRow.modelData.name }, function() { root.reload() })
              onClosed: root.panel.forceKeys()
            }
          }
        }
      }
    }

    Rectangle {
      objectName: "compactProblems"
      visible: root.recent.length > 0
      width: parent.width
      height: problemsCol.implicitHeight + Style.space(26)
      radius: Style.cornerRadius
      color: Util.alpha(Commons.Color.popups.text, 0.04)
      border.width: 1
      border.color: Util.alpha(Commons.Color.popups.text, 0.12)
      Column {
        id: problemsCol
        x: Style.space(14); y: Style.space(13)
        width: parent.width - Style.space(28)
        spacing: Style.space(12)
        Row {
          width: parent.width
          spacing: Style.space(8)
          Txt { text: MihomoI18n.t("recentProblems"); faint: true; bold: true; font.pixelSize: Style.font.bodySmall; font.letterSpacing: 1 }
          Txt { text: String(root.svc.newProblems); color: root.svc.red; bold: true; font.pixelSize: Style.font.bodySmall }
          Item { width: parent.width - x - problemsAll.width - parent.spacing; height: 1 }
          Txt {
            id: problemsAll
            text: MihomoI18n.t("allLink"); color: Commons.Color.accent; font.pixelSize: Style.font.bodySmall
            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.panel.showPage("problems") }
          }
        }
        Repeater {
          model: root.recent
          Item {
            id: problemRow
            required property var modelData
            readonly property var kinds: modelData.kinds ? Object.keys(modelData.kinds) : []
            readonly property var route: modelData.route || {}
            width: problemsCol.width
            height: Style.space(34)
            Rectangle {
              id: problemDot
              anchors.verticalCenter: parent.verticalCenter
              width: Style.space(6); height: width; radius: width / 2
              color: problemRow.kinds.length ? root.svc.kindColor(problemRow.kinds[0]) : Commons.Color.urgent
            }
            Column {
              anchors.left: problemDot.right; anchors.leftMargin: Style.space(12)
              anchors.right: routeBtn.left; anchors.rightMargin: Style.space(12)
              anchors.verticalCenter: parent.verticalCenter
              spacing: Style.space(2)
              Txt { width: parent.width; text: problemRow.modelData.name || problemRow.modelData.domain_hint || problemRow.modelData.host; bold: true; font.pixelSize: Style.font.body }
              Row {
                width: parent.width
                spacing: Style.space(8)
                Txt {
                  text: problemRow.kinds.slice(0, 2).map(function(k) { return MihomoI18n.kind(k) }).join(" · ")
                  color: problemDot.color; font.pixelSize: Style.font.bodySmall
                }
                Txt {
                  width: parent.width - x
                  text: problemRow.route.rule ? problemRow.route.rule + " → " + problemRow.route.target : (problemRow.route.target || "")
                  faint: true; font.pixelSize: Style.font.bodySmall
                }
              }
            }
            Btn {
              id: routeBtn
              anchors.right: parent.right
              anchors.verticalCenter: parent.verticalCenter
              icon: Icons.route
              tooltip: MihomoI18n.t("route")
              onClicked: root.panel.openEditor({ problem: problemRow.modelData })
            }
          }
        }
      }
    }

    Row {
      width: parent.width
      spacing: Style.space(10)
      Btn {
        objectName: "compactOpenFull"
        width: parent.width - escHint.width - parent.spacing
        primary: true
        icon: Icons.arrow
        text: MihomoI18n.t("openFull")
        keyHint: "Enter"
        onClicked: root.panel.openFull()
      }
      KeyHint { id: escHint; key: "esc"; anchors.verticalCenter: parent.verticalCenter }
    }
  }
}
