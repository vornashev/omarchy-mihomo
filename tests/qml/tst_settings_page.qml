import QtQuick
import QtTest
import "../../plugin" as Plugin

Rectangle {
  id: scene
  width: 860; height: 660
  color: "#1e1e2e"
  QtObject {
    id: service
    property var status: ({})
    property var issues: []
    property var fixing: ({})
    property var automation: ({ autofix: true, mode: "trial", failures: 3, window_min: 5, trial_min: 60, exclude_groups: [], strict_groups: [], ignore: [], notify: {}, probe_nodes: true })
    property color red: "#f38ba8"
    property color green: "#a6e3a1"
    property color yellow: "#f9e2af"
    property color peach: "#fab387"
    property var calls: []
    function formatBytes(n) { return n + " B" }
    function clock(ms) { return "12:00" }
    function runFix(f) {}
    function request(m, params, cb) {
      calls = calls.concat([{ method: m, params: params }])
      if (!cb) return
      if (m === "settings.get") cb({ "mixed-port": 7897, "update-interval-hours": 12, "allow-lan": false, ipv6: false, "find-process-mode": "always",
        tun: { enable: true, stack: "gvisor" }, "geo-auto-update": true, "geo-update-interval": 24, dns: {}, "rule-sets": [] }, null)
      else if (m === "backup.export") cb({ path: "/home/u/omarchy-mihomo-2026-10-27.json", rules: 12, profiles: 2 }, null)
      else if (m === "backup.import") cb({ rules: 12, profiles_added: ["Home"], profiles_kept: [], kept_tun: false, before: "/x" }, null)
      else if (m === "traffic.status") cb({ enabled: true, rows: 10, bytes: 4096 }, null)
      else if (m === "dns.get") cb({ settings: { policy: [], "fake-ip-filter": [] }, effective: { dns: {}, sniffer: false }, doh: "https://1.1.1.1/dns-query" }, null)
      else if (m === "core.updates") cb({ supported: true, current: "v1.19.32", geo: [] }, null)
      else cb([], null)
    }
  }
  QtObject {
    id: panel
    property string settingsTab: "main"
    property string moduleName: "mihomo"
    property var settings: ({})
    property var bar: null
    property string notification: ""
    function notify(t) { notification = t }
    function forceKeys() {}
    function showPage(p) {}
  }
  Plugin.SettingsPage {
    id: page
    anchors.fill: parent
    anchors.margins: 20
    svc: service
    panel: panel
  }
  TestCase {
    name: "SettingsPage"
    when: windowShown
    function init() {
      Plugin.I18n.language = "ru"
      panel.settingsTab = "main"
      service.calls = []
      panel.notification = ""
      waitForRendering(page)
    }
    function last() { return service.calls[service.calls.length - 1] }

    function test_backup_export_then_import() {
      var exp = findChild(page, "backupExport")
      verify(exp.visible)
      mouseClick(exp)
      compare(last().method, "backup.export")
      compare(last().params.path, null, "empty field: the daemon picks the path")
      compare(findChild(page, "backupPath").text, "/home/u/omarchy-mihomo-2026-10-27.json")
      compare(panel.notification, "Сохранено: 12 правил, 2 профилей → /home/u/omarchy-mihomo-2026-10-27.json")
      mouseClick(findChild(page, "backupImport"))
      compare(service.calls.filter(function(c) { return c.method === "backup.import" })[0].params.path, "/home/u/omarchy-mihomo-2026-10-27.json")
      compare(panel.notification, "Загружено: 12 правил; профилей добавлено 1, оставлено как было 0")
      grabImage(scene).save("/tmp/omm-settings-main.png")
    }

    function test_every_tab_opens() {
      var tabs = ["auto", "notify", "core", "dns", "main"]
      for (var i = 0; i < tabs.length; i++) {
        panel.settingsTab = tabs[i]
        waitForRendering(page)
      }
      verify(findChild(page, "backupExport").visible)
    }
  }
}
