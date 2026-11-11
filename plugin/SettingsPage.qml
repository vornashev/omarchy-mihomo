import QtQuick
import QtQuick.Controls
import qs.Commons
import "." as Mihomo

// Managed settings: applied on top of every profile.
Item {
  id: root
  property var svc
  property var panel

  property var s: null
  property bool saving: false
  readonly property bool typing: portField.input.activeFocus || intervalField.input.activeFocus || automation.typing || core.typing || dnsPage.typing || backupPath.input.activeFocus

  function reload() { svc.request("settings.get", null, function(r) { if (r) { s = r; portField.text = String(r["mixed-port"]); intervalField.text = String(r["update-interval-hours"]) } }) }

  function save(patch) {
    saving = true
    svc.request("settings.set", patch, function(r, err) {
      saving = false
      panel.notify(err ? err.message : MihomoI18n.t("settingsSaved"))
      reload()
    })
  }

  // The plugin's own entry in shell.json, as the shell's settings form writes it.
  function setSetting(key, value) {
    var bar = panel.bar
    if (!bar || !bar.shell || typeof bar.shell.updateEntryInline !== "function") {
      if (key === "locale") MihomoI18n.language = MihomoI18n.resolve(value)
      return
    }
    var entry = { id: panel.moduleName }
    for (var k in panel.settings) if (k !== "id") entry[k] = panel.settings[k]
    entry[key] = value
    bar.shell.updateEntryInline(panel.moduleName, entry)
  }

  property string backupBusy: ""
  function backup(method, params) {
    backupBusy = method === "backup.export" ? "export" : "import"
    svc.request(method, params, function(r, err) {
      root.backupBusy = ""
      if (err) { root.panel.notify(err.message); return }
      if (method === "backup.export") {
        backupPath.text = r.path
        root.panel.notify(MihomoI18n.t("backupSaved").arg(r.rules).arg(r.profiles).arg(r.path))
      } else {
        root.panel.notify(MihomoI18n.t("backupLoaded").arg(r.rules).arg((r.profiles_added || []).length).arg((r.profiles_kept || []).length))
        root.reload()
      }
    }, true)
  }

  property var traffic: null
  function reloadTraffic() { svc.request("traffic.status", null, function(r) { if (r) traffic = r }) }

  Component.onCompleted: { reload(); reloadTraffic() }

  Column {
    id: header
    width: parent.width
    spacing: Style.space(12)
    Column {
      spacing: Style.space(3)
      Txt { text: MihomoI18n.t("settingsTitle"); bold: true; font.pixelSize: Style.font.heading }
      Txt { text: MihomoI18n.t("settingsSubtitle"); dim: true; font.pixelSize: Style.font.body }
    }
    Seg {
      current: root.panel.settingsTab
      model: [{ key: "main", label: MihomoI18n.t("settingsTabMain") }, { key: "auto", label: MihomoI18n.t("settingsTabAuto") }, { key: "notify", label: MihomoI18n.t("settingsTabNotify") }, { key: "core", label: MihomoI18n.t("settingsTabCore") }, { key: "dns", label: "DNS" }]
      onPicked: function(k) { root.panel.settingsTab = k }
    }
  }

  AutomationSettings {
    id: automation
    visible: root.panel.settingsTab === "auto" || root.panel.settingsTab === "notify"
    section: root.panel.settingsTab
    anchors.top: header.bottom
    anchors.topMargin: Style.space(16)
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    svc: root.svc
    panel: root.panel
  }

  DnsSettings {
    id: dnsPage
    objectName: "dnsSettings"
    visible: root.panel.settingsTab === "dns"
    anchors.top: header.bottom
    anchors.topMargin: Style.space(16)
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    svc: root.svc
    panel: root.panel
  }

  CoreSettings {
    id: core
    objectName: "coreSettings"
    visible: root.panel.settingsTab === "core"
    anchors.top: header.bottom
    anchors.topMargin: Style.space(16)
    anchors.left: parent.left
    anchors.right: parent.right
    svc: root.svc
    panel: root.panel
    settings: root.s
    onSave: function(patch) { root.save(patch) }
  }

  Flickable {
    visible: root.s !== null && root.panel.settingsTab === "main"
    anchors.top: header.bottom
    anchors.topMargin: Style.space(16)
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    contentHeight: mainCol.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

  Column {
    id: mainCol
    width: parent.width - Style.space(8)
    spacing: Style.space(16)

    IssueList { width: parent.width; svc: root.svc; only: ["other_core", "core_no_caps", "tun_failed", "core_stopped"] }

    // Which window each mouse button opens on the bar icon.
    Repeater {
      model: [
        { key: "leftClick", label: MihomoI18n.t("leftClick"), hint: MihomoI18n.t("leftClickHint") },
        { key: "rightClick", label: MihomoI18n.t("rightClick"), hint: MihomoI18n.t("rightClickHint") }
      ]
      Row {
        required property var modelData
        width: parent.width
        Column {
          width: parent.width - clickSeg.width
          spacing: Style.space(2)
          Txt { text: modelData.label }
          Txt { width: parent.width - Style.space(16); text: modelData.hint; dim: true; font.pixelSize: Style.font.bodySmall; wrapMode: Text.WordWrap }
        }
        Seg {
          id: clickSeg
          objectName: modelData.key + "Setting"
          anchors.verticalCenter: parent.verticalCenter
          current: root.panel[modelData.key]
          model: [{ key: "compact", label: MihomoI18n.t("windowCompact") }, { key: "normal", label: MihomoI18n.t("windowNormal") }, { key: "large", label: MihomoI18n.t("windowLarge") }]
          onPicked: function(k) { root.setSetting(modelData.key, k) }
        }
      }
    }

    Repeater {
      model: root.s ? [
        { label: "TUN", hint: MihomoI18n.t("tunHint"), key: "tun", on: root.s.tun.enable },
        { label: MihomoI18n.t("allowLan"), hint: MihomoI18n.t("allowLanHint"), key: "allow-lan", on: root.s["allow-lan"] },
        { label: "IPv6", hint: MihomoI18n.t("ipv6Hint"), key: "ipv6", on: root.s.ipv6 },
        { label: MihomoI18n.t("processNames"), hint: MihomoI18n.t("processNamesHint"), key: "find-process-mode", on: root.s["find-process-mode"] === "always" }
      ] : []
      Row {
        required property var modelData
        width: parent.width
        Column {
          width: parent.width - sw.width
          spacing: Style.space(2)
          Txt { text: modelData.label }
          Txt { text: modelData.hint; dim: true; font.pixelSize: Style.font.bodySmall }
        }
        Mihomo.Switch {
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
      objectName: "trafficSetting"
      width: parent.width
      Column {
        width: parent.width - trafficClear.width - trafficSwitch.width - Style.space(12)
        spacing: Style.space(2)
        Txt { text: MihomoI18n.t("trafficSettings") }
        Txt {
          width: parent.width
          text: MihomoI18n.t("trafficSettingsHint") + (root.traffic && root.traffic.rows ? "  " + MihomoI18n.t("trafficSettingsSize").arg(root.traffic.rows).arg(root.svc.formatBytes(root.traffic.bytes)) : "")
          dim: true; font.pixelSize: Style.font.bodySmall
        }
      }
      Btn {
        id: trafficClear
        ghost: true
        anchors.verticalCenter: parent.verticalCenter
        text: MihomoI18n.t("trafficClear")
        enabled: root.traffic !== null && root.traffic.rows > 0
        onClicked: root.svc.request("traffic.clear", null, function(r, err) { if (r) root.traffic = r; root.panel.notify(err ? err.message : MihomoI18n.t("trafficCleared")) })
      }
      Item { width: Style.space(12); height: 1 }
      Mihomo.Switch {
        id: trafficSwitch
        anchors.verticalCenter: parent.verticalCenter
        checked: root.traffic === null || root.traffic.enabled
        onToggled: root.svc.request("traffic.set", { enabled: !(root.traffic === null || root.traffic.enabled) }, function(r) { if (r) root.traffic = r })
      }
    }

    Row {
      spacing: Style.space(16)
      Txt { text: MihomoI18n.t("tunStack"); width: Style.space(200); anchors.verticalCenter: parent.verticalCenter }
      Seg {
        current: root.s ? root.s.tun.stack : ""
        model: [{ key: "gvisor", label: "gVisor" }, { key: "system", label: "system" }, { key: "mixed", label: "mixed" }]
        onPicked: function(k) { root.save({ tun: { stack: k } }) }
      }
    }

    Row {
      spacing: Style.space(16)
      Txt { text: MihomoI18n.t("mixedPort"); width: Style.space(200); anchors.verticalCenter: parent.verticalCenter }
      Field { id: portField; width: Style.space(100); onAccepted: { var n = parseInt(text); if (n > 0 && n < 65536) root.save({ "mixed-port": n }); input.focus = false; root.panel.forceKeys() } }
      Txt { text: MihomoI18n.t("enterToSave"); faint: true; font.pixelSize: Style.font.bodySmall; anchors.verticalCenter: parent.verticalCenter }
    }

    Row {
      spacing: Style.space(16)
      Txt { text: MihomoI18n.t("updateInterval"); width: Style.space(200); anchors.verticalCenter: parent.verticalCenter }
      Field { id: intervalField; width: Style.space(100); onAccepted: { var n = parseInt(text); if (n >= 0) root.save({ "update-interval-hours": n }); input.focus = false; root.panel.forceKeys() } }
      Txt { text: MihomoI18n.t("hours0off"); faint: true; font.pixelSize: Style.font.bodySmall; anchors.verticalCenter: parent.verticalCenter }
    }

    Row {
      spacing: Style.space(16)
      Txt { text: MihomoI18n.t("language"); width: Style.space(200); anchors.verticalCenter: parent.verticalCenter }
      Seg {
        current: { var s = String((root.panel.settings || {}).locale || "auto"); return s === "ru" || s === "en" ? s : "auto" }
        model: [{ key: "auto", label: MihomoI18n.t("languageAuto") }, { key: "ru", label: "Русский" }, { key: "en", label: "English" }]
        onPicked: function(k) { root.setSetting("locale", k) }
      }
    }

    // One file for another machine or a reinstall.
    Column {
      width: parent.width
      spacing: Style.space(6)
      Txt { text: MihomoI18n.t("backupTitle") }
      Txt { width: parent.width; text: MihomoI18n.t("backupHint"); dim: true; font.pixelSize: Style.font.bodySmall; wrapMode: Text.Wrap }
      Row {
        width: parent.width
        spacing: Style.space(8)
        Field { id: backupPath; objectName: "backupPath"; width: parent.width - exportBtn.width - importBtn.width - Style.space(16); placeholder: MihomoI18n.t("backupPathHint") }
        Btn {
          id: exportBtn
          objectName: "backupExport"
          icon: Icons.down
          text: MihomoI18n.t("backupExport")
          busy: root.backupBusy === "export"
          enabled: root.backupBusy === ""
          onClicked: root.backup("backup.export", { path: backupPath.text.trim() || null, profiles: true })
        }
        Btn {
          id: importBtn
          objectName: "backupImport"
          icon: Icons.up
          text: MihomoI18n.t("backupImport")
          busy: root.backupBusy === "import"
          enabled: root.backupBusy === "" && backupPath.text.trim() !== ""
          onClicked: root.backup("backup.import", { path: backupPath.text.trim() })
        }
      }
    }

    Txt { width: parent.width; text: MihomoI18n.t("settingsFootnote"); faint: true; font.pixelSize: Style.font.bodySmall; wrapMode: Text.Wrap }
  }
  }
}
