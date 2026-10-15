import QtQuick
import QtQuick.Controls
import Quickshell
import qs.Commons

// Subscriptions and local configs; overrides are plain files opened in the editor.
Item {
  id: root
  property var svc
  property var panel

  property var items: []
  property string current: ""
  // Daemon's data directory, for opening profile and override files.
  property string dataRoot: Quickshell.env("HOME") + "/.local/share/omarchy-mihomo"
  property int cursor: 0
  property var busy: ({})
  property bool adding: false
  readonly property bool typing: urlField.input.activeFocus

  function reload() {
    svc.request("profiles.list", null, function(r) {
      if (!r) return
      items = r.items
      current = r.current || ""
      if (r.root) dataRoot = r.root
      cursor = Math.min(cursor, Math.max(0, items.length - 1))
    })
  }

  function setBusy(id, on) { var b = Object.assign({}, busy); if (on) b[id] = true; else delete b[id]; busy = b }

  function update(p) {
    setBusy(p ? p.id : "*", true)
    svc.request("profiles.update", p ? { id: p.id } : { force: true }, function(r, err) {
      setBusy(p ? p.id : "*", false)
      if (err) panel.notify(err.message)
      else {
        var failed = (r || []).filter(function(x) { return x.error })
        panel.notify(failed.length ? failed[0].name + ": " + failed[0].error : I18n.t("profileUpdated"))
      }
      reload()
    })
  }
  function use(p) {
    svc.request("profiles.select", { id: p.id }, function(r, err) { panel.notify(err ? err.message : I18n.t("profileSelected").arg(p.name)); reload() })
  }
  function del(p) {
    svc.request("profiles.remove", { id: p.id }, function(r, err) { if (err) panel.notify(err.message); reload() })
  }
  function add() {
    var src = urlField.text.trim()
    if (src === "") return
    adding = true
    var params = src.indexOf("://") > 0 ? { url: src } : { path: src.replace(/^~/, Quickshell.env("HOME")) }
    svc.request("profiles.add", params, function(r, err) {
      adding = false
      if (err) { panel.notify(err.message); return }
      urlField.text = ""
      panel.notify(I18n.t("profileAdded"))
      reload()
    })
  }
  function openInEditor(path) {
    Quickshell.execDetached(["bash", "-c", "omarchy-launch-editor \"$1\" || xdg-open \"$1\"", "omm", path])
  }
  function editOverrides(p) {
    var dir = dataRoot + "/overrides/" + (p ? p.id : "global")
    Quickshell.execDetached(["bash", "-c", "mkdir -p \"$1\" && cd \"$1\" && touch merge.yaml rules.yaml proxies.yaml groups.yaml && omarchy-launch-editor \"$1\" || xdg-open \"$1\"", "omm", dir])
    panel.notify(I18n.t("overridesHint"))
  }
  // An empty local profile opened in the editor; the daemon notices each
  // save, so pasting a config and pressing Ctrl+S is all it takes.
  function newFile() {
    var typed = urlField.text.trim()
    var looksLikeSource = typed.indexOf("://") > 0 || typed.indexOf("/") === 0 || typed.indexOf("~") === 0
    var name = typed !== "" && !looksLikeSource ? typed : I18n.t("newFileName")
    svc.request("profiles.create", { name: name }, function(r, err) {
      if (err) { panel.notify(err.message); return }
      if (!looksLikeSource) urlField.text = ""
      openInEditor(r.path)
      panel.notify(I18n.t("newFileHint"))
      reload()
    })
  }
  function editFile(p) { openInEditor(dataRoot + "/profiles/" + p.id + ".yaml") }

  Connections {
    target: root.svc
    function onProfileSaved(info) { root.reload() }
  }

  function move(dx, dy) { cursor = Math.max(0, Math.min(items.length - 1, cursor + dy)) }
  function activate() { if (items[cursor] && !items[cursor].draft) use(items[cursor]) }
  function remove() { if (items[cursor] && items[cursor].id !== current) del(items[cursor]) }
  function textKey(t) {
    if (t === "u" && items[cursor]) update(items[cursor])
    else if (t === "a" || t === "/") urlField.input.forceActiveFocus()
    else if (t === "e" && items[cursor]) editOverrides(items[cursor])
    else if (t === "n") newFile()
    else if (t === "o" && items[cursor] && items[cursor].source.type === "local") editFile(items[cursor])
  }

  Component.onCompleted: reload()

  Column {
    id: head
    width: parent.width
    spacing: Style.space(12)
    Row {
      width: parent.width
      Column {
        width: parent.width - updAll.width - globalBtn.width - Style.space(8)
        spacing: Style.space(3)
        Txt { text: I18n.t("profilesTitle"); bold: true; font.pixelSize: Style.font.heading }
        Txt { text: I18n.t("profilesSubtitle"); dim: true; font.pixelSize: Style.font.bodySmall }
      }
      Btn { id: globalBtn; icon: Icons.edit; text: I18n.t("globalOverrides"); onClicked: root.editOverrides(null) }
      Item { width: Style.space(8); height: 1 }
      Btn { id: updAll; icon: Icons.refresh; text: I18n.t("updateAll"); busy: !!root.busy["*"]; onClicked: root.update(null) }
    }
    Row {
      width: parent.width
      spacing: Style.space(8)
      Field {
        id: urlField
        width: parent.width - addBtn.width - newBtn.width - Style.space(16)
        icon: Icons.add
        placeholder: I18n.t("addPlaceholder")
        onAccepted: root.add()
        onEscaped: { text = ""; input.focus = false; root.panel.forceKeys() }
      }
      Btn { id: addBtn; primary: true; text: I18n.t("add"); busy: root.adding; onClicked: root.add() }
      Btn { id: newBtn; icon: Icons.file; text: I18n.t("newFile"); tooltip: I18n.t("newFileHint"); onClicked: root.newFile() }
    }
  }

  ListView {
    id: list
    anchors.top: head.bottom
    anchors.topMargin: Style.space(12)
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    clip: true
    spacing: Style.space(6)
    model: root.items
    ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
    delegate: Rectangle {
      required property var modelData
      required property int index
      readonly property bool isCurrent: modelData.id === root.current
      readonly property var u: modelData.usage
      width: list.width - Style.space(6)
      height: col.implicitHeight + Style.space(20)
      radius: Style.cornerRadius
      color: isCurrent ? Style.selectedFillFor(Color.popups.text, Color.accent) : (index === root.cursor ? Style.hoverFillFor(Color.popups.text, Color.accent) : Util.alpha(Color.popups.text, 0.03))
      border.width: 1
      border.color: isCurrent ? Color.accent : Util.alpha(Color.popups.text, 0.12)
      MouseArea { anchors.fill: parent; onClicked: root.cursor = index; onDoubleClicked: if (!modelData.draft) root.use(modelData) }
      Column {
        id: col
        anchors.left: parent.left; anchors.leftMargin: Style.space(12)
        anchors.right: actions.left; anchors.rightMargin: Style.space(10)
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(5)
        Row {
          spacing: Style.space(8)
          Txt { text: modelData.name; bold: true }
          Chip { showDot: false; text: modelData.source.type === "remote" ? I18n.t("remote") : I18n.t("local") }
          Chip { visible: isCurrent; dot: root.svc.green; text: I18n.t("active") }
          Chip { visible: !!modelData.draft; dot: root.svc.yellow; text: I18n.t("draft") }
        }
        Txt {
          width: parent.width
          text: modelData.draft ? I18n.t("newFileHint") : I18n.t("updatedAgo").arg(root.svc.ago(modelData.updated * 1000)) + (u && u.total > 0 ? " · " + I18n.t("usage").arg(root.svc.formatBytes(u.upload + u.download)).arg(root.svc.formatBytes(u.total)) : "") + (u && u.expire ? " · " + I18n.t("until").arg(new Date(u.expire * 1000).toLocaleDateString()) : "")
          dim: true; font.pixelSize: Style.font.caption
        }
        Txt { visible: !!modelData.last_error; width: parent.width; text: modelData.last_error || ""; color: root.svc.red; font.pixelSize: Style.font.caption; wrapMode: Text.Wrap; maximumLineCount: 2 }
      }
      Row {
        id: actions
        anchors.right: parent.right; anchors.rightMargin: Style.space(8)
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(4)
        Btn { visible: !isCurrent && !modelData.draft; text: I18n.t("use"); onClicked: root.use(modelData) }
        Btn { visible: modelData.source.type === "local"; ghost: true; icon: Icons.file; tooltip: I18n.t("editFile"); onClicked: root.editFile(modelData) }
        Btn { visible: modelData.source.type === "remote"; ghost: true; icon: Icons.refresh; busy: !!root.busy[modelData.id]; onClicked: root.update(modelData) }
        Btn { ghost: true; icon: Icons.edit; tooltip: I18n.t("editOverrides"); onClicked: root.editOverrides(modelData) }
        Btn { visible: !isCurrent; ghost: true; icon: Icons.remove; onClicked: root.del(modelData) }
      }
    }
    Txt { visible: root.items.length === 0; anchors.centerIn: parent; text: I18n.t("noProfiles"); dim: true }
  }
}
