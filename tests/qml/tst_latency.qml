import QtQuick
import QtTest
import "../../plugin" as Plugin
import "../../plugin/ProxyModels.js" as Models

Rectangle {
  id: scene
  width: 560; height: 160
  color: "#1e1e2e"
  QtObject {
    id: service
    property color red: "#f38ba8"
    property color green: "#a6e3a1"
    property color yellow: "#f9e2af"
  }
  Plugin.ProxyGroupCard {
    id: card
    x: 20; y: 20; width: 520
    entry: ({ name: "AI", type: "select", members: ["FI", "KZ"] })
    runtime: ({ now: "FI", members: [{ name: "FI", delay: 43 }, { name: "KZ", delay: 0 }] })
    svc: service
    latency: ({
      FI: { tests: 288, failures: 3, avg: 45, points: [40, 42, null, 61, 44, 47, 0, 45] },
      KZ: { tests: 40, failures: 40, avg: null, points: [0, 0, 0, 0] }
    })
  }
  TestCase {
    name: "Latency"
    when: windowShown

    function test_hint_wording() {
      Plugin.I18n.language = "ru"
      compare(Models.latencyHint({ tests: 288, failures: 3, avg: 45 }, Plugin.I18n.t), "24 ч: в среднем 45 мс · потери 1% · 288 проверок")
      compare(Models.latencyHint({ tests: 40, failures: 40, avg: null }, Plugin.I18n.t), "24 ч: все проверки неудачны · 40 проверок")
      compare(Models.latencyHint(null, Plugin.I18n.t), "")
      compare(Models.latencyLoss({ tests: 4, failures: 4 }), -1, "too few tests to judge")
      compare(Models.latencyLoss({ tests: 20, failures: 5 }), 25)
    }

    function test_card_shows_history_per_member() {
      Plugin.I18n.language = "ru"
      waitForRendering(card)
      var sparks = []
      function walk(item) {
        if (item.objectName === "memberSpark") sparks.push(item)
        for (var i = 0; i < (item.children || []).length; i++) walk(item.children[i])
      }
      walk(card)
      compare(sparks.length, 2)
      verify(sparks[0].visible && sparks[1].visible)
      compare(sparks[0].points.length, 8)
      grabImage(scene).save("/tmp/omm-latency-card.png")
    }
  }
}
