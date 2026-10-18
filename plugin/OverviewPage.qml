import QtQuick
import "." as Mihomo
import qs.Commons

Item {
  id: root
  property var svc
  property var panel

  property var profile: null
  property int connCount: 0
  property var recent: []
  property bool updating: false
  readonly property bool typing: false

  function reload() {
    svc.request("profiles.list", null, function(r) {
      if (!r) return
      profile = null
      for (var i = 0; i < r.items.length; i++) if (r.items[i].id === r.current) profile = r.items[i]
    })
    svc.request("problems.list", { only_new: true, limit: 3 }, function(r) { if (r) recent = r.items })
    svc.request("connections.list", null, function(r) { connCount = r && r.connections ? r.connections.length : 0 })
  }

  function updateProfile() {
    if (!profile) return
    updating = true
    svc.request("profiles.update", { id: profile.id }, function(r, err) {
      updating = false
      if (err) panel.notify(err.message)
      else if (r && r[0]) panel.notify(r[0].error ? r[0].error : I18n.t("profileUpdated"))
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

  Component.onCompleted: reload()
  Timer { interval: 3000; running: true; repeat: true; onTriggered: root.svc.request("connections.list", null, function(r) { root.connCount = r && r.connections ? r.connections.length : 0 }) }

  Connections {
    target: root.svc
    function onSystemProxyFailed(message) { root.panel.notify(message) }
  }

  Column {
    anchors.fill: parent
    spacing: Style.space(14)

    Row {
      width: parent.width
      Column {
        width: parent.width - restartBtn.width
        spacing: Style.space(3)
        Txt { text: I18n.t("navOverview"); bold: true; font.pixelSize: Style.font.heading }
        Txt {
          text: root.svc.coreRunning ? I18n.t("coreLine").arg(root.svc.status.core.version || "").arg(root.svc.status.core.mixed_port) : I18n.t("coreStopped")
          dim: true; font.pixelSize: Style.font.bodySmall
        }
      }
      Btn { id: restartBtn; icon: Icons.refresh; text: I18n.t("applyConfig"); onClicked: root.svc.request("config.apply", null, function(r, err) { root.panel.notify(err ? err.message : I18n.t("configApplied")) }) }
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
          [I18n.t("statDown"), root.svc.formatRate(root.svc.traffic.down), "", ""],
          [I18n.t("statUp"), root.svc.formatRate(root.svc.traffic.up), "", ""],
          [I18n.t("statConns"), String(root.connCount), "", ""],
          [I18n.t("statNew"), String(root.svc.newProblems), I18n.t("statNewSub").arg(root.svc.problemCounts.visible), "problems"]
        ]
        Rectangle {
          required property var modelData
          width: (parent.width - Style.space(30)) / 4
          height: Style.space(70)
          radius: Style.cornerRadius
          color: Util.alpha(Color.popups.text, 0.04)
          border.width: 1
          border.color: Util.alpha(Color.popups.text, 0.12)
          Column {
            anchors.fill: parent
            anchors.margins: Style.space(12)
            spacing: Style.space(4)
            Txt { text: modelData[0]; faint: true; font.pixelSize: Style.font.caption }
            Txt { text: modelData[1]; bold: true; font.pixelSize: Style.font.heading; color: modelData[3] === "problems" && root.svc.newProblems > 0 ? root.svc.red : Color.popups.text }
            Txt { visible: modelData[2] !== ""; text: modelData[2]; dim: true; font.pixelSize: Style.font.caption }
          }
          MouseArea { anchors.fill: parent; enabled: modelData[3] !== ""; cursorShape: Qt.PointingHandCursor; onClicked: root.panel.showPage(modelData[3]) }
        }
      }
    }

    Row {
      spacing: Style.space(16)
      Txt { text: I18n.t("modeLabel"); dim: true; width: Style.space(120); anchors.verticalCenter: parent.verticalCenter }
      Seg {
        current: root.svc.mode
        model: [{ key: "rule", label: I18n.mode("rule") }, { key: "global", label: I18n.mode("global") }, { key: "direct", label: I18n.mode("direct") }]
        onPicked: function(k) { root.svc.request("mode.set", { mode: k }, null) }
      }
    }

    Row {
      width: parent.width
      Column {
        width: parent.width - tunSwitch.width
        spacing: Style.space(2)
        Txt { text: "TUN" }
        Txt { text: I18n.t("tunHint"); dim: true; font.pixelSize: Style.font.caption }
      }
      Mihomo.Switch { id: tunSwitch; anchors.verticalCenter: parent.verticalCenter; checked: root.svc.tun; onToggled: root.svc.request("tun.set", { enable: !root.svc.tun }, null) }
    }

    SystemProxyControl { width: parent.width; svc: root.svc }

    Rectangle {
      visible: root.profile !== null
      width: parent.width
      height: profileCol.implicitHeight + Style.space(24)
      radius: Style.cornerRadius
      color: Util.alpha(Color.popups.text, 0.04)
      border.width: 1
      border.color: Util.alpha(Color.popups.text, 0.12)
      Column {
        id: profileCol
        anchors.fill: parent
        anchors.margins: Style.space(12)
        spacing: Style.space(10)
        Row {
          width: parent.width
          spacing: Style.space(10)
          Txt { text: Icons.profiles; color: Color.accent; anchors.verticalCenter: parent.verticalCenter }
          Txt { text: root.profile ? root.profile.name : ""; bold: true; anchors.verticalCenter: parent.verticalCenter }
          Txt {
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - Style.space(260)
            text: root.profile ? I18n.t("updatedAgo").arg(root.svc.ago(root.profile.updated * 1000)) + (root.profile.last_error ? " · " + root.profile.last_error : "") : ""
            dim: true; font.pixelSize: Style.font.caption
          }
          Btn { visible: root.profile && root.profile.source.type === "remote"; icon: Icons.refresh; text: I18n.t("update"); busy: root.updating; onClicked: root.updateProfile() }
        }
        Rectangle {
          readonly property var u: root.profile ? root.profile.usage : null
          visible: u && u.total > 0
          width: parent.width; height: Style.space(6); radius: height / 2
          color: Util.alpha(Color.popups.text, 0.12)
          Rectangle {
            readonly property real frac: parent.u ? Math.min(1, (parent.u.upload + parent.u.download) / parent.u.total) : 0
            width: parent.width * frac; height: parent.height; radius: parent.radius
            color: frac > 0.9 ? root.svc.red : Color.accent
          }
        }
        Txt {
          readonly property var u: root.profile ? root.profile.usage : null
          visible: u && u.total > 0
          text: u ? I18n.t("usage").arg(root.svc.formatBytes(u.upload + u.download)).arg(root.svc.formatBytes(u.total)) + (u.expire ? " · " + I18n.t("until").arg(new Date(u.expire * 1000).toLocaleDateString()) : "") : ""
          dim: true; font.pixelSize: Style.font.caption
        }
      }
    }

    Column {
      visible: root.recent.length > 0
      width: parent.width
      spacing: Style.space(4)
      Row {
        width: parent.width
        Txt { text: I18n.t("recentProblems"); faint: true; bold: true; font.pixelSize: Style.font.caption; font.letterSpacing: 1; width: parent.width - allLink.width }
        Txt {
          id: allLink
          text: I18n.t("allLink"); color: Color.accent; font.pixelSize: Style.font.caption
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
