import QtQuick
import QtTest
import "../../plugin" as Plugin
import "../../plugin/SearchModels.js" as SM

// The compact window and the large window's title bar and footer.
Rectangle {
  id: scene
  width: 2520; height: 1200
  color: "#1e1e2e"
  QtObject {
    id: service
    signal systemProxyFailed(string message)
    property bool connected: true
    property bool coreRunning: true
    property string mode: "rule"
    property bool tun: true
    property int newProblems: 12
    property var traffic: ({ down: 6553, up: 1024 })
    property var status: ({ daemon_version: "0.2.0", core: { version: "v1.19.33", mixed_port: 7897, tun_stack: "gVisor" } })
    property var issues: []
    property string configError: ""
    property var problemCounts: ({ visible: 487 })
    property bool systemProxy: true
    property bool systemProxyWanted: true
    property bool systemProxyAvailable: true
    property bool systemProxyBusy: false
    property string systemProxyError: ""
    property color red: "#f38ba8"
    property color green: "#a6e3a1"
    property color yellow: "#f9e2af"
    property color peach: "#fab387"
    property color mauve: "#cba6f7"
    property var calls: []
    function kindColor(k) { return red }
    function formatBytes(n) { return n + " B" }
    function formatRate(n) { return n + " B/s" }
    function ago(ms) { return "10 d" }
    function clock(ms) { return "14.10 22:54" }
    function toggleSystemProxy() {}
    function request(m, params, cb) {
      calls = calls.concat([{ method: m, params: params }])
      if (!cb) return
      if (m === "problems.list") cb({ items: [
        { host: "chatgpt.com", kinds: { other: 3, refused: 1 }, route: { rule: "DomainSuffix", target: "AI" }, count: 1437, "new": true },
        { host: "openrouter.ai", kinds: { reset: 2 }, route: { rule: "DomainSuffix", target: "AI" } }
      ] }, null)
      else if (m === "proxies.list") cb([
        { name: "GLOBAL", kind: "Selector", now: "DIRECT", members: [] },
        { name: "PROXY", kind: "Selector", now: "Amsterdam-02", members: [{ name: "Amsterdam-02", delay: 42 }, { name: "Paris", delay: 0 }] },
        { name: "AUTO", kind: "URLTest", now: "Paris", members: [] },
        { name: "AI", kind: "Selector", now: "Frankfurt-01", members: [{ name: "Frankfurt-01", delay: 61 }] }
      ], null)
      else if (m === "traffic.query") cb({ from: Date.now() - 3600000, step: 60000, total: { down: 347, up: 19, conns: 1204 },
        all: { up: [1, 3, 2, 5, 1], down: [40, 90, 60, 120, 80] },
        rows: [{ key: "zen-bin", label: "zen-bin", up: 10, down: 140 }, { key: "Telegram", label: "Telegram", up: 30, down: 90 }] }, null)
      else if (m === "rules.list") cb({ items: [{ id: 1, kind: "DOMAIN-SUFFIX", value: "youtube.com", target: "PROXY", hits: 12 }] }, null)
      else if (m === "profiles.list") cb({ current: "a", items: [{ id: "a", name: "Config.yml", updated: 0, source: { type: "local" } }] }, null)
      else if (m === "connections.list") cb({ connections: [
        { id: "1", metadata: { host: "neartoken.cc", destinationPort: "443", process: "node" }, chains: ["FI", "AI"], rule: "RuleSet", rulePayload: "cloudflare" },
        { id: "2", metadata: { host: "neartoken.cc", destinationPort: "443", process: "node" }, chains: ["FI", "AI"], rule: "RuleSet", rulePayload: "cloudflare" },
        { id: "3", metadata: { host: "", destinationIP: "1.1.1.1", destinationPort: "53", process: "" }, chains: ["DIRECT"], rule: "Match", rulePayload: "" }
      ] }, null)
      else cb(null, null)
    }
  }
  QtObject {
    id: panel
    property bool large: true
    property var pages: [{ key: "overview", label: "Обзор", icon: "" }, { key: "proxies", label: "Прокси", icon: "" }]
    property var shown: []
    property int fulls: 0
    property var edited: null
    property var ruled: null
    property string connectionsFilter: ""
    property string notification: ""
    function showPage(p) { shown = shown.concat([p]) }
    function openFull() { fulls++ }
    function openEditor(t) { edited = t }
    function openRule(s) { ruled = s }
    function showConnections(q) { connectionsFilter = q; showPage("connections") }
    function notify(t) { notification = t }
    function forceKeys() {}
  }

  Plugin.CompactPage {
    id: compact
    x: 15; y: 15
    width: 370
    svc: service
    panel: panel
  }
  Plugin.WindowTitleBar {
    id: titleBar
    x: 420; y: 0
    width: 1408
    height: implicitHeight
    svc: service
    panel: panel
  }
  Plugin.OverviewPage {
    id: overview
    x: 1100; y: 60
    width: 1360; height: 1100
    svc: service
    panel: panel
  }
  Plugin.WindowFooter {
    id: footer
    x: 420; y: 700
    width: 640
    height: implicitHeight
    svc: service
  }

  TestCase {
    name: "Windows"
    when: windowShown

    function init() {
      Plugin.MihomoI18n.language = "ru"
      panel.shown = []; panel.fulls = 0; panel.edited = null; panel.ruled = null; panel.connectionsFilter = ""
      service.calls = []
    }

    function test_search_models() {
      var pages = [{ key: "rules", label: "Правила" }, { key: "proxies", label: "Прокси" }]
      var hosts = [{ host: "youtube.com", route: { rule: "DomainSuffix", target: "PROXY" } }]
      var rules = [{ kind: "DOMAIN-SUFFIX", value: "youtube.com", target: "PROXY" }, { kind: "DOMAIN", value: "a.ru", target: "DIRECT" }]
      compare(SM.results("", pages, hosts, [], rules).length, 0)
      var r = SM.results("YouTube", pages, hosts, [], rules)
      compare(r.length, 2)
      compare(r[0].kind, "host")
      compare(r[0].detail, "DomainSuffix → PROXY")
      compare(r[1].kind, "rule")
      compare(SM.results("прав", pages, hosts, [], rules)[0].page, "rules")
      compare(SM.results("direct", pages, hosts, [], rules)[0].label, "a.ru", "rules match by target too")
      compare(SM.results("o", pages, hosts, [], rules, 1).length, 1)

      var conn = function(host, process, chains) {
        return { metadata: { host: host, sniffHost: "", destinationIP: "1.2.3.4", process: process }, chains: chains, rule: "RuleSet", rulePayload: "cf" }
      }
      var conns = [conn("near.cc", "node", ["FI", "PROXY"]), conn("near.cc", "node", ["FI", "PROXY"]), conn("b.cc", "zen", ["DIRECT"])]
      r = SM.results("near", pages, hosts, conns, rules)
      compare(r.length, 1, "connections to one host are one row")
      compare(r[0].kind, "conn")
      compare(r[0].query, "near.cc")
      compare(r[0].detail, "PROXY → FI · RuleSet(cf) · ×2")
      compare(SM.results("zen", pages, hosts, conns, rules)[0].label, "b.cc", "by process")
      compare(SM.results("fi", pages, hosts, conns, rules)[0].label, "near.cc", "by chain")
      var many = ["a", "b", "c", "d"].map(function(h) { return conn(h + ".x", "", ["DIRECT"]) })
      compare(SM.results(".x", [], [], many, []).length, 3, "at most three hosts")
    }

    function test_compact_shows_selector_groups_and_problems() {
      waitForRendering(compact)
      var proxies = findChild(compact, "compactProxies")
      verify(proxies.visible)
      compare(compact.groups.map(function(g) { return g.name }).join(","), "PROXY,AI", "selectors only, without GLOBAL")
      verify(findChild(compact, "compactProblems").visible)
      compare(compact.recent.length, 2)
      verify(compact.implicitHeight < 760)
      grabImage(scene).save("/tmp/omm-compact.png")
    }

    function test_compact_actions() {
      mouseClick(findChild(compact, "compactOpenFull"))
      compare(panel.fulls, 1)
      compact.activate()
      compare(panel.fulls, 2)
      mouseClick(findChild(compact, "compactSettings"))
      compare(panel.shown[0], "settings")
      var seg = findChild(compact, "compactMode")
      mouseClick(seg, seg.width - 20, seg.height / 2)
      compare(service.calls[service.calls.length - 1].method, "mode.set")
      compare(service.calls[service.calls.length - 1].params.mode, "direct")
    }

    function test_title_search_opens_a_section_or_rule() {
      titleBar.focusSearch()
      verify(titleBar.typing)
      keyClick("p"); keyClick("r"); keyClick("o")
      compare(titleBar.results.length, 2, "the section and the rule to PROXY")
      compare(titleBar.results[0].page, "proxies")
      verify(findChild(titleBar, "titleResults").visible)
      keyClick(Qt.Key_Return)
      compare(panel.shown[0], "proxies")
      verify(!titleBar.typing)

      titleBar.focusSearch()
      keyClick("y"); keyClick("o"); keyClick("u")
      wait(300)
      compare(titleBar.results.length, 1, "hosts from the daemon that do not match stay out")
      compare(titleBar.results[0].kind, "rule")
      keyClick(Qt.Key_Return)
      compare(panel.ruled.rule.value, "youtube.com")

      titleBar.focusSearch()
      keyClick("n"); keyClick("e"); keyClick("a"); keyClick("r")
      wait(300)
      compare(titleBar.results.length, 1)
      compare(titleBar.results[0].kind, "conn")
      keyClick(Qt.Key_Return)
      compare(panel.shown[panel.shown.length - 1], "connections")
      compare(panel.connectionsFilter, "neartoken.cc")
    }

    function test_large_overview_is_a_dashboard() {
      var dash = findChild(overview, "overviewChart")
      verify(dash && dash.visible, "the large window gets the dashboard")
      tryVerify(function() { return findChild(overview, "overviewApps") !== null && overview.rules !== null })
      compare(overview.rules.count, 1)
      compare(overview.rules.hits, 12)
      compare(overview.groups.map(function(g) { return g.name }).join(","), "PROXY,AI")
      verify(findChild(overview, "overviewSubtitle").text.indexOf("Config.yml") > 0)
      var problems = findChild(overview, "overviewProblems")
      verify(problems.y + problems.height <= overview.height + 1, "fits the window")
      waitForRendering(overview)
      grabImage(scene).save("/tmp/omm-overview.png")
    }

    function test_title_apply_and_footer_version() {
      mouseClick(findChild(titleBar, "titleApply"))
      compare(service.calls[service.calls.length - 1].method, "config.apply")
      compare(findChild(footer, "footerVersion").text, "omarchy-mihomo 0.2.0")
    }
  }
}
