import QtQuick
import qs.Commons
import "." as Mihomo
import qs.Commons as Commons
import "TrafficModels.js" as TM

// Overview in the large window: tiles, the switches, the last hour as a
// chart, new problems and who used the network. Data comes from the page.
Item {
  id: root
  property var page
  readonly property var svc: page.svc
  readonly property var panel: page.panel
  readonly property var hour: page.hour

  // The first selector the user picks a node in, with that node's delay.
  readonly property var group: page.groups.length ? page.groups[0] : null
  readonly property var groupNow: {
    if (!group) return null
    for (var i = 0; i < group.members.length; i++) if (group.members[i].name === group.now) return group.members[i]
    return null
  }
  readonly property var delay: groupNow ? groupNow.delay : null
  readonly property real appMax: hour ? hour.rows.reduce(function(m, r) { return Math.max(m, r.up + r.down) }, 0) : 0

  function delayText(d) { return d === null || d === undefined ? "—" : (d === 0 ? MihomoI18n.t("timeout") : d + " " + MihomoI18n.t("ms")) }
  function delayColor(d) { return d === null || d === undefined ? Commons.Color.popups.text : (d === 0 ? svc.red : (d < 400 ? Commons.Color.popups.text : svc.yellow)) }
  function timeLabel(ms, long) { return new Date(ms).toLocaleString(Qt.locale(MihomoI18n.language === "en" ? "en_US" : "ru_RU"), "HH:mm") }

  component Card: Rectangle {
    radius: Style.cornerRadius
    color: Util.alpha(Commons.Color.popups.text, 0.04)
    border.width: 1
    border.color: Util.alpha(Commons.Color.popups.text, 0.12)
  }

  component SectionHead: Item {
    id: head
    property string title
    property string link
    property string target
    width: parent.width
    height: Style.space(16)
    Txt { text: head.title; faint: true; bold: true; font.pixelSize: Style.font.bodySmall; font.letterSpacing: 1; anchors.verticalCenter: parent.verticalCenter }
    Txt {
      anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
      text: head.link; color: Commons.Color.accent; font.pixelSize: Style.font.bodySmall
      MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.panel.showPage(head.target) }
    }
  }

  Column {
    id: top
    anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
    spacing: Style.space(14)

    Item {
      width: parent.width
      height: Math.max(titleCol.implicitHeight, modeSeg.height)
      Column {
        id: titleCol
        anchors.left: parent.left; anchors.right: modeSeg.left; anchors.rightMargin: Style.space(16)
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(3)
        Txt { text: MihomoI18n.t("navOverview"); bold: true; font.pixelSize: Style.font.heading }
        Txt {
          objectName: "overviewSubtitle"
          width: parent.width
          elide: Text.ElideRight
          text: {
            var parts = [root.svc.coreRunning ? MihomoI18n.t("coreRunning") : MihomoI18n.t("coreStopped")]
            var p = root.page.profile
            if (p) parts.push(MihomoI18n.t("profileLine").arg(p.name).arg(root.svc.ago(p.updated * 1000)) + (p.last_error ? " · " + p.last_error : ""))
            return parts.join(" · ")
          }
          dim: true; font.pixelSize: Style.font.body
        }
      }
      Seg {
        id: modeSeg
        objectName: "overviewMode"
        anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
        current: root.svc.mode
        model: [{ key: "rule", label: MihomoI18n.mode("rule") }, { key: "global", label: MihomoI18n.mode("global") }, { key: "direct", label: MihomoI18n.mode("direct") }]
        onPicked: function(k) { root.svc.request("mode.set", { mode: k }, null) }
      }
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
      id: tiles
      width: parent.width
      spacing: Style.space(10)
      Repeater {
        model: [
          { label: MihomoI18n.t("statDown"), value: root.svc.formatRate(root.svc.traffic.down), sub: root.hour ? MihomoI18n.t("statHour").arg(root.svc.formatBytes(root.hour.total.down)) : "", page: "traffic", spark: "down" },
          { label: MihomoI18n.t("statUp"), value: root.svc.formatRate(root.svc.traffic.up), sub: root.hour ? MihomoI18n.t("statHour").arg(root.svc.formatBytes(root.hour.total.up)) : "", page: "traffic", spark: "up" },
          { label: MihomoI18n.t("statConns"), value: String(root.page.connCount), sub: root.hour ? MihomoI18n.t("statHour").arg(root.hour.total.conns.toLocaleString(Qt.locale("ru_RU"), "f", 0)) : "", page: "connections" },
          { label: MihomoI18n.t("statNew"), value: String(root.svc.newProblems), sub: MihomoI18n.t("statNewSub").arg(root.svc.problemCounts.visible), page: "problems", color: root.svc.newProblems > 0 ? root.svc.red : Commons.Color.popups.text },
          { label: root.group ? MihomoI18n.t("statDelay").arg(root.group.name) : MihomoI18n.t("statDelayNone"), value: root.delayText(root.delay), sub: root.group ? root.group.now : "", page: "proxies", color: root.delayColor(root.delay) },
          { label: MihomoI18n.t("statRules"), value: root.page.rules ? String(root.page.rules.count) : "—", sub: root.page.rules && root.page.rules.hits > 0 ? MihomoI18n.t("hits").arg(root.page.rules.hits) : "", page: "rules" }
        ]
        Card {
          id: tile
          required property var modelData
          objectName: "overviewTile"
          width: (tiles.width - tiles.spacing * 5) / 6
          height: Style.space(74)
          clip: true
          // The last hour behind the number.
          Canvas {
            visible: tile.modelData.spark !== undefined && root.hour !== null
            anchors.fill: parent
            anchors.topMargin: parent.height * 0.35
            property var points: root.hour && tile.modelData.spark ? root.hour.all[tile.modelData.spark] : []
            onPointsChanged: requestPaint()
            onWidthChanged: requestPaint()
            onPaint: {
              var ctx = getContext("2d")
              ctx.reset()
              var p = points || []
              if (p.length < 2) return
              var m = TM.maxOf(p) || 1
              var c = tile.modelData.spark === "up" ? root.svc.peach : Commons.Color.accent
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
            Txt { width: parent.width; elide: Text.ElideRight; text: tile.modelData.label; faint: true; font.pixelSize: Style.font.bodySmall }
            Txt { text: tile.modelData.value; bold: true; font.pixelSize: Style.font.heading; color: tile.modelData.color || Commons.Color.popups.text }
            Txt { width: parent.width; elide: Text.ElideRight; visible: tile.modelData.sub !== ""; text: tile.modelData.sub; dim: true; font.pixelSize: Style.font.bodySmall }
          }
          MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.panel.showPage(tile.modelData.page) }
        }
      }
    }

    Row {
      width: parent.width
      spacing: Style.space(10)
      Card {
        width: (parent.width - parent.spacing) / 2
        height: Math.max(tunRow.implicitHeight, proxyControl.implicitHeight) + Style.space(28)
        Row {
          id: tunRow
          anchors.left: parent.left; anchors.right: parent.right; anchors.margins: Style.space(14)
          anchors.verticalCenter: parent.verticalCenter
          Column {
            width: parent.width - tunSwitch.width
            spacing: Style.space(2)
            Txt { text: "TUN" }
            Txt {
              text: MihomoI18n.t("tunHint") + (root.svc.status.core && root.svc.status.core.tun_stack ? " · " + root.svc.status.core.tun_stack : "")
              color: Commons.Color.muted; font.pixelSize: Style.font.bodySmall
            }
          }
          Mihomo.Switch { id: tunSwitch; anchors.verticalCenter: parent.verticalCenter; checked: root.svc.tun; onToggled: root.svc.request("tun.set", { enable: !root.svc.tun }, null) }
        }
      }
      Card {
        width: (parent.width - parent.spacing) / 2
        height: Math.max(tunRow.implicitHeight, proxyControl.implicitHeight) + Style.space(28)
        SystemProxyControl {
          id: proxyControl
          anchors.left: parent.left; anchors.right: parent.right; anchors.margins: Style.space(14)
          anchors.verticalCenter: parent.verticalCenter
          svc: root.svc
        }
      }
    }
  }

  Card {
    id: chartCard
    objectName: "overviewChart"
    anchors.left: parent.left; anchors.right: parent.right
    anchors.top: top.bottom; anchors.topMargin: Style.space(14)
    height: Math.max(Style.space(170), (root.height - top.height) * 0.42)

    Item {
      id: chartHead
      anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
      anchors.margins: Style.space(14)
      height: Style.space(16)
      Txt { text: MihomoI18n.t("chartHour"); dim: true; font.pixelSize: Style.font.body; anchors.verticalCenter: parent.verticalCenter }
      Row {
        anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(14)
        Repeater {
          model: [[Commons.Color.accent, MihomoI18n.t("legendDown")], [root.svc.peach, MihomoI18n.t("legendUp")]]
          Row {
            required property var modelData
            spacing: Style.space(5)
            Rectangle { width: Style.space(7); height: width; radius: 2; color: modelData[0]; anchors.verticalCenter: parent.verticalCenter }
            Txt { text: modelData[1]; dim: true; font.pixelSize: Style.font.bodySmall }
          }
        }
      }
    }
    TrafficChart {
      anchors.top: chartHead.bottom; anchors.topMargin: Style.space(10)
      anchors.left: parent.left; anchors.leftMargin: Style.space(8)
      anchors.right: parent.right; anchors.rightMargin: Style.space(14)
      anchors.bottom: parent.bottom; anchors.bottomMargin: Style.space(8)
      svc: root.svc
      visible: root.hour !== null
      series: root.hour ? [{ key: "all", label: MihomoI18n.t("chartAll"), color: Commons.Color.accent, upColor: root.svc.peach, up: root.hour.all.up, down: root.hour.all.down }] : []
      from: root.hour ? root.hour.from : 0
      step: root.hour ? root.hour.step : TM.MINUTE
      timeLabel: root.timeLabel
    }
  }

  Card {
    id: problemsCard
    objectName: "overviewProblems"
    anchors.left: parent.left; anchors.top: chartCard.bottom; anchors.topMargin: Style.space(14)
    anchors.bottom: parent.bottom
    width: (parent.width - Style.space(10)) * 0.64
    clip: true

    Column {
      anchors.fill: parent
      anchors.margins: Style.space(14)
      spacing: Style.space(6)
      SectionHead { title: MihomoI18n.t("lastProblems"); link: MihomoI18n.t("allLink"); target: "problems" }
      Repeater {
        model: root.page.recent
        ProblemRow {
          required property var modelData
          width: parent.width
          problem: modelData
          svc: root.svc
          onRouteClicked: root.panel.openEditor({ problem: modelData })
          onHideClicked: root.svc.request("problems.hide", { kind: "host", value: modelData.host }, function() { root.page.reload() })
        }
      }
    }
    Column {
      visible: root.page.recent.length === 0
      anchors.centerIn: parent
      spacing: Style.space(8)
      Txt { anchors.horizontalCenter: parent.horizontalCenter; text: Icons.check; color: root.svc.green; font.pixelSize: Style.font.heading }
      Txt { anchors.horizontalCenter: parent.horizontalCenter; text: MihomoI18n.t("noProblems"); dim: true }
    }
  }

  Card {
    objectName: "overviewApps"
    anchors.left: problemsCard.right; anchors.leftMargin: Style.space(10)
    anchors.right: parent.right
    anchors.top: problemsCard.top; anchors.bottom: parent.bottom
    clip: true

    Column {
      id: appsCol
      anchors.fill: parent
      anchors.margins: Style.space(14)
      spacing: Style.space(12)
      SectionHead { title: MihomoI18n.t("hourTop"); link: MihomoI18n.t("allLink"); target: "traffic" }
      Repeater {
        model: root.hour ? root.hour.rows : []
        Item {
          id: app
          required property var modelData
          width: appsCol.width
          height: appName.height + Style.space(10)
          Txt {
            id: appName
            anchors.left: parent.left; anchors.right: appBytes.left; anchors.rightMargin: Style.space(8)
            elide: Text.ElideRight
            text: TM.label("app", app.modelData, MihomoI18n.t); font.pixelSize: Style.font.body
          }
          Txt { id: appBytes; anchors.right: parent.right; text: root.svc.formatBytes(app.modelData.up + app.modelData.down); dim: true; font.pixelSize: Style.font.body }
          Rectangle {
            anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom
            height: Style.space(3); radius: height / 2
            color: Util.alpha(Commons.Color.popups.text, 0.08)
            Rectangle {
              width: root.appMax > 0 ? Math.max(height, parent.width * (app.modelData.up + app.modelData.down) / root.appMax) : 0
              height: parent.height; radius: parent.radius
              color: Commons.Color.accent
            }
          }
          MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.panel.showPage("traffic") }
        }
      }
    }
    Txt {
      visible: !root.hour || root.hour.rows.length === 0
      anchors.centerIn: parent
      text: MihomoI18n.t("noTrafficHour"); dim: true
    }
  }
}
