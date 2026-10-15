import QtQuick
import QtTest
import "../../plugin" as Plugin
import "../../plugin/ProblemModels.js" as Models

Rectangle {
  width: 636; height: 660; color: "#1e1e2e"
  id: scene
  QtObject {
    id: mockPanel
    property string problemsView: "table"
    property var editing: null
    property string page: "problems"
    property string toast: ""
    function openEditor(value) { editing = value }
    function showPage(value) { page = value }
    function notify(value) { toast = value }
    function forceKeys() {}
  }
  QtObject {
    id: mockService
    signal problemsRevisionChanged()
    property var data: []
    property var calls: []
    property var pending: []
    property bool defer: false
    property color red: "#f38ba8"
    property color green: "#a6e3a1"
    property color yellow: "#f9e2af"
    function kindColor(k) { return k === "reset" ? "#fab387" : red }
    function ago(ms) { return "сейчас" }
    function clock(ms) { return "12:00" }
    function request(method, params, callback) {
      calls.push({ method: method, params: params })
      if (method === "problems.list") {
        var result = { items: data.filter(function(p) { return (!params.only_new || p.new) && (!params.search || p.host.indexOf(params.search) >= 0) && (!params.kind || p.kinds[params.kind]) }), counts: { new: 3, visible: data.length, hidden: 0 } }
        if (defer) pending.push(function() { callback(result, null) })
        else callback(result, null)
      } else if (method === "problems.ignores") callback([], null)
      else if (method === "proxies.list") callback([{ name: "AI", kind: "Selector" }], null)
      else if (method === "rules.test") callback({ probe: { url: params.host }, results: [] }, null)
      else if (method === "rules.save_many") callback({ rules: [], apply: { reloaded: true }, closed: 0 }, null)
      else if (callback) callback({}, null)
    }
  }
  Component { id: pageComponent; Plugin.ProblemsPage { anchors.fill: parent; anchors.margins: 20; svc: mockService; panel: mockPanel } }
  Component { id: editorComponent; Plugin.RouteEditor { anchors.fill: parent; anchors.margins: 20; svc: mockService; panel: mockPanel } }
  TestCase {
    name: "Problems"
    when: windowShown
    property var page
    function problem(host, target, rule, payload, count, recovered) {
      return { host: host, is_ip: /^[0-9.]+$/.test(host), route: { target: target, rule: rule, payload: payload }, count: count, kinds: recovered ? { reset: 1, timeout: 2 } : { timeout: count }, apps: { Telegram: 1 }, new: !recovered, ok_since_error: recovered ? 15 : 0, ports: [443], first_seen: 1, last_seen: 2 }
    }
    function init() {
      mockService.data = [problem("142.252.58.41", "DIRECT", "Match", "", 38, false), problem("8.6.112.0", "AI", "DomainSuffix", "chatgpt.com", 3, true), problem("142.252.58.48", "DIRECT", "Match", "", 38, false), problem("8.47.69.0", "AI", "DomainSuffix", "chatgpt.com", 3, true), problem("api.example.com", "AI", "DomainSuffix", "example.com", 2, false)]
      mockService.data[1].domain_hint = "chatgpt.com"
      mockService.data[3].domain_hint = "chatgpt.com"
      mockService.calls = []; mockService.pending = []; mockService.defer = false
      mockPanel.problemsView = "table"; mockPanel.editing = null
      page = createTemporaryObject(pageComponent, scene)
      verify(page)
      page.tab = "all"
      wait(30)
    }
    function test_group_identity_totals_and_scopes() {
      var groups = Models.groups(mockService.data)
      compare(groups.length, 3)
      compare(groups[0].items.length, 2)
      compare(groups[0].count, 76)
      compare(groups[0].kinds.timeout, 76)
      compare(groups[1].unresolved, false)
      compare(Models.groupScopes(groups[0]).length, 2)
      compare(Models.groupScopes(groups[0])[0].kind, "ip-cidr")
      compare(Models.groupScopes(groups[1]), [{ kind: "domain-suffix", value: "chatgpt.com" }])
    }
    function test_switch_collapse_navigation_actions() {
      page.select(2)
      mockPanel.problemsView = "groups"
      compare(page.rows[page.cursor].problem.host, "142.252.58.48")
      page.move(0, 1)
      verify(page.rows[page.cursor].group)
      page.activate()
      compare(mockPanel.editing.group.route.payload, "chatgpt.com")
      page.textKey("g")
      compare(page.rows[page.cursor].closed, true)
      page.move(0, -1)
      page.activate()
      compare(mockPanel.editing.problem.host, "142.252.58.48")
      page.remove()
      verify(mockService.calls.some(function(c) { return c.method === "problems.hide" && c.params.value === "142.252.58.48" }))
      mockPanel.problemsView = "table"
      compare(page.rows[page.cursor].problem.host, "142.252.58.48")
    }
    function test_stale_responses_do_not_replace_search() {
      mockService.defer = true
      page.search = "142"; page.reload()
      page.search = "api"; page.reload()
      mockService.pending[1]()
      compare(page.items.length, 1)
      mockService.pending[0]()
      compare(page.items[0].host, "api.example.com")
    }
    function test_group_editor_saves_all_resources() {
      page.openGroup(Models.groups(mockService.data)[0])
      page.destroy()
      var editor = createTemporaryObject(editorComponent, scene)
      verify(editor)
      compare(editor.groupRules.length, 2)
      compare(editor.testHost(), "142.252.58.41")
      editor.target = "AI"
      wait(40)
      grabImage(scene).save("/tmp/omm-problems-group-editor.png")
      editor.save()
      var calls = mockService.calls.filter(function(c) { return c.method === "rules.save_many" })
      compare(calls.length, 1)
      compare(calls[0].params.rules.length, 2)
      verify(!mockService.calls.some(function(c) { return c.method === "rules.save" }))
    }
    function textItem(item, text) {
      if (item.text === text) return item
      var children = item.children || []
      for (var i = 0; i < children.length; ++i) {
        var found = textItem(children[i], text)
        if (found) return found
      }
      return null
    }
    function test_mouse_controls_and_empty_filter() {
      var toggle = textItem(page, "По маршрутам")
      verify(toggle)
      mouseClick(toggle)
      compare(page.viewMode, "groups")
      wait(30)
      var route = textItem(page, "Маршрут")
      verify(route)
      mouseClick(route)
      verify(mockPanel.editing.group)
      compare(mockPanel.editing.group.items.length, 2)
      var heading = textItem(page, "Match → DIRECT")
      mouseClick(heading)
      verify(page.rows[0].closed)
      page.kind = "dns_fail"
      compare(page.rows.length, 0)
      page.move(0, 1); page.activate(); page.remove()
      page.kind = ""
      verify(page.rows.length > 0)
    }
    function test_visuals() {
      wait(80)
      grabImage(scene).save("/tmp/omm-problems-table.png")
      mockPanel.problemsView = "groups"
      wait(80)
      grabImage(scene).save("/tmp/omm-problems-groups.png")
      page.toggleGroup(Models.groups(mockService.data)[1])
      wait(40)
      grabImage(scene).save("/tmp/omm-problems-collapsed.png")
    }
  }
}
