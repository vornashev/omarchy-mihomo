import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

// Bar widget + popup panel for omarchy-mihomo.
Panel {
  id: root
  moduleName: "tema.omarchy-mihomo"
  ipcTarget: "tema.omarchy-mihomo"
  manageIpc: false

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  property string page: "problems"
  property string problemsView: "table"
  // Problem (or rule) being routed in the editor.
  property var editing: null
  property string toast: ""
  // An unsaved form survives popup dismissal, in memory only.
  property var proxyDraft: null
  // Text the logs page filters by when it opens next.
  property string logFilter: ""
  // The event shown on the compact notice page.
  property string noticeKey: ""
  // Settings tab to show next: main | auto.
  property string settingsTab: "main"
  // One event from a notification: a small card without the page rail.
  readonly property bool compact: page === "notice"

  readonly property var pages: [
    { key: "overview", icon: Icons.overview, label: I18n.t("navOverview") },
    { key: "problems", icon: Icons.problems, label: I18n.t("navProblems") },
    { key: "rules", icon: Icons.rules, label: I18n.t("navRules") },
    { key: "proxies", icon: Icons.proxies, label: I18n.t("navProxies") },
    { key: "profiles", icon: Icons.profiles, label: I18n.t("navProfiles") },
    { key: "connections", icon: Icons.connections, label: I18n.t("navConnections") },
    { key: "traffic", icon: Icons.traffic, label: I18n.t("navTraffic") },
    { key: "logs", icon: Icons.logs, label: I18n.t("navLogs") },
    { key: "settings", icon: Icons.settings, label: I18n.t("navSettings") }
  ]

  readonly property color barIconColor: !svc.connected ? Qt.darker(barForeground, 1.6)
    : (svc.status && svc.status.core && svc.status.core.error ? Color.urgent
    : (svc.coreRunning ? (svc.tun ? svc.green : barForeground) : Qt.darker(barForeground, 1.6)))

  Binding { target: I18n; property: "language"; value: String(root.setting("locale", "ru")) }

  function showPage(key) {
    if (key !== page && pageLoader.item && pageLoader.item.canLeave && !pageLoader.item.canLeave()) return
    if (key !== "editor" && key !== "rule") editing = null
    if (key !== "notice") svc.noticeOnScreen = ""
    page = key
    if (!opened) open()
  }

  function openEditor(target) {
    editing = target
    page = "editor"
  }

  // Rule form: { from, origin?, rule?, kind?, value?, full?, duplicate? }.
  function openRule(spec) {
    editing = spec
    page = "rule"
    if (!opened) open()
  }

  // Opened from a desktop notification (`openNotice` over IPC).
  function openNotice(key) {
    var show = function() {
      noticeKey = key
      svc.noticeOnScreen = key
      editing = null
      page = "notice"
      if (!opened) open()
    }
    if (svc.notices[key]) { show(); return }
    // The shell restarted since: ask the daemon what the event was.
    svc.request("notices.get", { key: key }, function(n) {
      if (n) {
        var next = Object.assign({}, svc.notices)
        next[key] = n
        svc.notices = next
        show()
      } else showPage("problems")
    }, true)
  }

  // SUPER+ALT+P: a rule for the program behind the focused window.
  function ruleForWindow() { activeWindow.running = true }

  function openWindowRule(win, nodes) {
    if (!nodes || !nodes.length) { svc.raise("window", "window_unknown", "normal", {}); return }
    // A terminal hardly talks to the network itself; what runs in it does.
    var pick = nodes[0]
    for (var i = 1; i < nodes.length; i++) if (nodes[i].connections > pick.connections) pick = nodes[i]
    svc.request("connections.list", null, function(r) {
      var routes = {}
      var conns = r && r.connections ? r.connections : []
      for (var j = 0; j < conns.length; j++) {
        var m = conns[j].metadata || {}
        if (m.processPath !== pick.path && m.process !== pick.name) continue
        var chain = (conns[j].chains || []).slice().reverse().join(" → ")
        routes[chain] = (routes[chain] || 0) + 1
      }
      var best = "", n = 0
      for (var c in routes) if (routes[c] > n) { best = c; n = routes[c] }
      openRule({
        from: "window",
        kind: "process-name",
        window: win,
        candidates: nodes.filter(function(x) { return x.connections > 0 || x.pid === nodes[0].pid }),
        origin: { host: "", ip: "", process: pick.name, process_path: pick.path, pid: pick.pid, route: best, connections: pick.connections }
      })
    }, true)
  }

  Process {
    id: activeWindow
    command: ["hyprctl", "activewindow", "-j"]
    stdout: StdioCollector {
      onStreamFinished: {
        var w = null
        try { w = JSON.parse(text) } catch (e) {}
        if (!w || !w.pid || w.pid <= 0) { svc.raise("window", "window_unknown", "normal", {}); return }
        var win = { pid: w.pid, cls: w["class"] || "", title: w.title || "" }
        svc.request("processes.tree", { pid: w.pid }, function(nodes) { root.openWindowRule(win, nodes) }, true)
      }
    }
  }

  // Filters the traffic page applies when it opens next.
  property var trafficFilter: null
  function openTraffic(app) {
    trafficFilter = app ? [{ dim: "app", value: app, exclude: false }] : null
    showPage("traffic")
  }

  function showLogs(filter) {
    logFilter = filter
    showPage("logs")
  }

  // Give the keyboard back to panel navigation (after a text field).
  function forceKeys() { keyCatcher.forceActiveFocus() }

  function notify(text) {
    toast = text
    toastTimer.restart()
  }

  onOpenedChanged: {
    if (!opened) svc.noticeOnScreen = ""
    if (opened) Qt.callLater(function() { keyCatcher.forceActiveFocus() })
    // Opening the panel acknowledges new problems shown on the badge.
    else if (page === "problems" && svc.newProblems > 0) svc.request("problems.mark_seen", null, null)
  }

  Service {
    id: svc
    settings: root.settings
    panelVisible: root.opened
  }

  IpcHandler {
    target: root.ipcTarget
    function open(): void { root.open() }
    function close(): void { root.close() }
    function toggle(): void { root.toggle() }
    function openPage(name: string): void { root.showPage(name) }
    // Clicking one of our desktop notifications lands here.
    function openNotice(key: string): void { root.openNotice(key) }
    function ruleForWindow(): void { root.ruleForWindow() }
    // Open the route editor for a problem host, e.g. from a script.
    function route(host: string): void {
      svc.request("problems.get", { host: host }, function(p, err) {
        if (err || !p) { root.showPage("problems"); root.notify(err ? err.message : host); return }
        if (!root.opened) root.open()
        root.openEditor({ problem: p })
      })
    }
    function status(): string {
      return JSON.stringify({ connected: svc.connected, core: svc.coreRunning, mode: svc.mode, tun: svc.tun, newProblems: svc.newProblems })
    }
  }

  BarIconButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    tooltipText: !svc.connected ? I18n.t("daemonDown")
      : (svc.coreRunning ? I18n.t("barTooltip").arg(svc.profileName || "—").arg(svc.mode).arg(svc.newProblems) : I18n.t("coreDown"))
    iconComponent: Component {
      Item {
        Text {
          anchors.centerIn: parent
          text: !svc.connected || !svc.coreRunning ? Icons.shieldOff : (svc.status.core.error ? Icons.shieldAlert : Icons.shield)
          color: root.barIconColor
          font.family: Style.font.family
          font.pixelSize: Style.bar.iconFont
        }
        // Autofix is doing something (a rule on trial, a suggestion).
        Text {
          visible: svc.autofixActive > 0
          anchors.left: parent.left
          anchors.bottom: parent.bottom
          anchors.leftMargin: -Style.space(5)
          anchors.bottomMargin: -Style.space(3)
          text: Icons.sparkles
          color: Color.accent
          font.family: Style.font.family
          font.pixelSize: Math.max(8, Style.font.caption - 1)
        }
        Rectangle {
          visible: svc.newProblems > 0 && String(root.setting("badge", "on")) !== "off"
          anchors.right: parent.right
          anchors.top: parent.top
          anchors.rightMargin: -Style.space(5)
          anchors.topMargin: -Style.space(3)
          width: Math.max(height, badgeLabel.implicitWidth + Style.space(5))
          height: badgeLabel.implicitHeight + Style.space(1)
          radius: height / 2
          color: root.bar ? root.bar.urgent : Color.urgent
          Text {
            id: badgeLabel
            anchors.centerIn: parent
            text: svc.newProblems > 99 ? "99+" : String(svc.newProblems)
            color: Color.background
            font.family: Style.font.family
            font.pixelSize: Math.max(8, Style.font.caption - 1)
            font.bold: true
          }
        }
      }
    }
    onPressed: function(buttonCode) {
      if (buttonCode === Qt.MiddleButton) svc.request("tun.set", { enable: !svc.tun }, null)
      else if (buttonCode === Qt.RightButton) root.showPage("problems")
      else root.toggle()
    }
  }

  Timer { id: toastTimer; interval: 3500; onTriggered: root.toast = "" }

  KeyboardPanel {
    id: panel
    anchorItem: button
    owner: root
    bar: root.bar
    open: root.opened
    focusTarget: keyCatcher
    padding: 0
    contentWidth: root.compact ? panel.fittedContentWidth(Style.space(380))
      : panel.fittedContentWidth(Style.space(root.page === "proxies" ? 820 : (root.page === "traffic" ? 1180 : 860)) + rail.width - Style.space(190))
    contentHeight: root.compact ? panel.fittedContentHeight((pageLoader.item ? pageLoader.item.implicitHeight : Style.space(200)) + Style.space(32))
      : panel.fittedContentHeight(Style.space(root.page === "proxies" ? 680 : (root.page === "traffic" ? 720 : 660)))

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      // Let text fields have the keyboard.
      blocked: pageLoader.item && pageLoader.item.typing === true
      onMoveRequested: function(dx, dy) { if (pageLoader.item && pageLoader.item.move) pageLoader.item.move(dx, dy) }
      onActivateRequested: if (pageLoader.item && pageLoader.item.activate) pageLoader.item.activate()
      onDeleteRequested: if (pageLoader.item && pageLoader.item.remove) pageLoader.item.remove()
      onCloseRequested: {
        if (pageLoader.item && pageLoader.item.canLeave && !pageLoader.item.canLeave()) return
        if (root.page === "editor") root.showPage("problems")
        else if (root.page === "rule" && root.editing && root.editing.from === "window") root.close()
        else if (root.page === "rule") root.showPage(root.editing && root.editing.from ? root.editing.from : "rules")
        else root.close()
      }
      onTabRequested: function(direction) { root.switchPanel(direction) }
      onTextKey: function(t) {
        var n = parseInt(t)
        if (n >= 1 && n <= root.pages.length) { root.showPage(root.pages[n - 1].key); return }
        if (pageLoader.item && pageLoader.item.textKey) pageLoader.item.textKey(t)
      }

      Rectangle {
        id: rail
        visible: !root.compact
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: root.compact ? 0 : Math.max(Style.space(190), navStatus.implicitWidth + Style.space(20))
        color: Util.alpha(Color.popups.background, 0.6)
        radius: Style.cornerRadius

        Column {
          anchors.fill: parent
          anchors.margins: Style.space(10)
          spacing: Style.space(2)

          Row {
            leftPadding: Style.space(10)
            bottomPadding: Style.space(12)
            topPadding: Style.space(4)
            spacing: Style.space(8)
            Txt { text: Icons.shield; color: svc.coreRunning ? Color.accent : Color.muted; font.pixelSize: Style.font.heading; anchors.verticalCenter: parent.verticalCenter }
            Column {
              Txt { text: "mihomo"; bold: true }
              Txt {
                text: svc.coreRunning ? (svc.status.core.version || "") + " · " + I18n.mode(svc.mode) : (svc.connected ? I18n.t("coreStopped") : I18n.t("noDaemon"))
                faint: true; font.pixelSize: Style.font.caption
              }
            }
          }

          Repeater {
            model: root.pages
            NavItem {
              required property var modelData
              required property int index
              width: parent.width
              icon: modelData.icon
              label: modelData.label
              keyHint: String(index + 1)
              badge: modelData.key === "problems" ? svc.newProblems : (modelData.key === "rules" ? svc.orphans.length : 0)
              selected: root.page === modelData.key || (root.page === "editor" && modelData.key === "problems")
                || (root.page === "rule" && modelData.key === (root.editing && root.editing.from && root.editing.from !== "window" ? root.editing.from : "rules"))
              onClicked: root.showPage(modelData.key)
            }
          }
        }

        NavStatus {
          id: navStatus
          anchors.left: parent.left
          anchors.right: parent.right
          anchors.bottom: parent.bottom
          anchors.margins: Style.space(10)
          service: svc
          panel: root
        }
      }

      Item {
        id: content
        anchors.left: rail.right
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.margins: Style.space(root.compact ? 16 : 18)

        // Daemon not reachable: one clear action instead of empty pages.
        Column {
          visible: !svc.connected
          anchors.centerIn: parent
          spacing: Style.space(12)
          Txt { anchors.horizontalCenter: parent.horizontalCenter; text: Icons.shieldOff; faint: true; font.pixelSize: Style.font.displayLarge }
          Txt { anchors.horizontalCenter: parent.horizontalCenter; text: I18n.t("daemonDown"); bold: true }
          Txt { anchors.horizontalCenter: parent.horizontalCenter; text: I18n.t("daemonHint"); dim: true; font.pixelSize: Style.font.bodySmall }
          Btn { anchors.horizontalCenter: parent.horizontalCenter; primary: true; icon: Icons.refresh; text: I18n.t("startDaemon"); onClicked: svc.startDaemon() }
        }

        Loader {
          id: pageLoader
          visible: svc.connected
          anchors.fill: parent
          active: root.opened
          property string loaded: ""
          function load() {
            var files = {
              overview: "OverviewPage.qml", problems: "ProblemsPage.qml", editor: "RouteEditor.qml", rule: "RuleEditor.qml",
              rules: "RulesPage.qml", proxies: "ProxiesPage.qml", profiles: "ProfilesPage.qml",
              connections: "ConnectionsPage.qml", traffic: "TrafficPage.qml", logs: "LogsPage.qml", settings: "SettingsPage.qml", notice: "NoticePage.qml"
            }
            if (loaded === root.page && item) return
            loaded = root.page
            svc.pageGeneration++
            setSource(files[root.page] || files.problems, { svc: svc, panel: root })
          }
          Component.onCompleted: load()
          onActiveChanged: if (active) load()
          Connections {
            target: root
            function onPageChanged() { pageLoader.load() }
            function onEditingChanged() { if (root.page === "editor" || root.page === "rule") { pageLoader.loaded = ""; pageLoader.load() } }
          }
        }

        Rectangle {
          visible: root.toast !== ""
          anchors.horizontalCenter: parent.horizontalCenter
          anchors.bottom: parent.bottom
          anchors.bottomMargin: Style.space(4)
          width: toastText.implicitWidth + Style.space(24)
          height: toastText.implicitHeight + Style.space(12)
          radius: Style.cornerRadius
          color: Color.popups.background
          border.width: 1
          border.color: Color.accent
          Txt { id: toastText; anchors.centerIn: parent; text: root.toast; font.pixelSize: Style.font.bodySmall }
        }
      }
    }
  }
}
