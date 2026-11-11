import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// Bar widget + popup panel for omarchy-mihomo.
Panel {
  id: root
  moduleName: "vornashev.omarchy-mihomo"
  ipcTarget: "vornashev.omarchy-mihomo"
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
  // Text the connections page filters by, taken when it opens or is open.
  property string connectionsFilter: ""
  // The event shown on the compact notice page.
  property string noticeKey: ""
  // Settings tab to show next: main | auto.
  property string settingsTab: "main"
  // Three windows: "compact" (a summary by the icon), "normal" (the full
  // interface by the icon) and "large" (centred over a dimmed screen). Each
  // mouse button opens the one chosen in settings.
  readonly property var windows: ["compact", "normal", "large"]
  readonly property string leftClick: windowSetting("leftClick", "normal")
  readonly property string rightClick: windowSetting("rightClick", "large")
  // Where a full page goes from the compact window.
  readonly property string fullWindow: leftClick !== "compact" ? leftClick : (rightClick !== "compact" ? rightClick : "normal")
  // The window in use; set when opening.
  property string surface: leftClick
  // What the window shows. A notice is always the small card by the icon.
  readonly property string view: page === "notice" ? "notice" : (surface === "compact" ? "compact" : page)
  // A small card without the page rail.
  readonly property bool compact: view === "notice" || view === "compact"
  readonly property bool large: surface === "large" && !compact

  function windowSetting(name, fallback) {
    var value = String(setting(name, fallback))
    return windows.indexOf(value) >= 0 ? value : fallback
  }

  readonly property var pages: [
    { key: "overview", icon: Icons.overview, label: MihomoI18n.t("navOverview") },
    { key: "problems", icon: Icons.problems, label: MihomoI18n.t("navProblems") },
    { key: "rules", icon: Icons.rules, label: MihomoI18n.t("navRules") },
    { key: "proxies", icon: Icons.proxies, label: MihomoI18n.t("navProxies") },
    { key: "sites", icon: Icons.sites, label: MihomoI18n.t("navSites") },
    { key: "profiles", icon: Icons.profiles, label: MihomoI18n.t("navProfiles") },
    { key: "connections", icon: Icons.connections, label: MihomoI18n.t("navConnections") },
    { key: "traffic", icon: Icons.traffic, label: MihomoI18n.t("navTraffic") },
    { key: "logs", icon: Icons.logs, label: MihomoI18n.t("navLogs") },
    { key: "settings", icon: Icons.settings, label: MihomoI18n.t("navSettings") }
  ]

  readonly property color barIconColor: !svc.connected ? Qt.darker(barForeground, 1.6)
    : (svc.status && svc.status.core && svc.status.core.error ? Commons.Color.urgent
    : (svc.coreRunning ? (svc.tun ? svc.green : barForeground) : Qt.darker(barForeground, 1.6)))

  Binding { target: MihomoI18n; property: "language"; value: MihomoI18n.resolve(root.setting("locale", "auto")) }

  function showPage(key) {
    if (key !== page && pageLoader.item && pageLoader.item.canLeave && !pageLoader.item.canLeave()) return
    if (key !== "editor" && key !== "rule") editing = null
    if (key !== "notice") svc.noticeOnScreen = ""
    page = key
    summonFull()
  }

  // Connections page filtered by `query`, from the title bar search.
  function showConnections(query) {
    connectionsFilter = query
    showPage("connections")
  }

  function openEditor(target) {
    editing = target
    page = "editor"
    summonFull()
  }

  // Rule form: { from, origin?, rule?, kind?, value?, full?, duplicate? }.
  function openRule(spec) {
    editing = spec
    page = "rule"
    summonFull()
  }

  // Opened from a desktop notification (`openNotice` over IPC).
  function openNotice(key) {
    var show = function() {
      noticeKey = key
      svc.noticeOnScreen = key
      editing = null
      page = "notice"
      root.summon()
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

  // Open where a left click would, unless already open.
  function summon() {
    if (opened) return
    surface = leftClick
    open()
  }

  // The compact window has no pages: they open in a full one.
  function summonFull() {
    summon()
    if (surface === "compact") surface = fullWindow
  }

  // "Open in full" from the compact window.
  function openFull() {
    if (page === "notice") page = "overview"
    surface = fullWindow
  }

  // A click on the icon: open that window, move there, or close it.
  function clickOpen(kind) {
    if (opened && (surface === kind || view === "notice")) { close(); return }
    // A notice once shown is not what a click on the icon asks for.
    if (!opened && page === "notice") page = "overview"
    surface = kind
    if (!opened) open()
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
    function open(): void { root.summon() }
    function close(): void { root.close() }
    function toggle(): void { if (root.opened) root.close(); else root.summon() }
    function openPage(name: string): void { root.showPage(name) }
    // compact | normal | large, e.g. for a hotkey.
    function openWindow(kind: string): void {
      if (root.windows.indexOf(kind) < 0) return
      if (root.page === "notice") root.page = "overview"
      root.surface = kind
      root.open()
    }
    // Clicking one of our desktop notifications lands here.
    function openNotice(key: string): void { root.openNotice(key) }
    function ruleForWindow(): void { root.ruleForWindow() }
    // Open the route editor for a problem host, e.g. from a script.
    function route(host: string): void {
      svc.request("problems.get", { host: host }, function(p, err) {
        if (err || !p) { root.showPage("problems"); root.notify(err ? err.message : host); return }
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
    text: !svc.connected || !svc.coreRunning ? Icons.shieldOff : (svc.status.core.error ? Icons.shieldAlert : Icons.shield)
    foreground: root.barIconColor
    useActiveColor: false
    tooltipText: !svc.connected ? MihomoI18n.t("daemonDown")
      : (svc.coreRunning ? MihomoI18n.t("barTooltip").arg(svc.profileName || "—").arg(svc.mode).arg(svc.newProblems) : MihomoI18n.t("coreDown"))
    // Badges must stay outside iconComponent: Omarchy 4.5 fits that entire
    // component to the icon size, shrinking and shifting the shield with it.
    Item {
      anchors.centerIn: parent
      width: Style.bar.iconCanvas
      height: Style.bar.iconCanvas
      // Autofix is doing something (a rule on trial, a suggestion).
      Text {
        visible: svc.autofixActive > 0
        anchors.left: parent.left
        anchors.bottom: parent.bottom
        anchors.leftMargin: -Style.space(5)
        anchors.bottomMargin: -Style.space(3)
        text: Icons.sparkles
        color: Commons.Color.accent
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
        color: root.bar ? root.bar.urgent : Commons.Color.urgent
        Text {
          id: badgeLabel
          anchors.centerIn: parent
          text: svc.newProblems > 99 ? "99+" : String(svc.newProblems)
          color: Commons.Color.background
          font.family: Style.font.family
          font.pixelSize: Math.max(8, Style.font.caption - 1)
          font.bold: true
        }
      }
    }
    onPressed: function(buttonCode) {
      if (buttonCode === Qt.MiddleButton) svc.request("tun.set", { enable: !svc.tun }, null)
      else if (buttonCode === Qt.RightButton) root.clickOpen(root.rightClick)
      else root.clickOpen(root.leftClick)
    }
  }

  Timer { id: toastTimer; interval: 3500; onTriggered: root.toast = "" }

  // One size for every full page, so switching pages never resizes the window.
  readonly property int fullWidth: Style.space(1080) + rail.width - Style.space(190)

  KeyboardPanel {
    id: panel
    anchorItem: button
    owner: root
    bar: root.bar
    open: root.opened && !root.large
    focusTarget: keyCatcher
    padding: 0
    contentWidth: panel.fittedContentWidth(root.view === "notice" ? Style.space(380) : (root.view === "compact" ? Style.space(400) : root.fullWidth))
    // The compact window grows with its content up to 70 % of the screen.
    contentHeight: root.compact ? panel.fittedContentHeight((pageLoader.item ? pageLoader.item.implicitHeight : Style.space(200)) + Style.space(32),
        root.view === "compact" ? panel.screenH * 0.7 : undefined)
      : panel.fittedContentHeight(Style.space(720))

    Item { id: popupSlot; anchors.fill: parent }
  }

  ModalPanel {
    id: modal
    anchorItem: button
    owner: root
    bar: root.bar
    open: root.opened && root.large
    focusTarget: keyCatcher
    minWidth: root.fullWidth
  }

  // One set of pages for both windows: it moves to the one in use.
  PanelKeyCatcher {
    id: keyCatcher
    parent: root.large ? modal.slot : popupSlot
    anchors.fill: parent
    // Let text fields have the keyboard.
    blocked: (pageLoader.item && pageLoader.item.typing === true) || titleBar.typing
    onMoveRequested: function(dx, dy) { if (pageLoader.item && pageLoader.item.move) pageLoader.item.move(dx, dy) }
    onActivateRequested: if (pageLoader.item && pageLoader.item.activate) pageLoader.item.activate()
    onDeleteRequested: if (pageLoader.item && pageLoader.item.remove) pageLoader.item.remove()
    onCloseRequested: {
      if (pageLoader.item && pageLoader.item.canLeave && !pageLoader.item.canLeave()) return
      if (root.compact) root.close()
      else if (root.page === "editor") root.showPage("problems")
      else if (root.page === "rule" && root.editing && root.editing.from === "window") root.close()
      else if (root.page === "rule") root.showPage(root.editing && root.editing.from ? root.editing.from : "rules")
      else root.close()
    }
    onTabRequested: function(direction) { root.switchPanel(direction) }
    onTextKey: function(t) {
      // 1…9, then 0 for the tenth page.
      var n = t === "0" ? 10 : parseInt(t)
      if (n >= 1 && n <= root.pages.length) { root.showPage(root.pages[n - 1].key); return }
      if (t === "/" && root.large) { titleBar.focusSearch(); return }
      if (pageLoader.item && pageLoader.item.textKey) pageLoader.item.textKey(t)
    }

    WindowTitleBar {
      id: titleBar
      visible: root.large
      anchors.left: parent.left
      anchors.right: parent.right
      anchors.top: parent.top
      height: visible ? implicitHeight : 0
      svc: svc
      panel: root
    }

    WindowFooter {
      id: footer
      visible: root.large
      anchors.left: parent.left
      anchors.right: parent.right
      anchors.bottom: parent.bottom
      height: visible ? implicitHeight : 0
      svc: svc
    }

    Rectangle {
      id: rail
      visible: !root.compact
      anchors.left: parent.left
      anchors.top: titleBar.bottom
      anchors.bottom: footer.top
      width: root.compact ? 0 : Math.max(Style.space(190), navStatus.implicitWidth + Style.space(20))
      color: Util.alpha(Commons.Color.popups.background, 0.6)
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
          Txt { text: Icons.shield; color: svc.coreRunning ? Commons.Color.accent : Commons.Color.muted; font.pixelSize: Style.font.heading; anchors.verticalCenter: parent.verticalCenter }
          Column {
            Txt { text: "mihomo"; bold: true }
            Txt {
              text: svc.coreRunning ? (svc.status.core.version || "") + " · " + MihomoI18n.mode(svc.mode) : (svc.connected ? MihomoI18n.t("coreStopped") : MihomoI18n.t("noDaemon"))
              faint: true; font.pixelSize: Style.font.bodySmall
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
            keyHint: String((index + 1) % 10)
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
      anchors.top: titleBar.bottom
      anchors.bottom: footer.top
      anchors.margins: Style.space(root.compact ? 16 : 18)

      // Daemon not reachable: one clear action instead of empty pages.
      Column {
        visible: !svc.connected
        anchors.centerIn: parent
        spacing: Style.space(12)
        Txt { anchors.horizontalCenter: parent.horizontalCenter; text: Icons.shieldOff; faint: true; font.pixelSize: Style.font.displayLarge }
        Txt { anchors.horizontalCenter: parent.horizontalCenter; text: MihomoI18n.t("daemonDown"); bold: true }
        Txt { anchors.horizontalCenter: parent.horizontalCenter; text: MihomoI18n.t("daemonHint"); dim: true; font.pixelSize: Style.font.body }
        Btn { anchors.horizontalCenter: parent.horizontalCenter; primary: true; icon: Icons.refresh; text: MihomoI18n.t("startDaemon"); onClicked: svc.startDaemon() }
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
            rules: "RulesPage.qml", proxies: "ProxiesPage.qml", sites: "SitesPage.qml", profiles: "ProfilesPage.qml",
            connections: "ConnectionsPage.qml", traffic: "TrafficPage.qml", logs: "LogsPage.qml", settings: "SettingsPage.qml", notice: "NoticePage.qml",
            compact: "CompactPage.qml"
          }
          if (loaded === root.view && item) return
          loaded = root.view
          svc.pageGeneration++
          setSource(files[root.view] || files.problems, { svc: svc, panel: root })
        }
        Component.onCompleted: load()
        onActiveChanged: if (active) load()
        Connections {
          target: root
          function onViewChanged() { pageLoader.load() }
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
        color: Commons.Color.popups.background
        border.width: 1
        border.color: Commons.Color.accent
        Txt { id: toastText; anchors.centerIn: parent; text: root.toast; font.pixelSize: Style.font.body }
      }
    }
  }
}
