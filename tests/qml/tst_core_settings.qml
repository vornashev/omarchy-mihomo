import QtQuick
import QtTest
import "../../plugin" as Plugin

Rectangle {
  id: scene
  width: 760; height: 560
  color: "#1e1e2e"
  QtObject {
    id: service
    property var status: ({})
    property var fixing: ({})
    property color yellow: "#f9e2af"
    property var reply: null
    property var methods: []
    property var fixes: []
    function formatBytes(n) { return (n / 1048576).toFixed(1) + " МБ" }
    function request(m, params, cb) { methods = methods.concat([m]); if (cb) cb(reply, null) }
    function runFix(fix) { fixes = fixes.concat([fix]) }
  }
  QtObject {
    id: panel
    property string notification: ""
    function notify(text) { notification = text }
    function forceKeys() {}
  }
  Plugin.CoreSettings {
    id: core
    x: 20; y: 20; width: 720
    svc: service
    panel: panel
    settings: ({ "geo-auto-update": true, "geo-update-interval": 24 })
  }
  TestCase {
    name: "CoreSettings"
    when: windowShown

    function info(patch) {
      return Object.assign({ supported: true, current: "v1.19.32", latest: "v1.19.32", available: null, checked_at: 1761500000000,
        error: null, checking: false, fix: null, previous: null, geo: [], geo_auto_update: true, geo_update_interval: 24, geo_updating: false }, patch)
    }
    function load(reply) {
      service.reply = reply
      core.reload()
      waitForRendering(core)
    }
    function init() {
      Plugin.MihomoI18n.language = "ru"
      service.methods = []; service.fixes = []
      panel.notification = ""
    }

    function test_up_to_date_offers_only_a_check() {
      load(info({ geo: [{ name: "GeoSite.dat", size: 5242880, modified: 1761500000000 }] }))
      verify(findChild(core, "coreVersionLine").text.indexOf("последняя версия") !== -1)
      verify(!findChild(core, "coreUpdateBtn").visible)
      verify(!findChild(core, "coreRollbackBtn").visible)
      verify(findChild(core, "coreCheckBtn").visible)
      mouseClick(findChild(core, "coreCheckBtn"))
      compare(service.methods[service.methods.length - 1], "core.check_updates")
      grabImage(scene).save("/tmp/omm-core-settings-current.png")
    }

    function test_available_update_runs_the_fix_and_rollback_is_offered() {
      var fix = { id: "update_core", privileged: true, confirm: false }
      load(info({ latest: "v1.20.0", available: "v1.20.0", fix: fix, previous: "v1.19.31" }))
      var update = findChild(core, "coreUpdateBtn")
      verify(update.visible)
      compare(update.text, "Обновить до v1.20.0")
      mouseClick(update)
      compare(service.fixes.length, 1)
      compare(service.fixes[0].id, "update_core")
      var rollback = findChild(core, "coreRollbackBtn")
      verify(rollback.visible)
      compare(rollback.text, "Откатить на v1.19.31")
      mouseClick(rollback)
      compare(service.methods[service.methods.length - 1], "core.rollback")
      grabImage(scene).save("/tmp/omm-core-settings-update.png")
    }

    function test_foreign_core_hides_actions() {
      load(info({ supported: false }))
      verify(!findChild(core, "coreCheckBtn").visible)
      verify(!findChild(core, "geoNowBtn").visible)
    }

    function test_geo_now_and_auto_switch() {
      load(info({}))
      var saved = null
      core.save.connect(function(p) { saved = p })
      mouseClick(findChild(core, "geoAutoSwitch"))
      compare(saved["geo-auto-update"], false)
      mouseClick(findChild(core, "geoNowBtn"))
      compare(service.methods[service.methods.length - 1], "core.update_geo")
      compare(panel.notification, "Базы обновлены, ядро перечитало конфиг")
    }
  }
}
