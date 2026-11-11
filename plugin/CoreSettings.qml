import QtQuick
import qs.Commons
import "." as Mihomo
import qs.Commons as Commons

// The core binary and its databases: versions, updates, rollback.
Column {
  id: root
  property var svc
  property var panel
  // Managed settings from the page: the geo keys live there.
  property var settings: null
  property var info: null
  // "check", "geo" or "rollback" while that request runs.
  property string busy: ""
  readonly property bool typing: geoInterval.input.activeFocus
  readonly property bool updating: !!(svc && svc.fixing && svc.fixing.update_core)
  signal save(var patch)
  spacing: Style.space(16)

  function reload() { svc.request("core.updates", null, function(r) { if (r) root.info = r }) }
  function call(method, key, done) {
    busy = key
    svc.request(method, null, function(r, err) {
      root.busy = ""
      if (r) root.info = r
      root.panel.notify(err ? err.message : done)
    }, true)
  }
  function day(ms) { return ms ? Qt.formatDateTime(new Date(ms), "dd.MM.yyyy HH:mm") : "" }

  Component.onCompleted: reload()
  onVisibleChanged: if (visible) reload()
  onSettingsChanged: if (settings) geoInterval.text = String(settings["geo-update-interval"] || 24)
  // An update or a restart shows up in the status first.
  Connections {
    target: root.svc
    function onStatusChanged() { if (root.visible) reloadTimer.restart() }
  }
  Timer { id: reloadTimer; interval: 400; onTriggered: root.reload() }

  // ------------------------------------------------------------- core
  Column {
    width: parent.width
    spacing: Style.space(8)
    Txt { text: MihomoI18n.t("coreTitle"); bold: true }
    Txt {
      objectName: "coreVersionLine"
      width: parent.width
      wrapMode: Text.Wrap
      dim: true
      font.pixelSize: Style.font.body
      text: {
        var i = root.info
        if (!i) return "…"
        var parts = [i.current ? MihomoI18n.t("coreVersion").arg(i.current) : MihomoI18n.t("coreNotRunning")]
        if (i.available) parts.push(MihomoI18n.t("coreAvailable").arg(i.available))
        else if (i.latest && i.current) parts.push(MihomoI18n.t("coreUpToDate"))
        if (i.checked_at) parts.push(MihomoI18n.t("coreChecked").arg(root.day(i.checked_at)))
        return parts.join(" · ")
      }
    }
    Txt {
      visible: !!root.info && !root.info.supported
      width: parent.width; wrapMode: Text.Wrap
      text: MihomoI18n.t("coreNotOurs"); faint: true; font.pixelSize: Style.font.bodySmall
    }
    Txt {
      visible: !!root.info && !!root.info.error
      width: parent.width; wrapMode: Text.Wrap
      text: root.info && root.info.error ? MihomoI18n.t("coreCheckFailed").arg(root.info.error) : ""
      color: root.svc.yellow; font.pixelSize: Style.font.bodySmall
    }
    Row {
      visible: !!root.info && root.info.supported
      spacing: Style.space(8)
      Btn {
        objectName: "coreUpdateBtn"
        visible: !!root.info && !!root.info.available && !!root.info.fix
        primary: true
        icon: Icons.down
        busy: root.updating
        text: root.info && root.info.available ? MihomoI18n.t("coreUpdateTo").arg(root.info.available) : ""
        tooltip: root.info && root.info.fix && root.info.fix.privileged ? MihomoI18n.t("coreUpdatePrivileged") : text
        onClicked: root.svc.runFix(root.info.fix)
      }
      Btn {
        objectName: "coreCheckBtn"
        ghost: !!root.info && !!root.info.available
        icon: Icons.refresh
        busy: root.busy === "check"
        enabled: root.busy === ""
        text: MihomoI18n.t("coreCheck")
        onClicked: root.call("core.check_updates", "check", MihomoI18n.t("coreChecked").arg(root.day(Date.now())))
      }
      Btn {
        objectName: "coreRollbackBtn"
        visible: !!root.info && !!root.info.previous
        ghost: true
        icon: Icons.undo
        busy: root.busy === "rollback"
        enabled: root.busy === "" && !root.updating
        text: root.info && root.info.previous ? MihomoI18n.t("coreRollback").arg(root.info.previous) : ""
        onClicked: root.call("core.rollback", "rollback", MihomoI18n.t("coreRolledBack"))
      }
    }
  }

  Rectangle { width: parent.width; height: 1; color: Util.alpha(Commons.Color.popups.text, 0.12) }

  // -------------------------------------------------------- databases
  Column {
    width: parent.width
    spacing: Style.space(8)
    Txt { text: MihomoI18n.t("geoTitle"); bold: true }
    Txt {
      visible: !!root.info && root.info.geo.length === 0
      width: parent.width; wrapMode: Text.Wrap
      text: MihomoI18n.t("geoEmpty"); faint: true; font.pixelSize: Style.font.bodySmall
    }
    Repeater {
      model: root.info ? root.info.geo : []
      Row {
        required property var modelData
        width: parent.width
        spacing: Style.space(12)
        Txt { width: Style.space(140); text: modelData.name }
        Txt { width: Style.space(90); text: root.svc.formatBytes(modelData.size); dim: true; font.pixelSize: Style.font.body }
        Txt { text: root.day(modelData.modified); dim: true; font.pixelSize: Style.font.body }
      }
    }
  }

  Row {
    width: parent.width
    Column {
      width: parent.width - geoSwitch.width
      spacing: Style.space(2)
      Txt { text: MihomoI18n.t("geoAuto") }
      Txt { text: MihomoI18n.t("geoAutoHint"); dim: true; font.pixelSize: Style.font.bodySmall }
    }
    Mihomo.Switch {
      id: geoSwitch
      objectName: "geoAutoSwitch"
      anchors.verticalCenter: parent.verticalCenter
      checked: !!root.settings && root.settings["geo-auto-update"] !== false
      onToggled: root.save({ "geo-auto-update": !(root.settings && root.settings["geo-auto-update"] !== false) })
    }
  }

  Row {
    spacing: Style.space(16)
    Txt { text: MihomoI18n.t("geoInterval"); width: Style.space(200); anchors.verticalCenter: parent.verticalCenter }
    Field {
      id: geoInterval
      width: Style.space(100)
      onAccepted: { var n = parseInt(text); if (n > 0) root.save({ "geo-update-interval": n }); input.focus = false; root.panel.forceKeys() }
    }
    Txt { text: MihomoI18n.t("enterToSave"); faint: true; font.pixelSize: Style.font.bodySmall; anchors.verticalCenter: parent.verticalCenter }
  }

  Btn {
    objectName: "geoNowBtn"
    visible: !!root.info && root.info.supported
    icon: Icons.refresh
    busy: root.busy === "geo" || (!!root.info && !!root.info.geo_updating)
    enabled: root.busy === ""
    text: MihomoI18n.t("geoNow")
    onClicked: root.call("core.update_geo", "geo", MihomoI18n.t("geoUpdated"))
  }
}
