import QtQuick
import QtQuick.Controls
import QtTest
import "../../plugin" as Plugin

Rectangle {
  id: scene
  width: 480; height: 260
  color: "#1e1e2e"
  Plugin.Btn {
    id: button
    x: 200; y: 40
    icon: Plugin.Icons.remove
    tooltip: "Очистить журнал"
    disabledTooltip: "Активный профиль нельзя удалить"
    keyHint: "c"
  }
  TestCase {
    name: "Hint"
    when: windowShown
    property var hint
    function init() {
      hint = findChild(button, "pluginHint")
      verify(hint)
      mouseMove(scene, 2, 2)
      button.x = 200; button.y = 40
      button.enabled = true
      button.tooltip = "Очистить журнал"
      button.keyHint = "c"
      tryCompare(hint, "visible", false)
    }
    function hover() {
      mouseMove(button, button.width / 2, button.height / 2)
      tryCompare(hint, "visible", true, 1500)
      wait(40)
    }
    function test_delay_and_key_badge() {
      mouseMove(button, button.width / 2, button.height / 2)
      wait(200)
      verify(!hint.visible)
      tryCompare(hint, "visible", true, 1500)
      compare(hint.keyHint, "c")
      compare(hint.y, button.height + 6)
      verify(hint.width <= 240)
      wait(50)
      grabImage(scene.parent).save("/tmp/omm-hint-key.png")
      mouseMove(scene, 2, 2)
      tryCompare(hint, "visible", false)
    }
    function test_long_text_wraps_at_right_edge() {
      button.x = scene.width - button.width - 8
      button.keyHint = ""
      button.tooltip = "Открыть папку переопределений этого профиля"
      hover()
      verify(hint.width <= 240)
      verify(button.x + hint.x + hint.width <= scene.width - 8)
      verify(hint.height > 30)
      grabImage(scene.parent).save("/tmp/omm-hint-long.png")
    }
    function test_flips_at_bottom_edge() {
      button.y = scene.height - button.height - 8
      hover()
      compare(hint.y, -hint.height - 6)
      grabImage(scene.parent).save("/tmp/omm-hint-bottom.png")
    }
    function test_disabled_button_explains_why() {
      button.enabled = false
      button.keyHint = ""
      hover()
      compare(hint.text, button.disabledTooltip)
      grabImage(scene.parent).save("/tmp/omm-hint-disabled.png")
    }
  }
}
