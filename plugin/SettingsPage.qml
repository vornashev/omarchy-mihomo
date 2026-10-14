import QtQuick
import qs.Commons

// Managed settings: applied on top of every profile.
Item {
  id: root
  property var svc
  property var panel

  property var s: null
  property bool saving: false
  readonly property bool typing: portField.input.activeFocus || intervalField.input.activeFocus

  function reload() { svc.request("settings.get", null, function(r) { if (r) { s = r; portField.text = String(r["mixed-port"]); intervalField.text = String(r["update-interval-hours"]) } }) }

  function save(patch) {
    saving = true
    svc.request("settings.set", patch, function(r, err) {
      saving = false
      panel.notify(err ? err.message : I18n.t("settingsSaved"))
      reload()
    })
  }

  function setLocale(lang) {
    var bar = panel.bar
    if (!bar || !bar.shell || typeof bar.shell.updateEntryInline !== "function") { I18n.language = lang; return }
    var entry = { id: panel.moduleName }
    for (var k in panel.settings) if (k !== "id") entry[k] = panel.settings[k]
    entry.locale = lang
    bar.shell.updateEntryInline(panel.moduleName, entry)
  }

  Component.onCompleted: reload()

  Column {
    visible: root.s !== null
    anchors.fill: parent
    spacing: Style.space(16)

    Column {
      spacing: Style.space(3)
      Txt { text: I18n.t("settingsTitle"); bold: true; font.pixelSize: Style.font.heading }
      Txt { text: I18n.t("settingsSubtitle"); dim: true; font.pixelSize: Style.font.bodySmall }
    }

    Repeater {
      model: root.s ? [
        { label: "TUN", hint: I18n.t("tunHint"), key: "tun", on: root.s.tun.enable },
        { label: I18n.t("allowLan"), hint: I18n.t("allowLanHint"), key: "allow-lan", on: root.s["allow-lan"] },
        { label: "IPv6", hint: I18n.t("ipv6Hint"), key: "ipv6", on: root.s.ipv6 },
        { label: I18n.t("processNames"), hint: I18n.t("processNamesHint"), key: "find-process-mode", on: root.s["find-process-mode"] === "always" }
      ] : []
      Row {
        required property var modelData
        width: parent.width
        Column {
          width: parent.width - sw.width
          spacing: Style.space(2)
          Txt { text: modelData.label }
          Txt { text: modelData.hint; dim: true; font.pixelSize: Style.font.caption }
        }
        Switch {
          id: sw
          anchors.verticalCenter: parent.verticalCenter
          checked: modelData.on
          onToggled: {
            var p = {}
            if (modelData.key === "tun") p.tun = { enable: !modelData.on }
            else if (modelData.key === "find-process-mode") p["find-process-mode"] = modelData.on ? "strict" : "always"
            else p[modelData.key] = !modelData.on
            root.save(p)
          }
        }
      }
    }

    Row {
      spacing: Style.space(16)
      Txt { text: I18n.t("tunStack"); width: Style.space(200); anchors.verticalCenter: parent.verticalCenter }
      Seg {
        current: root.s ? root.s.tun.stack : ""
        model: [{ key: "gvisor", label: "gVisor" }, { key: "system", label: "system" }, { key: "mixed", label: "mixed" }]
        onPicked: function(k) { root.save({ tun: { stack: k } }) }
      }
    }

    Row {
      spacing: Style.space(16)
      Txt { text: I18n.t("mixedPort"); width: Style.space(200); anchors.verticalCenter: parent.verticalCenter }
      Field { id: portField; width: Style.space(100); onAccepted: { var n = parseInt(text); if (n > 0 && n < 65536) root.save({ "mixed-port": n }); input.focus = false; root.panel.forceKeys() } }
      Txt { text: I18n.t("enterToSave"); faint: true; font.pixelSize: Style.font.caption; anchors.verticalCenter: parent.verticalCenter }
    }

    Row {
      spacing: Style.space(16)
      Txt { text: I18n.t("updateInterval"); width: Style.space(200); anchors.verticalCenter: parent.verticalCenter }
      Field { id: intervalField; width: Style.space(100); onAccepted: { var n = parseInt(text); if (n >= 0) root.save({ "update-interval-hours": n }); input.focus = false; root.panel.forceKeys() } }
      Txt { text: I18n.t("hours0off"); faint: true; font.pixelSize: Style.font.caption; anchors.verticalCenter: parent.verticalCenter }
    }

    Row {
      spacing: Style.space(16)
      Txt { text: I18n.t("language"); width: Style.space(200); anchors.verticalCenter: parent.verticalCenter }
      Seg {
        current: I18n.language
        model: [{ key: "ru", label: "Русский" }, { key: "en", label: "English" }]
        onPicked: function(k) { root.setLocale(k) }
      }
    }

    Txt { width: parent.width; text: I18n.t("settingsFootnote"); faint: true; font.pixelSize: Style.font.caption; wrapMode: Text.Wrap }
  }
}
