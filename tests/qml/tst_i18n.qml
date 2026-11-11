import QtQuick
import QtTest
import Qt.labs.folderlistmodel
import qs.Commons as Commons
import "../../plugin" as Plugin
import "../../plugin/i18n/ru.js" as Ru
import "../../plugin/i18n/en.js" as En

// Both languages carry the same keys with the same placeholders, no key is
// defined twice (a later one would silently win), and every literal key
// the plugin asks for exists.
Item {
  FolderListModel {
    id: sources
    folder: Qt.resolvedUrl("../../plugin")
    nameFilters: ["*.qml", "*.js"]
    showDirs: false
  }

  TestCase {
    name: "I18n"

    function read(url) {
      var xhr = new XMLHttpRequest()
      xhr.open("GET", url, false)
      xhr.send()
      return xhr.responseText
    }
    function placeholders(s) {
      var found = (String(s).match(/%[0-9]/g) || []).filter(function(v, i, a) { return a.indexOf(v) === i })
      return found.sort().join(",")
    }

    function test_same_keys_and_placeholders() {
      var ru = Object.keys(Ru.strings), en = Object.keys(En.strings)
      compare(ru.filter(function(k) { return !(k in En.strings) }), [], "keys only in Russian")
      compare(en.filter(function(k) { return !(k in Ru.strings) }), [], "keys only in English")
      var bad = ru.filter(function(k) { return k in En.strings && placeholders(Ru.strings[k]) !== placeholders(En.strings[k]) })
      compare(bad, [], "placeholders differ")
    }

    function test_no_key_defined_twice() {
      var files = ["ru.js", "en.js"]
      for (var f = 0; f < files.length; f++) {
        var src = read(Qt.resolvedUrl("../../plugin/i18n/" + files[f]))
        src = src.split("\n").filter(function(l) { return l.trim().indexOf("//") !== 0 }).join("\n")
        var keys = []
        var quoted = /"((?:[^"\\]|\\.)*)"\s*:/g, m
        while ((m = quoted.exec(src)) !== null) keys.push(m[1])
        var bare = src.replace(/"(?:[^"\\]|\\.)*"/g, "\"\"")
        var plain = /(?:^|[\s,{])([A-Za-z_][A-Za-z0-9_]*)\s*:/g
        while ((m = plain.exec(bare)) !== null) keys.push(m[1])
        var seen = {}, twice = []
        keys.forEach(function(k) { if (seen[k]) twice.push(k); seen[k] = true })
        compare(twice, [], files[f] + ": keys defined twice")
      }
    }

    function test_every_used_key_exists() {
      tryVerify(function() { return sources.status === FolderListModel.Ready && sources.count > 0 }, 5000)
      var missing = []
      var used = /MihomoI18n\.t\("([^"]+)"\)/g
      for (var i = 0; i < sources.count; i++) {
        var name = sources.get(i, "fileName")
        var src = read(sources.get(i, "fileUrl"))
        var m
        while ((m = used.exec(src)) !== null) {
          if (!(m[1] in Ru.strings) || !(m[1] in En.strings)) missing.push(name + ": " + m[1])
        }
      }
      verify(sources.count > 40, "plugin sources found")
      compare(missing, [], "keys the plugin uses but no dictionary has")
    }

    function test_fallback_and_lookup() {
      Plugin.MihomoI18n.language = "en"
      compare(Plugin.MihomoI18n.t("never"), "never")
      Plugin.MihomoI18n.language = "ru"
      compare(Plugin.MihomoI18n.t("dnsMode"), "Режим")
      compare(Plugin.MihomoI18n.t("no.such.key"), "no.such.key")
    }

    function test_shell_and_plugin_catalogs_are_independent() {
      compare(Commons.I18n.language, "en")
      compare(Commons.I18n.t("navOverview"), "navOverview")
      Plugin.MihomoI18n.language = "ru"
      compare(Plugin.MihomoI18n.t("navOverview"), Ru.strings.navOverview)
      compare(Plugin.MihomoI18n.mode("rule"), Ru.strings.mode_rule)
      Plugin.MihomoI18n.language = "en"
      compare(Plugin.MihomoI18n.t("navOverview"), En.strings.navOverview)
      compare(Commons.I18n.language, "en")
      compare(Commons.I18n.t("navOverview"), "navOverview")
      Plugin.MihomoI18n.language = "ru"
    }
  }
}
