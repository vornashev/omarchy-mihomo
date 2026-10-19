import QtQuick
import QtTest
import "../../plugin" as Plugin
import "../../plugin/Notices.js" as Notices

Rectangle {
  id: scene
  width: 420; height: 660
  color: "#1e1e2e"
  QtObject {
    id: mockPanel
    property string page: "notice"
    property string noticeKey: "autofix:chatgpt.com"
    property string settingsTab: "auto"
    property string toast: ""
    property bool closed: false
    function showPage(value) { page = value }
    function notify(value) { toast = value }
    function close() { closed = true }
    function forceKeys() {}
  }
  QtObject {
    id: mockService
    property var calls: []
    property color red: "#f38ba8"
    property color green: "#a6e3a1"
    property color yellow: "#f9e2af"
    property color peach: "#fab387"
    property var fixes: ({})
    property var notices: ({})
    property var automation: ({ autofix: true, mode: "trial", failures: 3, window_min: 5, trial_min: 60, exclude_groups: [], ignore: [], notify: { core: true, dead_group: true, subscription: true, orphans: true, autofix: true, saved: true } })
    function clock(ms) { return "17:42" }
    function setFix(f) { var n = Object.assign({}, fixes); n[f.host] = f; fixes = n }
    function last(method) { var c = calls.filter(function(x) { return x.method === method }); return c.length ? c[c.length - 1].params : null }
    function request(method, params, callback) {
      calls.push({ method: method, params: params })
      var r = {}
      if (method === "automation.set") r = params
      else if (method === "proxies.targets") r = ["DIRECT", "AI", "PROXY"]
      else if (method.indexOf("autofix.") === 0) r = Object.assign({}, fixes[params.host], { state: method === "autofix.pin" ? "pinned" : "trial" })
      if (callback) callback(r, null)
    }
  }

  function fix(state) {
    return {
      host: "chatgpt.com", port: 443, apps: ["firefox"], kind: "timeout", failures: 4, was: "DIRECT", was_rule: "Match",
      state: state, target: "AI", rule_id: 7, rule: "DOMAIN-SUFFIX,chatgpt.com,AI",
      results: [{ target: "AI", ok: true, delay: 212 }, { target: "PROXY", ok: true, delay: 348 }, { target: "DIRECT", ok: false, error: "timeout" }],
      checked_at: 1, applied_at: 1, ends_at: Date.now() + 52 * 60000, trial_ok: 37, trial_failed: 0, tried: [], updated: 1
    }
  }

  Component { id: noticeComponent; Plugin.NoticePage { width: 380; svc: mockService; panel: mockPanel } }
  Component { id: stripComponent; Plugin.AutofixStrip { width: 560; svc: mockService; panel: mockPanel } }
  Component { id: settingsComponent; Plugin.AutomationSettings { width: 600; height: 600; svc: mockService; panel: mockPanel } }

  TestCase {
    name: "Autofix"
    when: windowShown

    function init() {
      mockService.calls = []
      mockService.fixes = {}
      mockService.notices = {}
      mockPanel.page = "notice"
      mockPanel.closed = false
      mockPanel.toast = ""
    }

    function findText(item, text) {
      if (item.text !== undefined && String(item.text).indexOf(text) !== -1 && item.visible) return item
      for (var i = 0; i < item.children.length; i++) {
        var found = findText(item.children[i], text)
        if (found) return found
      }
      return null
    }

    function test_notice_texts() {
      var t = function(k) { return Plugin.I18n.t(k) }
      var trial = Notices.text({ kind: "autofix", data: fix("trial") }, t, function() { return "18:42" })
      verify(trial.title.indexOf("chatgpt.com") !== -1 && trial.title.indexOf("AI") !== -1, trial.title)
      verify(trial.body.indexOf("212") !== -1 && trial.body.indexOf("18:42") !== -1, trial.body)
      verify(trial.click !== "")
      var down = Notices.text({ kind: "core_down", data: { restarting: true, restart_count: 1 } }, t, function() { return "" })
      verify(down.title.indexOf("2") !== -1, down.title)
      var orphans = Notices.text({ kind: "orphans", data: { rules: [{ target: "TG[DE]" }, { target: "TG[DE]" }] } }, t, function() { return "" })
      verify(orphans.body.indexOf("TG[DE]") !== -1 && orphans.body.indexOf("TG[DE], TG[DE]") === -1, orphans.body)
      compare(Notices.text({ kind: "nope", data: {} }, t, function() { return "" }), null)
    }

    function test_trial_popup_pins_and_undoes() {
      var f = fix("trial")
      mockService.fixes = { "chatgpt.com": f }
      mockService.notices = { "autofix:chatgpt.com": { key: "autofix:chatgpt.com", kind: "autofix", urgency: "normal", data: f, at: 1 } }
      var page = createTemporaryObject(noticeComponent, scene)
      waitForPolish(page)
      wait(20)
      verify(page.implicitHeight > 100)
      verify(findText(page, "212"), "results are listed")
      var pin = findText(page, Plugin.I18n.t("afPin"))
      verify(pin)
      pin.clicked()
      compare(mockService.last("autofix.pin").host, "chatgpt.com")
      compare(mockService.fixes["chatgpt.com"].state, "pinned")
    }

    function test_found_popup_offers_alternatives() {
      var f = fix("found")
      mockService.fixes = { "chatgpt.com": f }
      mockService.notices = { "autofix:chatgpt.com": { key: "autofix:chatgpt.com", kind: "autofix", urgency: "normal", data: f, at: 1 } }
      var page = createTemporaryObject(noticeComponent, scene)
      waitForPolish(page)
      wait(20)
      var alt = findText(page, "PROXY  348")
      verify(alt, "the other working group is offered")
      alt.clicked()
      compare(mockService.last("autofix.apply").target, "PROXY")
      compare(mockService.fixes["chatgpt.com"].state, "trial", "the popup follows the live fix")
      mockService.fixes = { "chatgpt.com": f }
      var notNow = findText(page, Plugin.I18n.t("afNotNow"))
      notNow.clicked()
      compare(mockService.last("autofix.dismiss").host, "chatgpt.com")
      verify(mockPanel.closed)
    }

    function test_strip_states() {
      var strip = createTemporaryObject(stripComponent, scene, { fix: fix("found") })
      wait(20)
      verify(strip.visible && strip.implicitHeight > 0)
      verify(findText(strip, "212"))
      strip.fix = fix("pinned")
      verify(!strip.visible, "a pinned fix needs no strip")
      strip.fix = fix("trial")
      wait(20)
      verify(findText(strip, "37"), "trial shows connections so far")
      var undo = findText(strip, Plugin.I18n.t("afUndo"))
      undo.clicked()
      compare(mockService.last("autofix.undo").host, "chatgpt.com")
    }

    function test_settings_save_whole_object() {
      var page = createTemporaryObject(settingsComponent, scene)
      wait(20)
      var suggest = findText(page, Plugin.I18n.t("autoMode_suggest"))
      verify(suggest)
      mouseClick(suggest)
      var saved = mockService.last("automation.set")
      compare(saved.mode, "suggest")
      compare(saved.failures, 3, "unchanged fields are sent along")
      var ai = findText(page, "AI")
      verify(ai)
      mouseClick(ai)
      compare(mockService.last("automation.set").exclude_groups, ["AI"])
    }
  }
}
