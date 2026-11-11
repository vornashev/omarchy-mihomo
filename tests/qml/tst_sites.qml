import QtQuick
import QtTest
import "../../plugin" as Plugin
import "../../plugin/SiteModels.js" as SiteModels

Rectangle {
  id: scene
  width: 900; height: 640
  color: "#1e1e2e"
  QtObject {
    id: service
    property color red: "#f38ba8"
    property color green: "#a6e3a1"
    property color yellow: "#f9e2af"
    property color peach: "#fab387"
    property var calls: []
    property var sites: [
      { id: "a", name: "YouTube", url: "https://www.youtube.com/" },
      { id: "b", name: "GitHub", url: "https://github.com/", icon: "letter" },
      { id: "c", name: "Ya", url: "https://ya.ru/" }
    ]
    // sites.test replies are held until `flush()`, to see the testing state.
    property var held: []
    function ago(ms) { return "только что" }
    function snapshot() { return { sites: sites, results: {} } }
    function flush() { var h = held; held = []; h.forEach(function(f) { f() }) }
    function request(m, params, cb) {
      calls = calls.concat([{ method: m, params: params }])
      if (!cb) return
      if (m === "sites.test") held = held.concat([function() { cb({ id: params.id, ok: true, delay: 42, chain: ["Europe", "NL-02"], rule: "DomainSuffix,youtube.com", at: 1 }, null) }])
      else if (m === "sites.icon") cb({ file: null }, null)
      else if (m === "sites.remove") { sites = sites.filter(function(s) { return s.id !== params.id }); cb(snapshot(), null) }
      else if (m === "sites.order") { var by = {}; sites.forEach(function(s) { by[s.id] = s }); sites = params.ids.map(function(i) { return by[i] }); cb(snapshot(), null) }
      else if (m === "sites.save") { sites = sites.concat([{ id: "n", name: params.name || "example.org", url: "https://" + params.url + "/" }]); var r = snapshot(); r.saved = "n"; cb(r, null) }
      else cb(snapshot(), null)
    }
  }
  QtObject {
    id: panel
    property string notification: ""
    function notify(t) { notification = t }
    function forceKeys() {}
  }
  Plugin.SitesPage {
    id: page
    anchors.fill: parent
    anchors.margins: 20
    svc: service
    panel: panel
  }
  TestCase {
    name: "Sites"
    when: windowShown

    function methods() { return service.calls.map(function(c) { return c.method }) }
    function last(m) { var l = service.calls.filter(function(c) { return c.method === m }); return l[l.length - 1] }
    function cards() {
      var grid = findChild(page, "sitesGrid")
      var out = []
      for (var i = 0; i < grid.contentItem.children.length; i++) {
        var c = findChild(grid.contentItem.children[i], "siteCard")
        if (c) out.push(c)
      }
      return out.sort(function(a, b) { return a.parent.y - b.parent.y || a.parent.x - b.parent.x })
    }

    function init() {
      Plugin.MihomoI18n.language = "ru"
      service.sites = [
        { id: "a", name: "YouTube", url: "https://www.youtube.com/" },
        { id: "b", name: "GitHub", url: "https://github.com/", icon: "letter" },
        { id: "c", name: "Ya", url: "https://ya.ru/" }
      ]
      service.held = []
      page.sort = "custom"
      page.cursor = 0
      page.results = ({})
      page.testing = ({})
      page.reload()
      service.calls = []
      waitForRendering(page)
    }

    function test_models() {
      compare(SiteModels.state(null, false), "idle")
      compare(SiteModels.state({ ok: true, delay: 40 }, false), "good")
      compare(SiteModels.state({ ok: true, delay: 900 }, false), "slow")
      compare(SiteModels.state({ ok: false, error: "timeout" }, false), "fail")
      compare(SiteModels.state({ ok: true, delay: 40 }, true), "testing")
      compare(SiteModels.shortHost("https://www.youtube.com/watch?v=1"), "youtube.com")
      compare(SiteModels.defaultName("ya.ru"), "ya.ru")
      compare(SiteModels.route({ chain: ["Europe", "Auto", "NL-02"] }), "Europe · NL-02")
      compare(SiteModels.route({ chain: ["DIRECT"] }), "DIRECT")
      var sites = [{ id: "a", name: "b" }, { id: "b", name: "A" }, { id: "c", name: "c" }, { id: "d", name: "d" }]
      var res = { a: { ok: true, delay: 300 }, b: { ok: false }, c: { ok: true, delay: 20 } }
      compare(SiteModels.sorted(sites, res, "delay").map(function(s) { return s.id }), ["c", "a", "b", "d"])
      compare(SiteModels.sorted(sites, res, "name").map(function(s) { return s.id }), ["b", "a", "c", "d"])
      compare(SiteModels.sorted(sites, res, "custom").map(function(s) { return s.id }), ["a", "b", "c", "d"])
      compare(SiteModels.moved(sites, "a", 2), ["b", "c", "a", "d"])
      compare(SiteModels.moved(sites, "a", -5), ["a", "b", "c", "d"])
      compare(SiteModels.counts(sites, res), { ok: 1, slow: 1, fail: 1 })
      compare(SiteModels.iconValue("url", " https://x/i.png "), "https://x/i.png")
      compare(SiteModels.iconValue("url", ""), null)
      compare(SiteModels.iconMode("letter"), "letter")
    }

    function test_click_checks_one_site() {
      var c = cards()
      compare(c.length, 3)
      mouseClick(c[0])
      compare(last("sites.test").params, { id: "a" })
      compare(findChild(c[0], "siteLatency").text, "проверка…")
      service.flush()
      waitForRendering(page)
      compare(findChild(c[0], "siteLatency").text, "42 мс")
      compare(findChild(c[0], "siteRoute").text, "Europe · NL-02")
      compare(findChild(c[1], "siteLatency").text, "—")
    }

    function test_check_all() {
      mouseClick(findChild(page, "sitesCheckAll"))
      compare(methods().filter(function(m) { return m === "sites.test" }).length, 3)
      verify(page.busy)
      service.flush()
      verify(!page.busy)
      compare(Object.keys(page.results).length, 3)
    }

    function test_keys_delete_and_reorder() {
      page.move(1, 0)
      compare(page.current.id, "b")
      page.textKey("]")
      compare(last("sites.order").params.ids, ["a", "c", "b"])
      compare(page.current.id, "b")
      page.remove()
      compare(last("sites.remove").params, { id: "b" })
      compare(page.sites.length, 2)
      // Sorted views do not reorder.
      page.sort = "name"
      service.calls = []
      page.textKey("[")
      compare(methods().indexOf("sites.order"), -1)
    }

    function test_add_site() {
      page.textKey("n")
      var url = findChild(page, "siteUrlField")
      tryVerify(function() { return url.visible })
      url.text = "example.org"
      mouseClick(findChild(page, "siteSave"))
      compare(last("sites.save").params, { id: null, url: "example.org", name: "", icon: null })
      tryVerify(function() { return !url.visible })
      compare(page.current.id, "n")
      compare(last("sites.test").params, { id: "n" })
    }

    function test_empty_list() {
      service.sites = []
      page.reload()
      waitForRendering(page)
      verify(findChild(page, "sitesEmpty").visible)
      mouseClick(findChild(page, "sitesDefaults"))
      compare(last("sites.defaults").method, "sites.defaults")
    }
  }
}
