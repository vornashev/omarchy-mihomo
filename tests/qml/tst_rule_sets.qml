import QtQuick
import QtTest
import "../../plugin" as Plugin

Rectangle {
  id: scene
  width: 760; height: 420
  color: "#1e1e2e"
  QtObject {
    id: service
    property var calls: []
    property var error: null
    readonly property var listing: ({
      mine: [{ name: "geosite-openai", url: "https://github.com/MetaCubeX/meta-rules-dat/raw/meta/geo/geosite/openai.mrs", behavior: "domain", format: "mrs", "interval-hours": 24,
        runtime: { rule_count: 187, updated_at: "2026-10-27T09:12:00Z", behavior: "domain" } }],
      profile: [{ name: "ru-blocked", behavior: "domain", rule_count: 51234, updated_at: "2026-10-14T21:48:00Z", vehicle: "HTTP" }]
    })
    function request(m, params, cb) {
      calls = calls.concat([{ method: m, params: params }])
      if (error && m !== "rule_sets.list") { cb(null, { message: error }); return }
      if (m === "rule_sets.add") cb({ name: params.catalog ? params.catalog.replace(":", "-") : params.name, list: listing }, null)
      else cb(listing, null)
    }
  }
  QtObject {
    id: panel
    property string notification: ""
    function notify(t) { notification = t }
  }
  Plugin.RuleSetsPanel {
    id: sets
    x: 20; y: 20; width: 720; height: 380
    svc: service
    panel: panel
  }
  TestCase {
    name: "RuleSets"
    when: windowShown
    function init() {
      Plugin.I18n.language = "ru"
      service.calls = []; service.error = null; panel.notification = ""
      sets.reload()
      waitForRendering(sets)
    }
    function last() { return service.calls[service.calls.length - 1] }

    function test_catalog_and_url_adds() {
      var input = findChild(sets, "ruleSetsInput")
      input.text = "geosite:youtube"
      sets.add()
      compare(last().params, { catalog: "geosite:youtube" })
      compare(panel.notification, "Набор geosite-youtube добавлен")
      input.text = "https://example.org/lists/ai.yaml"
      sets.add()
      compare(last().params.name, "ai")
      compare(last().params.format, "yaml")
      compare(last().params.behavior, "domain")
      grabImage(scene).save("/tmp/omm-rule-sets.png")
    }

    function test_remove_reports_rules_in_use() {
      service.error = "rules still use geosite-openai: RULE-SET,geosite-openai,AI"
      var remove = findChild(sets, "ruleSetRemove0")
      verify(remove, "remove button of the first set")
      mouseClick(remove)
      compare(last().method, "rule_sets.remove")
      compare(panel.notification, "rules still use geosite-openai: RULE-SET,geosite-openai,AI")
    }
  }
}
