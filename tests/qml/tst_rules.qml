import QtQuick
import QtQuick.Controls
import QtTest
import "../../plugin" as Plugin
import "../../plugin/RuleModels.js" as RuleModels

Rectangle {
  id: scene
  width: 636; height: 660
  color: "#1e1e2e"
  QtObject {
    id: mockPanel
    property var editing: null
    property string page: "rules"
    property string toast: ""
    property string logFilter: ""
    property var opened: null
    function openRule(spec) { opened = spec; editing = spec; page = "rule" }
    function openEditor(value) { editing = value }
    function showPage(value) { page = value }
    function showLogs(f) { logFilter = f; page = "logs" }
    function notify(value) { toast = value }
    function forceKeys() {}
  }
  QtObject {
    id: mockService
    property var calls: []
    property color red: "#f38ba8"
    property color green: "#a6e3a1"
    property color yellow: "#f9e2af"
    property color mauve: "#cba6f7"
    property var connections: []
    signal statusChanged()
    function ago(ms) { return "5 мин назад" }
    function clock(ms) { return "18:40" }
    function formatBytes(n) { return n + " Б" }
    function last(method) { var c = calls.filter(function(x) { return x.method === method }); return c.length ? c[c.length - 1].params : null }
    function request(method, params, callback) {
      calls.push({ method: method, params: params })
      var r = {}
      if (method === "rules.scopes") r = [
        { kind: "domain", value: "api.anthropic.com", connections: 1, seen: 0, unknown: false, routes: [] },
        { kind: "domain-suffix", value: "anthropic.com", connections: 5, seen: 14, unknown: false, routes: [] },
        { kind: "process-name", value: "claude", connections: 7, seen: 0, unknown: false, routes: [] },
        { kind: "ip-cidr", value: "160.79.104.10/32", connections: 1, seen: 0, unknown: false, routes: [] }]
      else if (method === "proxies.list") r = [{ name: "AI", kind: "Selector", now: "FI" }, { name: "PROXY", kind: "Fallback", now: "NL" }, { name: "Стриминг", kind: "Selector", now: "WARP" }, { name: "GLOBAL", kind: "Selector" }]
      else if (method === "rules.test") r = { probe: { url: "https://" + params.host + "/" }, results: params.targets.map(function(t) { return { target: t, ok: t !== "Стриминг", delay: 200, error: t === "Стриминг" ? "timeout" : null } }) }
      else if (method === "rules.preview") r = { value: params.kind === "domain-suffix" ? String(params.value).replace(/^\*\./, "") : params.value, line: RuleModels.keyword(params.kind, params.value) + "," + params.value + "," + params.target, existing: null, coverage: { connections: 5, seen: 14, unknown: false, routes: [{ rule: "DomainSuffix", payload: "anthropic.com", target: "AI", count: 5 }] } }
      else if (method === "rules.suggest") r = { items: params.kind.indexOf("process") === 0
        ? [{ value: "telegram-desktop", source: "conn", detail: "/usr/bin/telegram-desktop · «Telegram»", count: 4, route: "TELEGRAM" }, { value: "telegram-cli", source: "log", count: 3, last: 1 }]
        : [{ value: "anthropic.com", source: "common", detail: "api.anthropic.com · console.anthropic.com", count: 3 }, { value: "api.anthropic.com", source: "conn", detail: "claude", count: 3, route: "AI → NY" }, { value: "console.anthropic.com", source: "log", count: 2, last: 1 }] }
      else if (method === "rules.save") r = { rule: { id: 9, kind: params.kind, value: params.value, target: params.target }, apply: { reloaded: true }, closed: 2 }
      else if (method === "connections.list") r = { connections: connections }
      else if (method === "rules.list") r = { items: [{ id: 3, kind: "domain-suffix", value: "x.com", target: "AI", enabled: true, source: "user", created: 1 }], orphans: [] }
      else if (method === "rules.history") r = []
      if (callback) callback(r, null)
    }
  }
  readonly property var conn: ({ id: "c1", metadata: { host: "api.anthropic.com", destinationIP: "160.79.104.10", destinationPort: "443", network: "tcp", process: "claude", processPath: "/home/u/.local/bin/claude" }, chains: ["New York", "AI"], rule: "DomainSuffix", rulePayload: "anthropic.com", upload: 1, download: 2 })

  Component { id: editorComponent; Plugin.RuleEditor { anchors.fill: parent; anchors.margins: 18; svc: mockService; panel: mockPanel } }
  Component { id: connectionsComponent; Plugin.ConnectionsPage { anchors.fill: parent; anchors.margins: 18; svc: mockService; panel: mockPanel } }
  Component { id: rulesComponent; Plugin.RulesPage { anchors.fill: parent; anchors.margins: 18; svc: mockService; panel: mockPanel } }

  TestCase {
    name: "Rules"
    when: windowShown

    function init() {
      mockService.calls = []
      mockService.connections = [scene.conn]
      mockPanel.editing = null; mockPanel.opened = null; mockPanel.page = "rules"; mockPanel.toast = ""
    }
    function find(item, name) {
      if (!item) return null
      if (item.objectName === name) return item
      var kids = item.children || []
      for (var i = 0; i < kids.length; i++) { var f = find(kids[i], name); if (f) return f }
      if (item.contentItem && item.contentItem !== item) return find(item.contentItem, name)
      return null
    }
    function shot(item, name) {
      var img = grabImage(item)
      img.save(Qt.resolvedUrl("../../target/qml-shots/" + name + ".png").toString().replace("file://", ""))
    }

    function test_models() {
      compare(RuleModels.registrable("rr3---sn-4g5e6nsz.googlevideo.com"), "googlevideo.com")
      compare(RuleModels.registrable("news.bbc.co.uk"), "bbc.co.uk")
      compare(RuleModels.logicValue([{ kind: "process-name", value: "firefox" }, { kind: "domain-suffix", value: " youtube.com " }, { kind: "network", value: "" }]), "((PROCESS-NAME,firefox),(DOMAIN-SUFFIX,youtube.com))")
      var q = RuleModels.quickScopes(RuleModels.origin(scene.conn))
      compare(q.map(function(s) { return s.kind }), ["domain", "domain-suffix", "process-name", "ip-cidr"])
      compare(q[1].value, "anthropic.com")
      compare(RuleModels.quickScopes({ host: "149.154.167.51", ip: "", process: "" }).map(function(s) { return s.title }), ["149.154.167.51/32"])
      compare(RuleModels.quickDefault({ host: "", ip: "1.2.3.4", process: "x" }).kind, "ip-cidr")
      var t = [{ name: "DIRECT" }, { name: "A" }, { name: "B" }, { name: "C" }]
      compare(RuleModels.chips(t, "C", 3).visible.map(function(x) { return x.name }), ["DIRECT", "A", "C"])
      compare(RuleModels.chips(t, "C", 3).more.map(function(x) { return x.name }), ["B"])
      var rows = RuleModels.rows([{ value: "a", source: "log" }, { value: "b", source: "common" }, { value: "c", source: "log" }])
      compare(rows.map(function(r) { return r.header ? "#" + r.source : r.value }), ["#common", "b", "#log", "a", "c"])
      compare(RuleModels.tabOf("uid"), "process")
      compare(RuleModels.kindOfKeyword("ip-cidr6"), "ip-cidr")
      var life = RuleModels.lifetime("hour", 1000)
      compare(life.expires, 1000 + 3600 * 1000)
      compare(RuleModels.lifetime("boot", 1).until_reboot, true)
    }

    function test_editor_from_connection_defaults_and_saves() {
      mockPanel.editing = { from: "connections", origin: RuleModels.origin(scene.conn) }
      var e = createTemporaryObject(editorComponent, scene)
      verify(e)
      tryCompare(e, "scopeIndex", 1)
      compare(e.full, false)
      compare(e.effectiveKind, "domain-suffix")
      compare(e.effectiveValue, "anthropic.com")
      compare(e.target, "AI", "the group the connection uses now")
      compare(mockService.last("rules.scopes").process, "claude")
      tryVerify(function() { return e.preview !== null })
      verify(e.previewText().indexOf("5") !== -1)
      tryVerify(function() { return e.results["AI"] !== undefined }, 3000)
      e.move(1, 0)
      compare(e.target, "PROXY")
      e.move(0, 1)
      compare(e.effectiveKind, "process-name")
      e.move(0, -1)
      wait(250)
      shot(e, "rule-editor-connection")
      e.save()
      var p = mockService.last("rules.save")
      compare(p.kind, "domain-suffix")
      compare(p.value, "anthropic.com")
      compare(p.target, "PROXY")
      compare(p.reconnect, true)
      compare(p.lifetime.until_reboot, false)
      compare(p.replace, undefined)
      compare(mockPanel.page, "connections")
      verify(mockPanel.toast.indexOf("PROXY") !== -1)
    }

    function test_full_form_suggestions_keyboard() {
      mockPanel.editing = { from: "rules" }
      var e = createTemporaryObject(editorComponent, scene)
      verify(e.full)
      compare(e.kind, "domain-suffix")
      e.pickTab("process")
      compare(e.kind, "process-name")
      var vi = null
      ;(function walk(it) { if (vi) return; if (it.hasOwnProperty("kind") && it.hasOwnProperty("suggestOpen")) { vi = it; return } for (var i = 0; i < it.children.length; i++) walk(it.children[i]) })(e)
      verify(vi)
      vi.input.forceActiveFocus()
      tryVerify(function() { return vi.suggestOpen })
      compare(mockService.last("rules.suggest").kind, "process-name")
      keyClick("t"); keyClick("e")
      tryVerify(function() { return mockService.last("rules.suggest").query === "te" })
      compare(vi.ghost, "legram-desktop")
      wait(100)
      shot(e, "rule-editor-process-suggest")
      keyClick(Qt.Key_Down)
      compare(vi.current.value, "telegram-cli")
      keyClick(Qt.Key_Up)
      keyClick(Qt.Key_Return)
      compare(vi.text, "telegram-desktop")
      verify(!vi.suggestOpen)
      compare(e.effectiveValue, "telegram-desktop")
      vi.text = "tel"
      keyClick(Qt.Key_Tab)
      tryCompare(vi, "text", "telegram-desktop")
      e.lifetimeKey = "hour"
      tryVerify(function() { return e.preview !== null && e.canSave })
      e.save()
      var p = mockService.last("rules.save")
      compare(p.kind, "process-name")
      verify(p.lifetime.expires > Date.now())
      compare(mockPanel.page, "rules")
    }

    function test_domain_suggestions_render() {
      mockPanel.editing = { from: "rules" }
      var e = createTemporaryObject(editorComponent, scene)
      var vi = null
      ;(function walk(it) { if (vi) return; if (it.hasOwnProperty("kind") && it.hasOwnProperty("suggestOpen")) { vi = it; return } for (var i = 0; i < it.children.length; i++) walk(it.children[i]) })(e)
      vi.input.forceActiveFocus()
      keyClick("a"); keyClick("n"); keyClick("t"); keyClick("h"); keyClick("r")
      tryVerify(function() { return vi.suggestOpen && vi.rows.length === 6 })
      compare(vi.rows[0].header, true)
      compare(vi.rows[1].source, "common")
      wait(100)
      shot(e, "rule-editor-domain-suggest")
      keyClick(Qt.Key_Escape)
      verify(!vi.suggestOpen)
    }

    function test_logic_rule() {
      mockPanel.editing = { from: "rules" }
      var e = createTemporaryObject(editorComponent, scene)
      e.pickTab("logic")
      compare(e.kind, "and")
      e.setPart(0, "value", "firefox")
      e.setPart(1, "value", "youtube.com")
      e.addPart()
      e.setPart(2, "kind", "network")
      e.setPart(2, "value", "udp")
      compare(e.effectiveValue, "((PROCESS-NAME,firefox),(DOMAIN-SUFFIX,youtube.com),(NETWORK,udp))")
      e.removePart(2)
      compare(e.parts.length, 2)
      wait(250)
      shot(e, "rule-editor-logic")
      e.target = "Стриминг"
      tryVerify(function() { return e.canSave })
      e.save()
      compare(mockService.last("rules.save").value, "((PROCESS-NAME,firefox),(DOMAIN-SUFFIX,youtube.com))")
    }

    function test_editing_a_rule_replaces_it_when_the_match_changes() {
      mockPanel.editing = { from: "rules", rule: { id: 3, kind: "domain-suffix", value: "x.com", target: "AI", effective_target: "AI", enabled: true, note: "n", expires: 5000 } }
      var e = createTemporaryObject(editorComponent, scene)
      compare(e.lifetimeKey, "until")
      tryVerify(function() { return e.preview !== null })
      e.save()
      var p = mockService.last("rules.save")
      compare(p.replace, undefined, "same match: plain update")
      compare(p.lifetime.expires, 5000)
      compare(p.note, "n")
      e.kind = "domain"
      tryVerify(function() { return e.preview !== null && e.preview.value === "x.com" })
      e.save()
      compare(mockService.last("rules.save").replace, 3)
    }

    function test_connection_menu() {
      var page = createTemporaryObject(connectionsComponent, scene)
      tryCompare(page, "conns", [scene.conn])
      page.openMenu(scene.conn, null)
      var menu = findChild(page, "connectionMenu")
      verify(menu)
      tryVerify(function() { return menu.opened })
      verify(page.typing)
      var items = []
      for (var i = 0; i < menu.count; i++) { var it = menu.itemAt(i); if (it && it.objectName) items.push(it.objectName) }
      compare(items, ["quick_domain", "quick_domain-suffix", "quick_process-name", "quick_ip-cidr", "quick_custom"])
      wait(50)
      shot(menu.contentItem.parent || menu.contentItem, "connection-menu")
      for (var j = 0; j < menu.count; j++) if (menu.itemAt(j).objectName === "quick_process-name") menu.itemAt(j).triggered()
      compare(mockPanel.opened.from, "connections")
      compare(mockPanel.opened.kind, "process-name")
      compare(mockPanel.opened.value, "claude")
      compare(mockPanel.opened.origin.host, "api.anthropic.com")
      menu.close()
      page.textKey("r")
      compare(mockPanel.opened.kind, "domain-suffix")
      compare(mockPanel.opened.full, false)
      page.textKey("R")
      compare(mockPanel.opened.full, true)
    }

    function test_rules_page_add_and_duplicate() {
      var page = createTemporaryObject(rulesComponent, scene)
      tryCompare(page.rules, "length", 1)
      page.textKey("n")
      compare(mockPanel.opened.from, "rules")
      compare(mockPanel.opened.rule, undefined)
      page.textKey("d")
      compare(mockPanel.opened.duplicate, true)
      compare(mockPanel.opened.rule.id, 3)
      page.activate()
      compare(mockPanel.opened.rule.id, 3)
      compare(mockPanel.opened.duplicate, undefined)
    }
  }
}
