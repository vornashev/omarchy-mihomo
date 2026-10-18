import QtQuick
import QtTest
import "../../plugin" as Plugin

Rectangle {
  width: 596; height: 644
  color: "#1e1e2e"
  QtObject {
    id: service
    property bool connected: true
    property bool coreRunning: true
    property var logLines: []
    property color red: "#f38ba8"
    property color yellow: "#f9e2af"
    function want(topic, on) {}
  }
  Plugin.LogsPage { id: page; anchors.fill: parent; svc: service }
  TestCase {
    name: "Logs"
    when: windowShown
    function init() {
      service.connected = true; service.coreRunning = true
      page.level = "all"; page.search = ""; page.follow = true
      service.logLines = [
        {type: "info", payload: "[TCP] github.com", timestamp: 1000},
        {type: "warning", payload: "[UDP] timeout", timestamp: 2000},
        {type: "error", payload: "[DNS] failed", timestamp: 3000}
      ]
    }
    function test_levels_and_case_insensitive_search() {
      compare(page.shown.length, 3)
      page.level = "warning"; compare(page.shown.length, 2)
      page.level = "error"; compare(page.shown.length, 1)
      page.search = "dns"; compare(page.shown.length, 1)
      page.search = "github"; compare(page.shown.length, 0)
      verify(page.emptyText.indexOf("фильтр") !== -1)
    }
    function test_clear_and_resume_follow() {
      mouseClick(findChild(page, "clearLogs"))
      compare(service.logLines.length, 0)
      var lines = []
      for (var i = 0; i < 100; i++) lines.push({type: "info", payload: "message " + i})
      service.logLines = lines
      var list = findChild(page, "logList")
      tryCompare(list, "atYEnd", true)
      page.follow = false
      list.positionViewAtBeginning()
      verify(!list.atYEnd)
      mouseClick(findChild(page, "followLogs"))
      tryCompare(list, "atYEnd", true)
      page.textKey("c"); compare(service.logLines.length, 0)
    }
    function test_long_plain_text_is_not_truncated() {
      service.logLines = [{type: "error", payload: "<b>literal</b> " + "long error ".repeat(100)}]
      var list = findChild(page, "logList")
      tryVerify(function() { return list.contentItem.children.length > 0 })
      tryVerify(function() { return list.contentHeight > 100 })
      verify(list.y >= 0)
    }
  }
}
