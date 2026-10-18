import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons

// Connection to omarchy-mihomod: one unix socket, newline-delimited JSON.
// Requests carry an id and get a callback; the daemon pushes `status`,
// `problems`, and (while a page needs them) `traffic` and `logs` events.
// All routing logic lives in the daemon; this object only mirrors state.
Item {
  id: root

  property var settings: ({})
  property bool panelVisible: false
  onPanelVisibleChanged: {
    if (connected) subscribe()
    if (!panelVisible) traffic = { up: 0, down: 0 }
  }

  readonly property string socketPath: {
    var custom = settings && settings.socketPath ? String(settings.socketPath) : ""
    if (custom !== "") return custom
    var runtime = Quickshell.env("XDG_RUNTIME_DIR") || ("/run/user/" + (Quickshell.env("UID") || "1000"))
    return runtime + "/omarchy-mihomo/daemon.sock"
  }

  // ---------------------------------------------------------------- state

  readonly property bool connected: sock.connected
  property var status: null
  readonly property bool coreRunning: !!(status && status.core && status.core.running)
  onCoreRunningChanged: if (!coreRunning) traffic = { up: 0, down: 0 }
  readonly property string mode: status && status.core && status.core.mode ? status.core.mode : ""
  readonly property bool tun: !!(status && status.core && status.core.tun)
  readonly property var systemProxyStatus: status && status.system_proxy ? status.system_proxy : null
  readonly property bool systemProxy: !!(systemProxyStatus && systemProxyStatus.enabled)
  readonly property bool systemProxyWanted: !!(systemProxyStatus && systemProxyStatus.wanted)
  readonly property bool systemProxyAvailable: !!(systemProxyStatus && systemProxyStatus.available)
  readonly property string systemProxyError: systemProxyStatus && systemProxyStatus.error ? systemProxyStatus.error : ""
  property bool systemProxyBusy: false
  signal systemProxyFailed(string message)

  function toggleSystemProxy() {
    if (systemProxyBusy || !connected) return
    systemProxyBusy = true
    request("system_proxy.set", { enable: !(systemProxy || systemProxyWanted) }, function(r, err) {
      systemProxyBusy = false
      if (r && root.status) {
        var next = Object.assign({}, root.status)
        next.system_proxy = r
        root.status = next
      }
      if (err) root.systemProxyFailed(err.message)
    }, true)
  }
  // TUN is on in the settings, yet the core runs without it.
  readonly property var tunIssue: status && status.core && status.core.tun_issue ? status.core.tun_issue : null
  // Setup problems, each with the fix the daemon offers.
  readonly property var issues: status && status.issues ? status.issues : []
  // Fix ids running now.
  property var fixing: ({})

  function setFixing(id, on) { var f = Object.assign({}, fixing); if (on) f[id] = true; else delete f[id]; fixing = f }

  // Unprivileged fixes run in the daemon; the others need sudo, so they run
  // as `omm fix` in Omarchy's floating terminal, where sudo can ask.
  function runFix(fix) {
    if (fix.privileged) {
      var omm = Quickshell.env("HOME") + "/.local/bin/omm"
      var args = fix.id === "migrate_cvr" ? "migrate cvr" : "fix " + fix.id
      Quickshell.execDetached(["omarchy-launch-floating-terminal-with-presentation", "'" + omm + "' " + args])
      return
    }
    setFixing(fix.id, true)
    request("health.fix", { id: fix.id }, function(r, err) {
      setFixing(fix.id, false)
      Quickshell.execDetached(["notify-send", "-a", "Mihomo", "-u", err ? "critical" : "normal", I18n.t("fix_" + fix.id), err ? err.message : I18n.t("fixDone")])
    }, true)
  }
  readonly property string profileName: status && status.config && status.config.profile ? status.config.profile.name : ""
  readonly property var orphans: status && status.config && status.config.orphans ? status.config.orphans : []
  readonly property string configError: status && status.config && status.config.error ? status.config.error : ""

  property var problemCounts: ({ "new": 0, "visible": 0, "hidden": 0 })
  readonly property int newProblems: problemCounts && problemCounts["new"] ? problemCounts["new"] : 0
  // Bumped whenever the daemon recorded failures, so open lists refresh.
  property int problemsRevision: 0

  property var traffic: ({ up: 0, down: 0 })
  property var logLines: []
  readonly property int maxLogLines: 400

  // Extra topics wanted by open pages (traffic, logs).
  property var extraTopics: []

  signal logLine(var line)
  // A local profile's file was saved in an editor and checked by the daemon.
  signal profileSaved(var info)

  // ---------------------------------------------------------------- palette
  // The shell exposes only foreground/background/accent/urgent/muted; the
  // problem kinds need the theme's red/yellow/green too.

  property var themeColors: ({})
  function themeColor(name, fallback) {
    var v = themeColors[name]
    return v ? v : fallback
  }
  readonly property color red: themeColor("red", Color.urgent)
  readonly property color yellow: themeColor("yellow", Color.accent)
  readonly property color green: themeColor("green", Color.accent)
  readonly property color peach: themeColor("bright_red", red)
  readonly property color mauve: themeColor("magenta", Color.accent)

  FileView {
    id: colorsFile
    path: Color.currentThemePath + "/colors.toml"
    printErrors: false
    onLoaded: {
      var out = {}
      var lines = text().split("\n")
      for (var i = 0; i < lines.length; i++) {
        var m = lines[i].match(/^\s*([a-z_]+)\s*=\s*"(#[0-9a-fA-F]{6,8})"/)
        if (m) out[m[1]] = m[2]
      }
      root.themeColors = out
    }
  }

  // Theme switches push new colors into Color; re-read ours alongside.
  Connections {
    target: Color
    function onAccentChanged() { colorsFile.reload() }
    function onBackgroundChanged() { colorsFile.reload() }
  }

  // ---------------------------------------------------------------- transport

  property int nextId: 1
  property var pending: ({})
  // Bumped by the panel whenever it swaps pages; replies to a page that is
  // gone are dropped instead of writing into a destroyed object.
  property int pageGeneration: 0

  // `keep`: deliver the reply even if the panel changed pages meanwhile.
  function request(method, params, callback, keep) {
    if (!sock.connected) {
      if (callback) callback(null, { code: "core_unavailable", message: I18n.t("daemonDown") })
      return
    }
    var id = nextId++
    if (callback) pending[id] = { cb: callback, gen: keep ? -1 : pageGeneration }
    var frame = { id: id, method: method }
    if (params !== undefined && params !== null) frame.params = params
    sock.write(JSON.stringify(frame) + "\n")
    sock.flush()
  }

  function subscribe() {
    var topics = ["status", "problems", "profiles"].concat(extraTopics)
    if (panelVisible && topics.indexOf("traffic") === -1) topics.push("traffic")
    request("events.subscribe", { topics: topics }, null)
  }

  function want(topic, on) {
    var list = extraTopics.slice()
    var i = list.indexOf(topic)
    if (on && i === -1) list.push(topic)
    else if (!on && i !== -1) list.splice(i, 1)
    else return
    extraTopics = list
    if (sock.connected) subscribe()
  }

  function handleFrame(line) {
    var msg
    try { msg = JSON.parse(line) } catch (e) { return }
    if (msg.event !== undefined) {
      if (msg.event === "status") root.status = msg.data
      else if (msg.event === "problems") {
        root.problemCounts = msg.data.counts
        if (msg.data.recorded > 0) root.problemsRevision++
      } else if (msg.event === "profiles") {
        // The panel is usually closed while the editor is in front.
        var p = msg.data
        Quickshell.execDetached(["notify-send", "-a", "Mihomo", "-u", p.error ? "critical" : "normal",
          p.error ? I18n.t("profileSaveFailed").arg(p.name) : I18n.t("profileSaved").arg(p.name), p.error || ""])
        root.profileSaved(p)
      }
      else if (msg.event === "traffic" && connected && coreRunning && panelVisible) root.traffic = msg.data
      else if (msg.event === "logs") {
        if (Array.isArray(msg.data.lines)) {
          root.logLines = msg.data.lines.slice(-root.maxLogLines)
          return
        }
        var lines = root.logLines.slice(Math.max(0, root.logLines.length - root.maxLogLines + 1))
        lines.push(msg.data)
        root.logLines = lines
        root.logLine(msg.data)
      }
      return
    }
    var entry = pending[msg.id]
    if (entry) {
      delete pending[msg.id]
      if (entry.gen === -1 || entry.gen === root.pageGeneration) entry.cb(msg.result === undefined ? null : msg.result, msg.error || null)
    }
  }

  Socket {
    id: sock
    path: root.socketPath
    connected: true
    parser: SplitParser {
      onRead: function(data) { root.handleFrame(data) }
    }
    onConnectionStateChanged: {
      if (connected) {
        retry.interval = 500
        root.subscribe()
      } else {
        // Fail outstanding calls instead of leaving the UI waiting.
        var calls = root.pending
        root.pending = {}
        for (var id in calls) if (calls[id].gen === -1 || calls[id].gen === root.pageGeneration) calls[id].cb(null, { code: "core_unavailable", message: I18n.t("daemonDown") })
        root.status = null
        retry.start()
      }
    }
  }

  // Reconnect with backoff while the daemon is down (e.g. restarting).
  Timer {
    id: retry
    interval: 500
    onTriggered: {
      if (sock.connected) return
      sock.connected = false
      sock.connected = true
      interval = Math.min(interval * 2, 10000)
      start()
    }
  }
  Component.onCompleted: if (!sock.connected) retry.start()
  // Settings arrive after creation; a changed path needs a new connection.
  onSocketPathChanged: {
    sock.connected = false
    retry.interval = 100
    retry.restart()
  }

  // ---------------------------------------------------------------- helpers

  function startDaemon() {
    Quickshell.execDetached(["systemctl", "--user", "start", "omarchy-mihomod.service"])
    retry.interval = 500
    retry.restart()
  }

  function formatBytes(n) {
    n = Number(n) || 0
    var units = ["B", "KB", "MB", "GB", "TB"]
    var i = 0
    while (n >= 1024 && i < units.length - 1) { n /= 1024; i++ }
    return (i === 0 ? n.toFixed(0) : n.toFixed(1)) + " " + I18n.t("unit" + units[i])
  }

  function formatRate(n) {
    return formatBytes(n) + I18n.t("perSec")
  }

  function ago(ms) {
    if (!ms) return I18n.t("never")
    var s = Math.max(0, Math.floor((Date.now() - ms) / 1000))
    if (s < 45) return I18n.t("justNow")
    if (s < 3600) return I18n.t("minAgo").arg(Math.round(s / 60))
    if (s < 86400) return I18n.t("hAgo").arg(Math.round(s / 3600))
    return I18n.t("dAgo").arg(Math.round(s / 86400))
  }

  function clock(ms) {
    if (!ms) return ""
    var d = new Date(ms)
    var today = new Date()
    var hm = ("0" + d.getHours()).slice(-2) + ":" + ("0" + d.getMinutes()).slice(-2)
    if (d.toDateString() === today.toDateString()) return hm
    return d.getDate() + "." + ("0" + (d.getMonth() + 1)).slice(-2) + " " + hm
  }

  function kindColor(kind) {
    if (kind === "timeout") return red
    if (kind === "reset" || kind === "refused" || kind === "no_route") return peach
    if (kind === "dns_fail" || kind === "dns_stub") return yellow
    if (kind === "tls" || kind === "proxy") return mauve
    return Color.muted
  }
}
