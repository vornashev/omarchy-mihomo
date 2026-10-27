import QtQuick
import QtQuick.Controls
import qs.Commons
import "RuleModels.js" as RuleModels

// Value field of a rule with suggestions from live connections, logs,
// problems, running programs and the core's databases (`rules.suggest`).
// ↑↓ choose, Tab completes, Enter picks, Esc closes the list.
Rectangle {
  id: root
  property var svc
  property string kind: "domain-suffix"
  property alias text: input.text
  property alias input: input
  property string placeholder: ""
  property var items: []
  property string note: ""
  // Why the last catalog pick failed, shown under the field.
  property string addError: ""
  property bool adding: false
  property int highlighted: -1
  property bool suggestOpen: false
  property int serial: 0
  readonly property var rows: RuleModels.rows(items)
  readonly property var current: highlighted >= 0 && highlighted < rows.length ? rows[highlighted] : null
  // Grey tail the top suggestion would add to what is typed.
  readonly property string ghost: {
    if (!input.activeFocus || input.text === "" || items.length === 0) return ""
    var first = items[0].value
    return first.toLowerCase().indexOf(input.text.toLowerCase()) === 0 ? first.slice(input.text.length) : ""
  }
  signal picked(var item)
  signal accepted()
  signal escaped()

  implicitHeight: Style.space(32)
  radius: Style.cornerRadius
  color: Qt.darker(Color.popups.background, 1.15)
  border.width: 1
  border.color: input.activeFocus ? Color.accent : Util.alpha(Color.popups.text, 0.15)

  function refresh() {
    if (!svc) return
    var mine = ++serial
    addError = ""
    svc.request("rules.suggest", { kind: kind, query: input.text }, function(r, err) {
      if (mine !== serial) return
      items = err || !r ? [] : r.items
      note = r && r.note ? r.note : ""
      highlighted = firstItem(0, 1)
      suggestOpen = input.activeFocus && (rows.length > 0 || note !== "")
    })
  }
  function firstItem(from, step) {
    for (var i = from; i >= 0 && i < rows.length; i += step) if (!rows[i].header) return i
    return -1
  }
  function moveHighlight(step) {
    var next = firstItem(highlighted + step, step)
    if (next !== -1) { highlighted = next; list.positionViewAtIndex(next, ListView.Contain) }
  }
  function pick(item) {
    suggestOpen = false
    if (item.source === "catalog") { addFromCatalog(item); return }
    input.text = item.value
    picked(item)
  }
  // A catalog list becomes one of the user's rule sets before the rule
  // can name it.
  function addFromCatalog(item) {
    adding = true
    addError = ""
    svc.request("rule_sets.add", { catalog: item.detail }, function(r, err) {
      root.adding = false
      if (err) { root.addError = I18n.t("catalogFailed").arg(err.message); root.items = []; root.suggestOpen = true; return }
      input.text = r.name
      root.picked({ value: r.name, source: "provider" })
    }, true)
  }
  function submit() {
    if (suggestOpen && current) pick(current)
    else accepted()
  }
  function complete() {
    if (ghost !== "") input.text = input.text + ghost
    else if (current) input.text = current.value
  }
  function metaText(it) {
    var parts = []
    if (it.source === "conn" && it.count) parts.push(I18n.t("suggestConns").arg(it.count))
    if (it.source === "log" && it.last && svc) parts.push(svc.ago(it.last))
    if (it.source === "problem") parts.push("×" + it.count)
    if (it.source === "db") parts.push(I18n.t("suggestDomains").arg(it.count))
    if (it.source === "provider") parts.push(I18n.t("suggestRules").arg(it.count))
    if (it.source === "catalog") parts.push(it.detail + " · " + I18n.t("catalogAdds"))
    if (it.source === "user" && it.count) parts.push(I18n.t("suggestConns").arg(it.count))
    if (it.route) parts.push(it.route)
    return parts.join(" · ")
  }
  function sourceIcon(s) {
    return ({ common: Icons.sparkles, conn: Icons.pulse, log: Icons.logs, problem: Icons.problems, window: Icons.app,
              running: Icons.cpu, user: Icons.account, db: Icons.sets, provider: Icons.sets, known: Icons.info })[s] || Icons.info
  }

  onKindChanged: { items = []; if (input.activeFocus) refresh() }
  Timer { id: debounce; interval: 160; onTriggered: root.refresh() }

  Txt {
    id: prefix
    visible: root.kind === "domain-suffix"
    anchors.left: parent.left; anchors.leftMargin: Style.space(10)
    anchors.verticalCenter: parent.verticalCenter
    text: "*."; faint: true
  }
  TextInput {
    id: input
    anchors.left: prefix.visible ? prefix.right : parent.left
    anchors.leftMargin: prefix.visible ? 0 : Style.space(10)
    anchors.right: tabKey.left; anchors.rightMargin: Style.space(8)
    anchors.verticalCenter: parent.verticalCenter
    color: Color.popups.text
    selectionColor: Style.selectionFillFor(Color.popups.text, Color.accent)
    font.family: Style.font.family
    font.pixelSize: Style.font.body
    clip: true
    onTextEdited: debounce.restart()
    onActiveFocusChanged: {
      if (activeFocus) root.refresh()
      else root.suggestOpen = false
    }
    Keys.onDownPressed: function(e) { if (root.suggestOpen) root.moveHighlight(1); else root.refresh(); e.accepted = true }
    Keys.onUpPressed: function(e) { if (root.suggestOpen) root.moveHighlight(-1); e.accepted = true }
    Keys.onTabPressed: function(e) { root.complete(); debounce.restart(); e.accepted = true }
    Keys.onReturnPressed: function(e) { root.submit(); e.accepted = true }
    Keys.onEnterPressed: function(e) { root.submit(); e.accepted = true }
    Keys.onEscapePressed: function(e) {
      if (root.suggestOpen) root.suggestOpen = false
      else root.escaped()
      e.accepted = true
    }
    Txt {
      x: input.contentWidth
      anchors.verticalCenter: parent.verticalCenter
      text: root.ghost; faint: true
    }
  }
  Txt {
    visible: input.text === "" && !input.activeFocus
    anchors.fill: input
    text: root.placeholder; faint: true
  }
  Rectangle {
    id: tabKey
    visible: root.ghost !== ""
    anchors.right: parent.right; anchors.rightMargin: Style.space(8)
    anchors.verticalCenter: parent.verticalCenter
    width: visible ? tabLabel.implicitWidth + Style.space(10) : 0
    height: tabLabel.implicitHeight + Style.space(2)
    radius: 3
    color: "transparent"
    border.width: 1
    border.color: Util.alpha(Color.popups.text, 0.15)
    Txt { id: tabLabel; anchors.centerIn: parent; text: "Tab"; faint: true; font.pixelSize: Style.font.caption - 1 }
  }
  MouseArea { anchors.fill: parent; cursorShape: Qt.IBeamCursor; onClicked: input.forceActiveFocus(); propagateComposedEvents: true }

  Popup {
    id: popup
    objectName: "ruleSuggestions"
    popupType: Popup.Item
    visible: root.suggestOpen
    y: root.height + Style.space(2)
    width: root.width
    padding: Style.space(4)
    closePolicy: Popup.NoAutoClose
    focus: false
    background: MenuSurface {}
    contentItem: Column {
      spacing: 0
      Txt {
        visible: root.rows.length === 0 && (root.note !== "" || root.addError !== "" || root.adding)
        width: parent.width
        leftPadding: Style.space(8); topPadding: Style.space(6); bottomPadding: Style.space(6)
        text: root.adding ? I18n.t("catalogAdding") : root.addError || (root.note ? I18n.t("note_" + root.note) : "")
        dim: true; wrapMode: Text.Wrap; font.pixelSize: Style.font.bodySmall
      }
      ListView {
        id: list
        width: parent.width
        height: Math.min(contentHeight, Style.space(300))
        clip: true
        interactive: contentHeight > height
        boundsBehavior: Flickable.StopAtBounds
        model: root.rows
        ScrollIndicator.vertical: ScrollIndicator {}
        delegate: Item {
          required property var modelData
          required property int index
          width: list.width
          height: modelData.header ? Style.space(22) : (modelData.detail ? Style.space(36) : Style.space(28))
          Row {
            visible: !!modelData.header
            anchors.left: parent.left; anchors.leftMargin: Style.space(8)
            anchors.right: parent.right; anchors.rightMargin: Style.space(8)
            anchors.bottom: parent.bottom; anchors.bottomMargin: Style.space(3)
            Txt { width: parent.width - cnt.width; text: modelData.header ? I18n.t("src_" + modelData.source) : ""; faint: true; bold: true; font.pixelSize: Style.font.caption; font.letterSpacing: 1 }
            Txt { id: cnt; text: modelData.header ? String(modelData.count) : ""; faint: true; font.pixelSize: Style.font.caption }
          }
          Rectangle {
            visible: !modelData.header
            anchors.fill: parent
            radius: Style.space(4)
            color: index === root.highlighted ? Util.alpha(Color.popups.text, 0.18) : (rowMouse.containsMouse ? Util.alpha(Color.popups.text, 0.08) : "transparent")
            Txt {
              id: icon
              anchors.left: parent.left; anchors.leftMargin: Style.space(8)
              anchors.verticalCenter: parent.verticalCenter
              text: modelData.header ? "" : root.sourceIcon(modelData.source)
              color: modelData.failed ? root.svc.red : (modelData.source === "common" ? root.svc.yellow : (modelData.source === "conn" ? root.svc.green : Util.alpha(Color.popups.text, 0.6)))
              font.pixelSize: Style.font.icon
            }
            Column {
              anchors.left: icon.right; anchors.leftMargin: Style.space(8)
              anchors.right: meta.left; anchors.rightMargin: Style.space(8)
              anchors.verticalCenter: parent.verticalCenter
              spacing: Style.space(1)
              Row {
                spacing: Style.space(8)
                Txt { text: modelData.header ? "" : (modelData.source === "common" && root.kind === "domain-suffix" ? "*." : "") + modelData.value; bold: true }
                Rectangle {
                  visible: !!modelData.used_by
                  anchors.verticalCenter: parent.verticalCenter
                  width: usedLabel.implicitWidth + Style.space(10); height: usedLabel.implicitHeight + Style.space(2)
                  radius: 3
                  color: Util.alpha(root.svc ? root.svc.yellow : Color.accent, 0.15)
                  Txt { id: usedLabel; anchors.centerIn: parent; text: modelData.used_by ? I18n.t("usedBy").arg(modelData.used_by) : ""; color: root.svc ? root.svc.yellow : Color.accent; font.pixelSize: Style.font.caption - 1 }
                }
              }
              Txt { visible: !!modelData.detail; width: parent.width; text: modelData.detail || ""; faint: true; font.pixelSize: Style.font.caption }
            }
            Txt {
              id: meta
              anchors.right: parent.right; anchors.rightMargin: Style.space(8)
              anchors.verticalCenter: parent.verticalCenter
              width: Math.min(implicitWidth, parent.width * 0.42)
              horizontalAlignment: Text.AlignRight
              text: modelData.header ? "" : root.metaText(modelData)
              color: modelData.failed ? root.svc.red : Util.alpha(Color.popups.text, 0.42)
              font.pixelSize: Style.font.caption
            }
            MouseArea { id: rowMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.pick(modelData) }
          }
        }
      }
      Row {
        visible: root.rows.length > 0
        topPadding: Style.space(5); leftPadding: Style.space(8); bottomPadding: Style.space(3)
        spacing: Style.space(10)
        Repeater {
          model: [["↑↓", I18n.t("keyChoose")], ["Tab", I18n.t("keyComplete")], ["Enter", I18n.t("keyPick")]]
          Row {
            required property var modelData
            spacing: Style.space(4)
            Rectangle {
              anchors.verticalCenter: parent.verticalCenter
              width: k.implicitWidth + Style.space(8); height: k.implicitHeight + Style.space(2); radius: 3
              color: Util.alpha(Color.popups.text, 0.08)
              Txt { id: k; anchors.centerIn: parent; text: modelData[0]; dim: true; font.pixelSize: Style.font.caption - 1 }
            }
            Txt { text: modelData[1]; faint: true; font.pixelSize: Style.font.caption }
          }
        }
      }
    }
  }
}
