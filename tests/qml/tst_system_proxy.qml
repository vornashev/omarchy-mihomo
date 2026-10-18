import QtQuick
import QtTest
import "../../plugin" as Plugin

Rectangle {
  id: scene
  width: 640; height: 100
  color: "#1e1e2e"
  QtObject {
    id: service
    property bool connected: true
    property bool coreRunning: true
    property bool systemProxy: false
    property bool systemProxyWanted: false
    property bool systemProxyAvailable: true
    property bool systemProxyBusy: false
    property string systemProxyError: ""
    property color red: "#f38ba8"
    property int calls: 0
    function toggleSystemProxy() { calls++; systemProxyBusy = true }
  }
  Plugin.SystemProxyControl {
    id: control
    anchors.left: parent.left; anchors.right: parent.right
    anchors.margins: 20
    y: 20
    svc: service
  }
  TestCase {
    name: "SystemProxy"
    when: windowShown
    property var toggle
    function init() {
      toggle = findChild(control, "systemProxySwitch")
      verify(toggle)
      service.connected = true; service.coreRunning = true
      service.systemProxy = false; service.systemProxyWanted = false
      service.systemProxyAvailable = true; service.systemProxyBusy = false
      service.systemProxyError = ""; service.calls = 0
    }
    function test_click_waits_for_confirmed_state_and_blocks_double_click() {
      mouseClick(toggle)
      compare(service.calls, 1)
      verify(!toggle.checked)
      verify(!toggle.enabled)
      mouseClick(toggle)
      compare(service.calls, 1)
      service.systemProxy = true; service.systemProxyWanted = true; service.systemProxyBusy = false
      verify(toggle.checked)
      mouseClick(toggle)
      compare(service.calls, 2)
    }
    function test_disable_remains_available_when_core_stops() {
      service.coreRunning = false
      verify(!toggle.enabled)
      service.systemProxyWanted = true
      verify(toggle.enabled)
      mouseClick(toggle)
      compare(service.calls, 1)
    }
    function test_unavailable_and_disconnected() {
      service.systemProxyAvailable = false
      service.systemProxyError = "gsettings: No such schema"
      verify(!toggle.enabled)
      mouseClick(toggle)
      compare(service.calls, 0)
      service.systemProxy = true; service.connected = false
      verify(!toggle.enabled)
    }
  }
}
