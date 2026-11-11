pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import qs.Commons
import qs.Commons as Commons
import "ProxyModels.js" as Models

ComboBox {
  id: control
  property bool searchable: count > 8
  property string searchPlaceholder: MihomoI18n.t("proxyFindNode")
  // Optional runtime metadata, keyed by the original model text.
  property var optionDetails: ({})
  property var groupNames: []
  property string query: ""
  readonly property var options: {
    if (!model) return [];
    var result = [], needle = query.trim().toLowerCase();
    for (var i = 0; i < count; i++) {
      var label = textAt(i), info = optionDetails[label] || {};
      if (needle && label.toLowerCase().indexOf(needle) < 0) continue;
      result.push({ index: i, label: label, delay: info.delay == null ? undefined : info.delay,
        disabled: info.disabled === true || info.delay === 0,
        group: groupNames.length ? (groupNames.indexOf(label) >= 0 ? MihomoI18n.t("proxyGroups") : MihomoI18n.t("proxyNodes")) : "" });
    }
    if (groupNames.length) result.sort(function(a, b) {
      return Number(groupNames.indexOf(a.label) >= 0) - Number(groupNames.indexOf(b.label) >= 0) || a.index - b.index;
    });
    return result;
  }
  readonly property var currentDetails: optionDetails[currentText] || ({})
  function delayColor(delay) {
    return delay === 0 ? Commons.Color.urgent : delay == null ? Util.alpha(Commons.Color.popups.text, 0.42)
      : delay < 150 ? "#a6e3a1" : "#f9e2af";
  }
  function moveOption(step) {
    if (!options.length) { choices.currentIndex = -1; return; }
    var next = choices.currentIndex;
    for (var i = 0; i < options.length; i++) {
      next = (next + step + options.length) % options.length;
      if (!options[next].disabled) {
        choices.currentIndex = next;
        choices.positionViewAtIndex(next, ListView.Contain);
        return;
      }
    }
    choices.currentIndex = -1;
  }
  function choose(index) {
    var option = options[index];
    if (!option || option.disabled) return;
    currentIndex = option.index;
    if (editable) editText = textAt(currentIndex);
    activated(currentIndex);
    popup.close();
  }
  onOptionsChanged: {
    choices.currentIndex = -1;
    moveOption(1);
  }
  implicitHeight: Style.space(30)
  font.family: Style.font.family
  font.pixelSize: Style.font.body
  leftPadding: Style.space(currentDetails.delay != null ? 24 : 10)
  rightPadding: Style.space(30) + (currentDelay.visible ? currentDelay.implicitWidth + Style.space(8) : 0)
  contentItem: TextInput {
    text: control.editable ? control.editText : control.displayText
    font: control.font
    color: Commons.Color.popups.text
    verticalAlignment: TextInput.AlignVCenter
    readOnly: !control.editable
    selectByMouse: control.editable
    clip: true
    selectionColor: Style.selectionFillFor(Commons.Color.popups.text, Commons.Color.accent)
    onTextEdited: control.editText = text
    onAccepted: control.accepted()
  }
  Rectangle {
    visible: control.currentDetails.delay != null
    x: Style.space(10)
    anchors.verticalCenter: parent.verticalCenter
    width: Style.space(6); height: width; radius: width / 2
    color: control.delayColor(control.currentDetails.delay)
  }
  Txt {
    id: currentDelay
    visible: control.currentDetails.delay != null
    anchors.right: parent.right
    anchors.rightMargin: Style.space(30)
    anchors.verticalCenter: parent.verticalCenter
    text: control.currentDetails.delay === 0 ? MihomoI18n.t("proxyNoResponse") : Models.delayText(control.currentDetails.delay, MihomoI18n.t("ms"))
    color: control.delayColor(control.currentDetails.delay)
    font.pixelSize: Style.font.bodySmall
  }
  indicator: Txt {
    x: control.width - width - Style.space(10)
    anchors.verticalCenter: parent.verticalCenter
    text: Icons.expanded
    rotation: control.popup.visible ? 180 : 0
    font.pixelSize: Style.space(12)
    faint: true
  }
  background: Rectangle {
    color: Util.alpha(Commons.Color.popups.text, control.popup.visible ? 0.08 : 0.04)
    radius: Style.space(6)
    border.width: 1
    border.color: control.activeFocus || control.popup.visible ? Commons.Color.accent : Util.alpha(Commons.Color.popups.text, 0.15)
  }
  popup: Popup {
    id: dropdown
    objectName: "proxyChoicePopup"
    popupType: Popup.Item
    y: control.height + Style.space(5)
    width: control.width
    padding: Style.space(4)
    margins: Style.space(8)
    implicitHeight: Math.min(choices.contentHeight + searchBox.height + emptyLabel.height + padding * 2, Style.space(320))
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
    onAboutToShow: {
      control.query = "";
      choices.currentIndex = control.options.findIndex(function(o) { return o.index === control.currentIndex && !o.disabled; });
      if (choices.currentIndex < 0) control.moveOption(1);
    }
    onOpened: {
      if (control.searchable) searchInput.forceActiveFocus();
      else choices.forceActiveFocus();
      choices.positionViewAtIndex(choices.currentIndex, ListView.Contain);
    }
    onClosed: control.forceActiveFocus()
    background: MenuSurface {}
    enter: Transition {}
    exit: Transition {}
    contentItem: Item {
      Keys.onDownPressed: control.moveOption(1)
      Keys.onUpPressed: control.moveOption(-1)
      Keys.onReturnPressed: control.choose(choices.currentIndex)
      Keys.onEnterPressed: control.choose(choices.currentIndex)
      Keys.onEscapePressed: dropdown.close()
      Item {
        id: searchBox
        width: parent.width
        height: control.searchable ? Style.space(30) : 0
        visible: control.searchable
        Txt {
          x: Style.space(8)
          anchors.verticalCenter: parent.verticalCenter
          text: Icons.search
          font.pixelSize: Style.space(12)
          faint: true
        }
        TextField {
          id: searchInput
          objectName: "proxyChoiceSearch"
          x: Style.space(26)
          width: parent.width - x - Style.space(8)
          height: parent.height
          padding: 0
          text: control.query
          onTextEdited: control.query = text
          placeholderText: control.searchPlaceholder
          placeholderTextColor: Util.alpha(Commons.Color.popups.text, 0.42)
          color: Commons.Color.popups.text
          font.family: Style.font.family
          font.pixelSize: Style.font.body
          background: null
          selectByMouse: true
          Keys.onDownPressed: control.moveOption(1)
          Keys.onUpPressed: control.moveOption(-1)
          Keys.onReturnPressed: control.choose(choices.currentIndex)
          Keys.onEnterPressed: control.choose(choices.currentIndex)
        }
        Rectangle {
          anchors.bottom: parent.bottom
          width: parent.width; height: 1
          color: Util.alpha(Commons.Color.popups.text, 0.15)
        }
      }
      Txt {
        id: emptyLabel
        anchors.top: searchBox.bottom
        width: parent.width
        height: visible ? Style.space(28) : 0
        visible: control.options.length === 0
        text: MihomoI18n.t("proxyNoResults")
        horizontalAlignment: Text.AlignHCenter
        font.pixelSize: Style.font.body
        faint: true
      }
      ListView {
        id: choices
        objectName: "proxyChoiceOptions"
        anchors.top: emptyLabel.bottom
        anchors.bottom: parent.bottom
        width: parent.width
        clip: true
        keyNavigationEnabled: false
        model: control.options
        spacing: Style.space(1)
        boundsBehavior: Flickable.StopAtBounds
        ScrollIndicator.vertical: ScrollIndicator {}
        delegate: Column {
          id: optionRow
          required property var modelData
          required property int index
          width: choices.width
          Item {
            width: parent.width
            height: visible ? Style.space(23) : 0
            visible: optionRow.modelData.group !== "" && (optionRow.index === 0 || control.options[optionRow.index - 1].group !== optionRow.modelData.group)
            Txt {
              x: Style.space(29)
              anchors.verticalCenter: parent.verticalCenter
              text: optionRow.modelData.group.toUpperCase()
              font.pixelSize: Style.font.caption
              font.letterSpacing: Style.space(1)
              bold: true
              faint: true
            }
          }
          ActionMenuItem {
            width: parent.width
            text: optionRow.modelData.label
            enabled: !optionRow.modelData.disabled
            marked: control.currentIndex === optionRow.modelData.index
            highlighted: choices.currentIndex === optionRow.index
            detail: optionRow.modelData.delay == null ? "" : optionRow.modelData.delay === 0 ? MihomoI18n.t("proxyNoResponse") : Models.delayText(optionRow.modelData.delay, MihomoI18n.t("ms"))
            detailColor: control.delayColor(optionRow.modelData.delay)
            onTriggered: control.choose(optionRow.index)
          }
        }
      }
    }
  }
}
