import QtQuick
import QtTest
import "../../plugin" as Plugin

Rectangle {
  id: scene
  width: footer.width + 40; height: 110
  color: "#181825"
  QtObject {
    id: service
    property bool connected: true
    property var status: ({})
    property var issues: []
    property var traffic: ({ down: 999.9 * 1048576, up: 999.9 * 1048576 })
    property bool systemProxyWanted: false
    property color red: "#f38ba8"
    property color yellow: "#f9e2af"
    property color green: "#a6e3a1"
    property color peach: "#fab387"
    property int calls: 0
    property string method: ""
    property var callback: null
    property var lastFix: null
    function formatBytes(n) { return (n / 1024).toFixed(0) + " КБ" }
    function formatRate(n) { return n >= 1048576 ? (n / 1048576).toFixed(1) + " МБ/с" : formatBytes(n) + "/с" }
    function request(m, params, cb, keep) { calls++; method = m; callback = cb }
    function startDaemon() { calls++; method = "daemon" }
    function runFix(fix) { calls++; lastFix = fix }
  }
  QtObject {
    id: panel
    property string notification: ""
    property string page: ""
    function notify(text) { notification = text }
    function showPage(key) { page = key }
  }
  Plugin.NavStatus {
    id: footer
    x: 10; y: 12; width: implicitWidth
    service: service
    panel: panel
  }
  TestCase {
    name: "NavStatus"
    when: windowShown

    function init() {
      Plugin.I18n.language = "ru"
      service.connected = true
      service.status = { core: { running: true, tun: true, tun_stack: "gvisor", mixed_port: 7897 } }
      service.issues = []
      service.systemProxyWanted = false
      service.calls = 0; service.method = ""; service.callback = null; service.lastFix = null
      footer.actionBusy = false
      panel.notification = ""; panel.page = ""
    }
    function changeCore(patch) {
      service.status = { core: Object.assign({}, service.status.core, patch) }
    }
    function test_all_design_states_data() {
      return [
        { tag: "01-normal", key: "running", core: {}, text: "TUN · gVisor" },
        { tag: "02-tun-off", key: "tun_off", core: {tun: false}, text: "TUN выкл · прокси :7897" },
        { tag: "03-starting", key: "starting", core: {}, operation: {kind: "applying", detail: "Config.yml"}, text: "проверка Config.yml" },
        { tag: "04-restarting", key: "restarting", core: {running: false, restarting: true, restart_count: 2, crashed_at: Date.now() - 40000}, text: "TUN · gVisor" },
        { tag: "05-downloading", key: "downloading", core: {running: false}, operation: {kind: "downloading", detail: "v1.19.32", downloaded: 14680064, total: 23068672}, text: "" },
        { tag: "06-tun-failed", key: "tun_failed", core: {tun: false, tun_issue: {no_capabilities: false}}, text: "TUN не поднялся" },
        { tag: "07-no-caps", key: "no_caps", core: {tun: false, tun_issue: {no_capabilities: true}}, text: "TUN: нет прав" },
        { tag: "08-other-core", key: "other_core", core: {}, issues: [{id: "other_core", detail: "Clash Verge"}], text: "мешает Clash Verge" },
        { tag: "09-stopped", key: "stopped", core: {running: false}, text: "трафик идёт напрямую" },
        { tag: "10-disconnected", key: "disconnected", core: {}, disconnected: true, text: "состояние неизвестно" },
        { tag: "11-offline", key: "offline", core: {}, network: {offline: true, settling: false}, text: "ошибки не учитываются" },
        { tag: "12-settling", key: "settling", core: {}, network: {offline: false, settling: true}, text: "ошибки не учитываются" },
        { tag: "13-offline-core-down", key: "stopped", core: {running: false}, network: {offline: true}, text: "трафик идёт напрямую" }
      ]
    }
    function test_all_design_states(data) {
      changeCore(data.core)
      service.status = Object.assign({}, service.status, {core_operation: data.operation || null, network: data.network || null})
      service.issues = data.issues || []
      service.connected = !data.disconnected
      compare(footer.stateKey, data.key)
      compare(footer.rows.length, 3)
      compare(footer.rows[1].text, data.text)
      wait(30)
      var last = findChild(footer, "navStatusRow2")
      verify(last.y + last.height <= footer.height)
      grabImage(scene).save("/tmp/omm-nav-" + data.tag + ".png")
    }
    function test_disconnect_overrides_stale_operation_and_health() {
      service.status = {core: {running: true}, core_operation: {kind: "downloading", downloaded: 99, total: 100}}
      service.issues = [{id: "other_core", detail: "Clash Verge"}]
      service.connected = false
      compare(footer.stateKey, "disconnected")
      compare(footer.rows[2].action, "daemon")
      verify(footer.rows[2].text.indexOf("99") === -1)
      mouseClick(findChild(footer, "navStatusRow2"))
      mouseClick(findChild(footer, "navStatusRow2"))
      compare(service.calls, 1)
      service.connected = true
      verify(!footer.actionBusy)
    }
    function test_start_error_and_keyboard_retry() {
      changeCore({running: false})
      var action = findChild(footer, "navStatusRow2")
      mouseClick(action)
      compare(service.method, "core.start")
      verify(footer.actionBusy)
      mouseClick(action)
      compare(service.calls, 1)
      service.callback(null, {message: "failed to start"})
      compare(panel.notification, "failed to start")
      verify(!footer.actionBusy)
      action.forceActiveFocus()
      keyClick(Qt.Key_Return)
      compare(service.calls, 2)
    }
    function test_permissions_action_uses_privileged_fix() {
      changeCore({tun: false, tun_issue: {no_capabilities: true}})
      mouseClick(findChild(footer, "navStatusRow2"))
      compare(service.calls, 1)
      compare(service.lastFix.id, "setcap")
      verify(service.lastFix.privileged)
    }
    function test_download_unknown_length_and_recovery() {
      service.status = {core: {running: false}, core_operation: {kind: "downloading", detail: "v1.19.32", downloaded: 0}}
      compare(footer.progress, -1)
      service.status = {core: {running: false}, core_operation: {kind: "downloading", detail: "v1.19.32", downloaded: 14 * 1048576, total: 22 * 1048576}}
      compare(footer.progress, 14 / 22)
      compare(footer.rows[2].text, "v1.19.32 · 14/22 МБ")
      service.status = {core: {running: false}}
      service.issues = [{id: "core_install_failed", detail: "timeout"}]
      compare(footer.stateKey, "stopped")
      compare(footer.rows[2].action, "overview")
      mouseClick(findChild(footer, "navStatusRow2"))
      compare(panel.page, "overview")
    }
    function test_live_values_and_language() {
      changeCore({tun_stack: "system"})
      compare(footer.rows[1].text, "TUN · system")
      service.traffic = {down: 0, up: 1024}
      compare(footer.rows[2].text, "↓ 0 КБ/с  ↑ 1 КБ/с")
      changeCore({tun: false, mixed_port: 8080})
      compare(footer.rows[1].text, "TUN выкл · прокси :8080")
      Plugin.I18n.language = "en"
      compare(footer.rows[1].text, "TUN off · proxy :8080")
    }
    function cleanupTestCase() { Plugin.I18n.language = "ru" }
  }
}
