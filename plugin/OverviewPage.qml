import QtQuick
import qs.Commons
import "." as Mihomo
import qs.Commons as Commons
import "TrafficModels.js" as TM

Item {
  id: root
  property var svc
  property var panel

  property var profile: null
  property int connCount: 0
  // New problems; in the large window the latest ones, new marked by a dot.
  property var recent: []
  // The last hour from the traffic history: totals per minute, top apps.
  property var hour: null
  property bool updating: false
  readonly property bool typing: false
  // The large window shows a dashboard with more on it.
  readonly property bool wide: panel && panel.large === true
  property var groups: []
  // Own rules: { count, hits }.
  property var rules: null

  function reload() {
    svc.request("profiles.list", null, function(r) {
      if (!r) return
      profile = null
      for (var i = 0; i < r.items.length; i++) if (r.items[i].id === r.current) profile = r.items[i]
    })
    svc.request("problems.list", { only_new: !wide, limit: wide ? 8 : 3 }, function(r) { if (r) recent = r.items })
    svc.request("connections.list", null, function(r) { connCount = r && r.connections ? r.connections.length : 0 })
    if (!wide) return
    svc.request("proxies.list", null, function(r) {
      if (r) groups = r.filter(function(g) { return g.kind === "Selector" && g.name !== "GLOBAL" })
    })
    svc.request("rules.list", null, function(r) {
      if (!r) return
      var hits = 0
      r.items.forEach(function(x) { hits += x.hits || 0 })
      rules = { count: r.items.length, hits: hits }
    })
  }

  function updateProfile() {
    if (!profile) return
    updating = true
    svc.request("profiles.update", { id: profile.id }, function(r, err) {
      updating = false
      if (err) panel.notify(err.message)
      else if (r && r[0]) panel.notify(r[0].error ? r[0].error : MihomoI18n.t("profileUpdated"))
      reload()
    })
  }

  function textKey(t) {
    if (t === "r") svc.request("mode.set", { mode: "rule" }, null)
    else if (t === "g") svc.request("mode.set", { mode: "global" }, null)
    else if (t === "d") svc.request("mode.set", { mode: "direct" }, null)
    else if (t === "t") svc.request("tun.set", { enable: !svc.tun }, null)
    else if (t === "u") updateProfile()
  }

  function reloadHour() {
    svc.request("traffic.query", { from: Date.now() - TM.HOUR, by: "app", limit: 0, rows: wide ? 8 : 3 }, function(r) { if (r) hour = r })
  }

  Component.onCompleted: { reload(); reloadHour() }
  onWideChanged: { reload(); reloadHour() }
  Timer { interval: 3000; running: true; repeat: true; onTriggered: root.svc.request("connections.list", null, function(r) { root.connCount = r && r.connections ? r.connections.length : 0 }) }
  Timer { interval: 15000; running: true; repeat: true; onTriggered: root.reloadHour() }

  Connections {
    target: root.svc
    function onSystemProxyFailed(message) { root.panel.notify(message) }
  }

  Loader {
    anchors.fill: parent
    active: root.wide
    sourceComponent: OverviewDashboard { page: root }
  }

  Column {
    visible: !root.wide
    anchors.fill: parent
    spacing: Style.space(14)

    Row {
      width: parent.width
      Column {
        width: parent.width - restartBtn.width
        spacing: Style.space(3)
        Txt { text: MihomoI18n.t("navOverview"); bold: true; font.pixelSize: Style.font.heading }
        Txt {
          text: root.svc.coreRunning ? MihomoI18n.t("coreLine").arg(root.svc.status.core.version || "").arg(root.svc.status.core.mixed_port) : MihomoI18n.t("coreStopped")
          dim: true; font.pixelSize: Style.font.body
        }
      }
      Btn { id: restartBtn; icon: Icons.refresh; text: MihomoI18n.t("applyConfig"); onClicked: root.svc.request("config.apply", null, function(r, err) { root.panel.notify(err ? err.message : MihomoI18n.t("configApplied")) }) }
    }

    IssueList { width: parent.width; svc: root.svc }
    Banner {
      visible: root.svc.configError !== ""
      width: parent.width
      tone: "error"
      toneColor: root.svc.red
      text: root.svc.configError
    }

    Row {
      width: parent.width
      spacing: Style.space(10)
      Repeater {
        model: [
          [MihomoI18n.t("statDown"), root.svc.formatRate(root.svc.traffic.down), root.hour ? MihomoI18n.t("statHour").arg(root.svc.formatBytes(root.hour.total.down)) : "", "traffic", "down"],
          [MihomoI18n.t("statUp"), root.svc.formatRate(root.svc.traffic.up), root.hour ? MihomoI18n.t("statHour").arg(root.svc.formatBytes(root.hour.total.up)) : "", "traffic", "up"],
          [MihomoI18n.t("statConns"), String(root.connCount), "", ""],
          [MihomoI18n.t("statNew"), String(root.svc.newProblems), MihomoI18n.t("statNewSub").arg(root.svc.problemCounts.visible), "problems"]
        ]
        Rectangle {
          required property var modelData
          width: (parent.width - Style.space(30)) / 4
          height: Style.space(70)
          radius: Style.cornerRadius
          color: Util.alpha(Commons.Color.popups.text, 0.04)
          border.width: 1
          border.color: Util.alpha(Commons.Color.popups.text, 0.12)
          clip: true
          // The last hour behind the number.
          Canvas {
            objectName: "overviewSpark"
            visible: modelData[4] !== undefined && root.hour !== null
            anchors.fill: parent
            anchors.topMargin: parent.height * 0.35
            property var points: root.hour && modelData[4] ? root.hour.all[modelData[4]] : []
            onPointsChanged: requestPaint()
            onPaint: {
              var ctx = getContext("2d")
              ctx.reset()
              var p = points || []
              if (p.length < 2) return
              var m = TM.maxOf(p) || 1
              var c = modelData[4] === "up" ? root.svc.peach : Commons.Color.accent
              ctx.beginPath()
              ctx.moveTo(0, height)
              for (var i = 0; i < p.length; i++) ctx.lineTo(i * width / (p.length - 1), height - 2 - (height - 6) * p[i] / m)
              ctx.lineTo(width, height)
              ctx.closePath()
              ctx.fillStyle = Qt.rgba(c.r, c.g, c.b, 0.14)
              ctx.fill()
            }
          }
          Column {
            anchors.fill: parent
            anchors.margins: Style.space(12)
            spacing: Style.space(4)
            Txt { text: modelData[0]; faint: true; font.pixelSize: Style.font.bodySmall }
            Txt { text: modelData[1]; bold: true; font.pixelSize: Style.font.heading; color: modelData[3] === "problems" && root.svc.newProblems > 0 ? root.svc.red : Commons.Color.popups.text }
            Txt { visible: modelData[2] !== ""; text: modelData[2]; dim: true; font.pixelSize: Style.font.bodySmall }
          }
          MouseArea { anchors.fill: parent; enabled: modelData[3] !== ""; cursorShape: Qt.PointingHandCursor; onClicked: root.panel.showPage(modelData[3]) }
        }
      }
    }

    // Who used the network in the last hour.
    Row {
      objectName: "overviewTopApps"
      visible: root.hour !== null && root.hour.rows.length > 0
      width: parent.width
      spacing: Style.space(10)
      Txt { text: MihomoI18n.t("hourTop"); faint: true; bold: true; font.pixelSize: Style.font.bodySmall; font.letterSpacing: 1; anchors.verticalCenter: parent.verticalCenter }
      Repeater {
        model: root.hour ? root.hour.rows : []
        Row {
          required property var modelData
          required property int index
          spacing: Style.space(5)
          anchors.verticalCenter: parent.verticalCenter
          Rectangle { width: Style.space(7); height: width; radius: 2; color: [Commons.Color.accent, root.svc.peach, root.svc.mauve][index % 3]; anchors.verticalCenter: parent.verticalCenter }
          Txt { text: TM.label("app", modelData, MihomoI18n.t); font.pixelSize: Style.font.body }
          Txt { text: root.svc.formatBytes(modelData.up + modelData.down); dim: true; font.pixelSize: Style.font.body }
        }
      }
      Item { width: 1; height: 1 }
      Txt {
        text: MihomoI18n.t("hourAll"); color: Commons.Color.accent; font.pixelSize: Style.font.bodySmall
        anchors.verticalCenter: parent.verticalCenter
        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.panel.showPage("traffic") }
      }
    }

    Row {
      spacing: Style.space(16)
      Txt { text: MihomoI18n.t("modeLabel"); dim: true; width: Style.space(120); anchors.verticalCenter: parent.verticalCenter }
      Seg {
        current: root.svc.mode
        model: [{ key: "rule", label: MihomoI18n.mode("rule") }, { key: "global", label: MihomoI18n.mode("global") }, { key: "direct", label: MihomoI18n.mode("direct") }]
        onPicked: function(k) { root.svc.request("mode.set", { mode: k }, null) }
      }
    }

    Row {
      width: parent.width
      Column {
        width: parent.width - tunSwitch.width
        spacing: Style.space(2)
        Txt { text: "TUN" }
        Txt { text: MihomoI18n.t("tunHint"); dim: true; font.pixelSize: Style.font.bodySmall }
      }
      Mihomo.Switch { id: tunSwitch; anchors.verticalCenter: parent.verticalCenter; checked: root.svc.tun; onToggled: root.svc.request("tun.set", { enable: !root.svc.tun }, null) }
    }

    SystemProxyControl { width: parent.width; svc: root.svc }

    Rectangle {
      visible: root.profile !== null
      width: parent.width
      height: profileCol.implicitHeight + Style.space(24)
      radius: Style.cornerRadius
      color: Util.alpha(Commons.Color.popups.text, 0.04)
      border.width: 1
      border.color: Util.alpha(Commons.Color.popups.text, 0.12)
      Column {
        id: profileCol
        anchors.fill: parent
        anchors.margins: Style.space(12)
        spacing: Style.space(10)
        Row {
          width: parent.width
          spacing: Style.space(10)
          Txt { text: Icons.profiles; color: Commons.Color.accent; anchors.verticalCenter: parent.verticalCenter }
          Txt { text: root.profile ? root.profile.name : ""; bold: true; anchors.verticalCenter: parent.verticalCenter }
          Txt {
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - Style.space(260)
            text: root.profile ? MihomoI18n.t("updatedAgo").arg(root.svc.ago(root.profile.updated * 1000)) + (root.profile.last_error ? " · " + root.profile.last_error : "") : ""
            dim: true; font.pixelSize: Style.font.bodySmall
          }
          Btn { visible: root.profile && root.profile.source.type === "remote"; icon: Icons.refresh; text: MihomoI18n.t("update"); busy: root.updating; onClicked: root.updateProfile() }
        }
        Rectangle {
          readonly property var u: root.profile ? root.profile.usage : null
          visible: !!u && u.total > 0
          width: parent.width; height: Style.space(6); radius: height / 2
          color: Util.alpha(Commons.Color.popups.text, 0.12)
          Rectangle {
            readonly property real frac: parent.u ? Math.min(1, (parent.u.upload + parent.u.download) / parent.u.total) : 0
            width: parent.width * frac; height: parent.height; radius: parent.radius
            color: frac > 0.9 ? root.svc.red : Commons.Color.accent
          }
        }
        Txt {
          readonly property var u: root.profile ? root.profile.usage : null
          visible: !!u && u.total > 0
          text: u ? MihomoI18n.t("usage").arg(root.svc.formatBytes(u.upload + u.download)).arg(root.svc.formatBytes(u.total)) + (u.expire ? " · " + MihomoI18n.t("until").arg(new Date(u.expire * 1000).toLocaleDateString()) : "") : ""
          dim: true; font.pixelSize: Style.font.bodySmall
        }
      }
    }

    Column {
      visible: root.recent.length > 0
      width: parent.width
      spacing: Style.space(4)
      Row {
        width: parent.width
        Txt { text: MihomoI18n.t("recentProblems"); faint: true; bold: true; font.pixelSize: Style.font.bodySmall; font.letterSpacing: 1; width: parent.width - allLink.width }
        Txt {
          id: allLink
          text: MihomoI18n.t("allLink"); color: Commons.Color.accent; font.pixelSize: Style.font.bodySmall
          MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.panel.showPage("problems") }
        }
      }
      Repeater {
        model: root.recent
        ProblemRow {
          required property var modelData
          width: parent.width
          problem: modelData
          svc: root.svc
          onRouteClicked: root.panel.openEditor({ problem: modelData })
          onHideClicked: root.svc.request("problems.hide", { kind: "host", value: modelData.host }, function() { root.reload() })
        }
      }
    }
  }
}
