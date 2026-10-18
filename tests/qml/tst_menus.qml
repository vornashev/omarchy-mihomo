import QtQuick
import QtQuick.Controls
import QtTest
import "../../plugin" as Plugin

Rectangle {
  id: scene
  width: 900; height: 620
  color: "#1e1e2e"
  QtObject {
    id: service
    property color red: "#f38ba8"
    property color green: "#a6e3a1"
    property color yellow: "#f9e2af"
  }
  Component {
    id: menuComponent
    Plugin.ActionMenu {
      x: 20; y: 20
      property string result: ""
      Plugin.ActionMenuItem { glyph: Plugin.Icons.edit; text: "Редактировать"; keyHint: "Enter"; onTriggered: result = "edit" }
      Plugin.ActionMenuItem { glyph: Plugin.Icons.test; text: "Проверить сейчас"; keyHint: "T"; onTriggered: result = "test" }
      Plugin.ActionMenuItem { glyph: Plugin.Icons.copy; text: "Дублировать"; keyHint: "Ctrl+D" }
      Plugin.ActionMenuSeparator {}
      Plugin.ActionMenuItem { enabled: false; glyph: Plugin.Icons.undo; text: "Сбросить к профилю"; hint: "нет локальных изменений" }
      Plugin.ActionMenuSeparator {}
      Plugin.ActionMenuItem { dangerous: true; glyph: Plugin.Icons.remove; text: "Удалить…"; keyHint: "Del"; onTriggered: result = "delete" }
    }
  }
  Component {
    id: choiceComponent
    Plugin.ProxyChoice {
      x: 300; y: 20; width: 276
      searchable: true
      model: ["Frankfurt 02", "Amsterdam", "New York", "Helsinki", "Auto · Europe"]
      groupNames: ["Auto · Europe"]
      optionDetails: ({ "Frankfurt 02": {delay:48}, Amsterdam: {delay:92}, "New York": {delay:160}, Helsinki: {delay:0}, "Auto · Europe": {delay:48} })
      property int activations: 0
      onActivated: activations++
    }
  }
  Component {
    id: cardComponent
    Plugin.ProxyGroupCard {
      x: 20; y: 330; width: 510
      entry: ({name:"Стриминг", type:"select", members:["WARP", "FI", "KZ", "DIRECT"]})
      runtime: ({now:"WARP", members:[{name:"WARP", delay:43}, {name:"FI", delay:168}, {name:"KZ", delay:0}, {name:"DIRECT", delay:12}]})
      svc: service
      property string selection: ""
      property int editCount: 0
      property int testCount: 0
      onSelected: function(name) { selection = name; }
      onEditRequested: editCount++
      onTestRequested: testCount++
    }
  }
  TestCase {
    name: "Menus"
    when: windowShown
    function test_select_search_keyboard_and_original_index() {
      var c = createTemporaryObject(choiceComponent, scene);
      c.popup.open();
      tryCompare(c.popup, "opened", true);
      var input = findChild(c.popup, "proxyChoiceSearch");
      verify(input.activeFocus);
      keyClick(Qt.Key_A); keyClick(Qt.Key_M);
      compare(c.options.length, 1);
      compare(c.options[0].index, 1);
      keyClick(Qt.Key_Return);
      compare(c.currentIndex, 1);
      compare(c.activations, 1);
      tryCompare(c.popup, "visible", false);
      verify(c.activeFocus);
      c.popup.open();
      tryCompare(c.popup, "opened", true);
      compare(c.query, "");
      keyClick(Qt.Key_Escape);
      tryCompare(c.popup, "visible", false);
      compare(c.activations, 1);
    }
    function test_select_skips_unavailable_and_empty_result() {
      var c = createTemporaryObject(choiceComponent, scene, {currentIndex:2});
      c.popup.open();
      tryCompare(c.popup, "opened", true);
      keyClick(Qt.Key_Down);
      keyClick(Qt.Key_Return);
      compare(c.currentIndex, 4);
      c.popup.open();
      tryCompare(c.popup, "opened", true);
      keyClick(Qt.Key_Z);
      compare(c.options.length, 0);
      keyClick(Qt.Key_Return);
      verify(c.popup.visible);
      compare(c.activations, 1);
      keyClick(Qt.Key_Escape);
    }
    function test_editable_select_preserves_free_text() {
      var c = createTemporaryObject(choiceComponent, scene, {editable:true});
      c.contentItem.forceActiveFocus();
      keyClick(Qt.Key_A, Qt.ControlModifier);
      keyClick(Qt.Key_X);
      compare(c.editText, "x");
      c.popup.open();
      tryCompare(c.popup, "opened", true);
      keyClick(Qt.Key_A); keyClick(Qt.Key_M);
      keyClick(Qt.Key_Return);
      compare(c.editText, "Amsterdam");
      compare(c.currentText, "Amsterdam");
      compare(c.activations, 1);
    }
    function test_select_without_search_skips_unavailable() {
      var c = createTemporaryObject(choiceComponent, scene, {searchable:false, currentIndex:2});
      c.popup.open();
      tryCompare(c.popup, "opened", true);
      keyClick(Qt.Key_Down); keyClick(Qt.Key_Return);
      compare(c.currentIndex, 4);
    }
    function test_same_length_model_update_and_unknown_latency() {
      var c = createTemporaryObject(choiceComponent, scene);
      c.model = ["One", "Two", "Three", "Four", "Five"];
      compare(c.options[0].label, "One");
      c.optionDetails = {One:{delay:null}};
      verify(!c.options[0].disabled);
      compare(c.options[0].delay, undefined);
      c.popup.open();
      tryCompare(c.popup, "opened", true);
      keyClick(Qt.Key_Return);
      compare(c.currentText, "One");
      compare(c.activations, 1);
    }
    function test_menu_default_enter() {
      var m = createTemporaryObject(menuComponent, scene);
      m.open();
      tryCompare(m, "opened", true);
      keyClick(Qt.Key_Return);
      compare(m.result, "edit");
      tryCompare(m, "visible", false);
    }
    function test_menu_keyboard_skips_disabled_and_closes() {
      var m = createTemporaryObject(menuComponent, scene);
      m.open();
      tryCompare(m, "opened", true);
      m.currentIndex = 2;
      m.itemAt(2).forceActiveFocus();
      keyClick(Qt.Key_Down);
      compare(m.currentIndex, 6);
      keyClick(Qt.Key_Return);
      compare(m.result, "delete");
      tryCompare(m, "visible", false);
    }
    function test_outside_click_and_escape() {
      var c = createTemporaryObject(choiceComponent, scene);
      c.popup.open();
      tryCompare(c.popup, "opened", true);
      mouseClick(scene, 850, 590);
      tryCompare(c.popup, "visible", false);
      compare(c.activations, 0);
      var m = createTemporaryObject(menuComponent, scene);
      m.open();
      tryCompare(m, "opened", true);
      keyClick(Qt.Key_Escape);
      tryCompare(m, "visible", false);
      compare(m.result, "");
    }
    function test_quick_switch_selection_and_actions() {
      var c = createTemporaryObject(cardComponent, scene);
      var more = findChild(c, "proxyGroupMoreButton");
      var popup = findChild(c, "proxyQuickSwitch");
      mouseClick(more, more.width/2, more.height/2);
      tryCompare(popup, "opened", true);
      keyClick(Qt.Key_Down);
      keyClick(Qt.Key_Return);
      compare(c.selection, "FI");
      tryCompare(popup, "visible", false);
      verify(more.activeFocus);
      mouseClick(more, more.width/2, more.height/2);
      tryCompare(popup, "opened", true);
      var test = findChild(popup, "quickSwitchTest");
      mouseClick(test, test.width/2, test.height/2);
      compare(c.testCount, 1);
      var edit = findChild(popup, "quickSwitchEdit");
      mouseClick(edit, edit.width/2, edit.height/2);
      compare(c.editCount, 1);
      tryCompare(popup, "visible", false);
    }
    function test_quick_switch_automatic_group_does_not_select() {
      var c = createTemporaryObject(cardComponent, scene, {entry:{name:"Auto", type:"url-test"}});
      var popup = findChild(c, "proxyQuickSwitch");
      popup.open();
      tryCompare(popup, "opened", true);
      keyClick(Qt.Key_Down); keyClick(Qt.Key_Return);
      compare(c.selection, "");
      verify(popup.visible);
      keyClick(Qt.Key_Escape);
    }
    function test_visual_and_scroll() {
      var c = createTemporaryObject(choiceComponent, scene);
      c.popup.open();
      tryCompare(c.popup, "opened", true);
      wait(30);
      grabImage(scene.parent).save("/tmp/mihomo-select.png");
      c.popup.close();
      var m = createTemporaryObject(menuComponent, scene);
      m.open();
      tryCompare(m, "opened", true);
      m.currentIndex = 0;
      wait(30);
      grabImage(scene.parent).save("/tmp/mihomo-action-menu.png");
      m.close();
      var card = createTemporaryObject(cardComponent, scene);
      var popup = findChild(card, "proxyQuickSwitch");
      popup.open();
      tryCompare(popup, "opened", true);
      wait(30);
      grabImage(scene.parent).save("/tmp/mihomo-quick-switch.png");
      popup.close();
      var names = [];
      for (var i = 0; i < 80; i++) names.push("Node " + i);
      c.model = names;
      c.currentIndex = 78;
      c.popup.open();
      tryCompare(c.popup, "opened", true);
      verify(c.popup.height <= 320);
      keyClick(Qt.Key_Down); keyClick(Qt.Key_Return);
      compare(c.currentIndex, 79);
    }
  }
}
