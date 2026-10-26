import QtQuick
import QtQuick.Controls
import qs.Commons

// Shared footer for every page. Only confirmed daemon data is shown;
// loss of the socket takes precedence over the last known core state.
Item {
  id: root
  property var service
  property var panel
  property bool actionBusy: false
  property double now: Date.now()
  readonly property var core: service && service.status ? service.status.core || {} : ({})
  readonly property var operation: service && service.status ? service.status.core_operation : null
  readonly property var network: service && service.status && service.status.network ? service.status.network : ({})
  readonly property var tunIssue: core.tun_issue || null
  readonly property var capsIssue: issue("core_no_caps")
  readonly property var otherIssue: issue("other_core")
  readonly property color dimColor: Util.alpha(Color.popups.text, 0.68)
  readonly property color faintColor: Util.alpha(Color.popups.text, 0.42)
  readonly property string stateKey: {
    if (!service || !service.connected) return "disconnected"
    if ((operation && operation.kind === "downloading") || issue("core_installing")) return "downloading"
    if (operation && (operation.kind === "starting" || operation.kind === "applying")) return "starting"
    if (!core.running) return core.restarting ? "restarting" : (core.starting ? "starting" : "stopped")
    if (network.offline) return "offline"
    if (network.settling) return "settling"
    if (!core.tun && ((tunIssue && tunIssue.no_capabilities) || capsIssue)) return "no_caps"
    if (otherIssue || (tunIssue && tunIssue.other_core)) return "other_core"
    if (tunIssue || issue("tun_failed")) return "tun_failed"
    return core.tun ? "running" : "tun_off"
  }
  readonly property string stack: core.tun_stack === "gvisor" ? "gVisor" : (core.tun_stack || "")
  readonly property string tunText: stack ? "TUN · " + stack : "TUN"
  readonly property string portText: core.mixed_port > 0 ? ":" + core.mixed_port : "—"
  readonly property real progress: operation && operation.total > 0 ? Math.max(0, Math.min(1, operation.downloaded / operation.total)) : -1

  function issue(id) {
    var list = service && service.issues ? service.issues : []
    for (var i = 0; i < list.length; i++) if (list[i].id === id) return list[i]
    return null
  }
  function row(icon, text, color, action, iconColor) {
    return { icon: icon, text: text, color: color || dimColor, action: action || "", iconColor: iconColor || color || dimColor }
  }

  readonly property var rows: {
    var red = service ? service.red : Color.urgent
    var yellow = service ? service.yellow : Color.accent
    var green = service ? service.green : Color.accent
    var traffic = service ? service.traffic : null
    var rates = traffic ? "↓ " + service.formatRate(traffic.down) + "  ↑ " + service.formatRate(traffic.up) : "—"
    var running = row(Icons.statusCheck, I18n.t("coreRunning"), dimColor, "", green)
    var speed = row(Icons.connections, rates)
    switch (stateKey) {
    case "disconnected":
      return [row(Icons.unplug, I18n.t("navDaemonDown"), red), row(Icons.statusUnknown, I18n.t("navStateUnknown"), faintColor),
        row(Icons.terminal, I18n.t("startDaemon"), Color.accent, "daemon")]
    case "downloading":
      var detail = operation ? operation.detail : ""
      if (operation && operation.downloaded > 0) {
        detail += (detail ? " · " : "") + (operation.total > 0
          ? I18n.t("navDownloadBytes").arg(Math.floor(operation.downloaded / 1048576)).arg(Math.ceil(operation.total / 1048576))
          : service.formatBytes(operation.downloaded))
      }
      return [row(Icons.down, I18n.t("issue_core_installing"), dimColor, "", Color.accent), row("", ""), row(Icons.disk, detail || I18n.t("navConnecting"), dimColor, "", faintColor)]
    case "starting":
      return [row(Icons.refresh, I18n.t("navCoreStarting"), yellow),
        row(Icons.file, operation && operation.kind === "applying" ? I18n.t("navCheckingConfig").arg(operation.detail || "config.yaml") : I18n.t("navWaitingCore")),
        row(Icons.connections, "—", faintColor)]
    case "restarting":
      var elapsed = core.crashed_at ? Math.max(0, Math.floor((now - core.crashed_at) / 1000)) : 0
      return [row(Icons.refresh, core.restart_count > 0 ? I18n.t("navRestartAttempt").arg(core.restart_count) : I18n.t("navRestarting"), service ? service.peach : yellow),
        row(Icons.shield, tunText), row(Icons.history, core.crashed_at ? I18n.t("navCrashedAgo").arg(elapsed) : "—", faintColor)]
    case "stopped":
      var failure = issue("core_install_failed") || issue("core_missing") || issue("no_profile")
      return [row(Icons.statusOff, I18n.t("coreStopped"), red),
        row(Icons.shieldOff, failure ? I18n.t("issue_" + failure.id).arg(failure.detail || "") : I18n.t(service && service.systemProxyWanted ? "navProxyUnavailable" : "navTrafficDirect"), faintColor),
        row(Icons.play, failure ? I18n.t("navOverview") : I18n.t("fix_start_core"), Color.accent, failure ? "overview" : "start")]
    case "offline":
      return [row(Icons.unplug, I18n.t("navOffline"), red), row(Icons.history, I18n.t("navCountingPaused"), faintColor), speed]
    case "settling":
      return [row(Icons.refresh, I18n.t("navNetworkChanged"), yellow), row(Icons.history, I18n.t("navCountingPaused"), faintColor), speed]
    case "no_caps":
      return [running, row(Icons.shieldAlert, I18n.t("navTunNoCaps"), yellow), row(Icons.terminal, I18n.t("navSetcapFix"), Color.accent, "setcap")]
    case "tun_failed":
      return [running, row(Icons.shieldAlert, I18n.t("tunFailedShort"), red), row(Icons.connections, I18n.t("navProxyOnly").arg(portText))]
    case "other_core":
      return [running, row(Icons.problems, I18n.t("navOtherCore").arg(otherIssue ? otherIssue.detail : tunIssue.other_core), yellow), speed]
    case "tun_off":
      return [running, row(Icons.shieldOff, I18n.t("navTunOffProxy").arg(portText), dimColor, "", faintColor), speed]
    default:
      return [running, row(Icons.shield, tunText), speed]
    }
  }

  function activate(action) {
    if (actionBusy || !service) return
    if (action === "daemon") {
      actionBusy = true
      service.startDaemon()
      daemonRetry.restart()
    } else if (action === "start") {
      actionBusy = true
      service.request("core.start", null, function(result, error) {
        root.actionBusy = false
        if (error && root.panel) root.panel.notify(error.message)
      }, true)
    } else if (action === "setcap") {
      service.runFix(capsIssue && capsIssue.fix ? capsIssue.fix : { id: "setcap", privileged: true, confirm: false })
    } else if (action === "overview" && panel) panel.showPage("overview")
  }

  // Reserve both three-digit rates, including decimals and units, using
  // the current theme font and locale rather than a fixed rail width.
  implicitWidth: Math.ceil(trafficMetrics.advanceWidth) + Style.space(37)
  implicitHeight: body.implicitHeight + Style.space(10)
  TextMetrics {
    id: trafficMetrics
    font.family: Style.font.family
    font.pixelSize: Style.font.caption
    text: "↓ 999.9 " + I18n.t("unitKB") + I18n.t("perSec") + "  ↑ 999.9 " + I18n.t("unitKB") + I18n.t("perSec")
  }
  Timer { interval: 1000; running: root.visible && root.stateKey === "restarting"; repeat: true; onTriggered: root.now = Date.now() }
  Timer { id: daemonRetry; interval: 3000; onTriggered: root.actionBusy = false }
  onStateKeyChanged: { now = Date.now(); if (stateKey !== "disconnected") daemonRetry.stop() }
  Connections {
    target: root.service
    function onConnectedChanged() { root.actionBusy = false }
  }

  Rectangle { width: parent.width; height: 1; color: Util.alpha(Color.popups.text, 0.15) }
  Column {
    id: body
    anchors.top: parent.top
    anchors.topMargin: Style.space(10)
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.leftMargin: Style.space(10)
    anchors.rightMargin: Style.space(10)
    spacing: Style.space(4)

    Repeater {
      model: root.rows
      delegate: Item {
        id: line
        required property var modelData
        required property int index
        objectName: "navStatusRow" + index
        width: body.width
        height: Math.max(Style.space(13), label.implicitHeight)
        readonly property bool isProgress: root.stateKey === "downloading" && index === 1
        readonly property bool isTraffic: index === 2 && ["running", "tun_off", "other_core", "offline", "settling"].indexOf(root.stateKey) !== -1
        activeFocusOnTab: modelData.action !== ""
        Accessible.role: modelData.action ? Accessible.Button : Accessible.StaticText
        Accessible.name: modelData.text
        Hint {
          visible: mouse.containsMouse && !mouse.pressed && modelData.action !== ""
          text: modelData.action === "setcap" ? I18n.t("fix_setcap") + " · " + I18n.t("privilegedHint") : modelData.text
        }
        Accessible.onPressAction: root.activate(modelData.action)
        Keys.onReturnPressed: root.activate(modelData.action)
        Keys.onSpacePressed: root.activate(modelData.action)

        Rectangle {
          anchors.fill: parent
          anchors.margins: -Style.space(2)
          radius: Style.space(2)
          color: Style.hoverFillFor(Color.popups.text, Color.accent)
          visible: line.activeFocus || mouse.containsMouse
        }
        Txt {
          id: icon
          visible: !line.isProgress
          width: Style.space(11)
          anchors.verticalCenter: parent.verticalCenter
          text: line.modelData.icon
          font.pixelSize: Style.space(11)
          color: line.modelData.iconColor
          RotationAnimation on rotation {
            running: root.visible && line.index === 0 && ["starting", "restarting", "settling"].indexOf(root.stateKey) !== -1
            from: 0; to: 360; duration: 1200; loops: Animation.Infinite
          }
        }
        Txt {
          id: label
          visible: !line.isProgress
          anchors.left: icon.right
          anchors.leftMargin: Style.space(6)
          anchors.right: parent.right
          anchors.verticalCenter: parent.verticalCenter
          text: line.modelData.text
          textFormat: Text.PlainText
          wrapMode: line.isTraffic ? Text.NoWrap : Text.Wrap
          elide: Text.ElideNone
          color: line.modelData.color
          font.pixelSize: Style.font.caption
          opacity: root.actionBusy && line.modelData.action ? 0.5 : 1
        }
        Rectangle {
          id: track
          objectName: "navDownloadProgress"
          visible: line.isProgress
          anchors.left: parent.left
          anchors.leftMargin: Style.space(17)
          anchors.right: parent.right
          anchors.verticalCenter: parent.verticalCenter
          height: Style.space(4)
          radius: height / 2
          color: Util.alpha(Color.popups.text, 0.2)
          clip: true
          Rectangle {
            id: progressFill
            width: track.width * (root.progress < 0 ? 0.3 : root.progress)
            height: track.height; radius: track.radius
            color: Color.accent
            SequentialAnimation on x {
              running: root.visible && track.visible && root.progress < 0
              loops: Animation.Infinite
              onStopped: progressFill.x = 0
              NumberAnimation { from: -track.width * 0.3; to: track.width; duration: 1100 }
            }
            x: 0
          }
        }
        MouseArea {
          id: mouse
          anchors.fill: parent
          enabled: line.modelData.action !== "" && !root.actionBusy
          hoverEnabled: true
          cursorShape: Qt.PointingHandCursor
          onClicked: root.activate(line.modelData.action)
        }
      }
    }
  }
}
