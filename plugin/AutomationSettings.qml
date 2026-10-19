import QtQuick
import QtQuick.Controls
import "." as Mihomo
import qs.Commons

// Settings → Automation: autofix and which events notify.
Item {
  id: root
  property var svc
  property var panel

  property var a: svc.automation
  property var groups: []
  readonly property bool typing: failuresField.input.activeFocus || windowField.input.activeFocus || ignoreField.input.activeFocus

  function save(patch) {
    var next = Object.assign({}, a || {}, patch)
    svc.request("automation.set", next, function(r, err) {
      if (err) { panel.notify(err.message); return }
      svc.automation = r
      panel.notify(I18n.t("settingsSaved"))
    }, true)
  }

  function sync() {
    if (!a) return
    failuresField.text = String(a.failures)
    windowField.text = String(a.window_min)
    ignoreField.text = (a.ignore || []).join(", ")
  }

  onAChanged: sync()
  Component.onCompleted: {
    sync()
    svc.request("proxies.targets", null, function(r) { if (r) groups = r.filter(function(g) { return g !== "DIRECT" }) })
  }

  component Label: Txt {
    width: Style.space(190)
    anchors.verticalCenter: parent.verticalCenter
  }

  Flickable {
    anchors.fill: parent
    contentHeight: col.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

    Column {
      id: col
      visible: root.a !== null
      width: parent.width - Style.space(6)
      spacing: Style.space(14)

      Txt { text: I18n.t("autoSection"); faint: true; bold: true; font.pixelSize: Style.font.caption }

      Row {
        width: parent.width
        Column {
          width: parent.width - enableSwitch.width
          spacing: Style.space(2)
          Txt { text: I18n.t("autoEnable") }
          Txt { width: parent.width - Style.space(16); text: I18n.t("autoEnableHint"); dim: true; font.pixelSize: Style.font.caption; wrapMode: Text.Wrap }
        }
        Mihomo.Switch {
          id: enableSwitch
          anchors.verticalCenter: parent.verticalCenter
          checked: !!root.a && root.a.autofix
          onToggled: root.save({ autofix: !root.a.autofix })
        }
      }

      Column {
        width: parent.width
        spacing: Style.space(6)
        opacity: root.a && root.a.autofix ? 1 : 0.45
        enabled: !!root.a && root.a.autofix
        Row {
          spacing: Style.space(12)
          Label { text: I18n.t("autoMode") }
          Seg {
            current: root.a ? root.a.mode : ""
            model: ["show", "suggest", "trial"].map(function(k) { return { key: k, label: I18n.t("autoMode_" + k) } })
            onPicked: function(k) { root.save({ mode: k }) }
          }
        }
        Txt {
          x: Style.space(202)
          width: parent.width - x
          text: root.a ? I18n.t("autoModeHint_" + root.a.mode) : ""
          faint: true; font.pixelSize: Style.font.caption; wrapMode: Text.Wrap
        }
      }

      Row {
        spacing: Style.space(10)
        enabled: !!root.a && root.a.autofix
        opacity: enabled ? 1 : 0.45
        Label { text: I18n.t("autoThreshold") }
        Field {
          id: failuresField
          width: Style.space(48)
          onAccepted: { var n = parseInt(text); if (n > 0) root.save({ failures: n }); input.focus = false; root.panel.forceKeys() }
          onEscaped: { root.sync(); input.focus = false; root.panel.forceKeys() }
        }
        Txt { text: I18n.t("autoFailuresIn"); dim: true; anchors.verticalCenter: parent.verticalCenter }
        Field {
          id: windowField
          width: Style.space(48)
          onAccepted: { var n = parseInt(text); if (n > 0) root.save({ window_min: n }); input.focus = false; root.panel.forceKeys() }
          onEscaped: { root.sync(); input.focus = false; root.panel.forceKeys() }
        }
        Txt { text: I18n.t("autoMinutes") + " · " + I18n.t("autoThresholdHint"); faint: true; font.pixelSize: Style.font.caption; anchors.verticalCenter: parent.verticalCenter }
      }

      Row {
        spacing: Style.space(12)
        enabled: !!root.a && root.a.autofix && root.a.mode !== "show"
        opacity: enabled ? 1 : 0.45
        Label { text: I18n.t("autoTrial") }
        Seg {
          current: root.a ? String(root.a.trial_min) : ""
          model: [{ key: "30", label: I18n.t("autoTrial30") }, { key: "60", label: I18n.t("autoTrial60") }, { key: "180", label: I18n.t("autoTrial180") }]
          onPicked: function(k) { root.save({ trial_min: parseInt(k) }) }
        }
      }

      Row {
        width: parent.width
        spacing: Style.space(12)
        enabled: !!root.a && root.a.autofix
        opacity: enabled ? 1 : 0.45
        Label { text: I18n.t("autoExclude"); anchors.verticalCenter: undefined }
        Flow {
          width: parent.width - Style.space(202)
          spacing: Style.space(6)
          Repeater {
            model: root.groups
            Chip {
              required property string modelData
              readonly property bool excluded: !!root.a && (root.a.exclude_groups || []).indexOf(modelData) !== -1
              text: modelData
              dot: excluded ? Color.muted : root.svc.green
              opacity: excluded ? 0.55 : 1
              MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: {
                  var list = (root.a.exclude_groups || []).slice()
                  var i = list.indexOf(parent.modelData)
                  if (i === -1) list.push(parent.modelData); else list.splice(i, 1)
                  root.save({ exclude_groups: list })
                }
              }
            }
          }
          Txt { text: I18n.t("autoExcludeHint"); faint: true; font.pixelSize: Style.font.caption; height: Style.space(20) }
        }
      }

      Row {
        width: parent.width
        spacing: Style.space(12)
        enabled: !!root.a && root.a.autofix
        opacity: enabled ? 1 : 0.45
        Label { text: I18n.t("autoIgnore") }
        Field {
          id: ignoreField
          width: parent.width - Style.space(202)
          placeholder: I18n.t("autoIgnoreHint")
          onAccepted: {
            var list = text.split(",").map(function(x) { return x.trim() }).filter(function(x) { return x !== "" })
            root.save({ ignore: list })
            input.focus = false
            root.panel.forceKeys()
          }
          onEscaped: { root.sync(); input.focus = false; root.panel.forceKeys() }
        }
      }

      Rectangle { width: parent.width; height: 1; color: Util.alpha(Color.popups.text, 0.12) }

      Row {
        spacing: Style.space(8)
        Txt { text: I18n.t("notifSection"); faint: true; bold: true; font.pixelSize: Style.font.caption }
        Txt { text: I18n.t("notifSectionHint"); faint: true; font.pixelSize: Style.font.caption }
      }

      Grid {
        width: parent.width
        columns: 2
        columnSpacing: Style.space(24)
        rowSpacing: Style.space(10)
        Repeater {
          model: ["core", "dead_group", "subscription", "orphans", "autofix", "saved"]
          Item {
            id: notifyItem
            required property string modelData
            readonly property bool on: !!root.a && !!root.a.notify && root.a.notify[modelData] !== false
            width: (col.width - Style.space(24)) / 2
            height: notifyRow.implicitHeight
          Row {
            id: notifyRow
            width: parent.width
            spacing: Style.space(8)
            Rectangle {
              width: Style.space(14); height: width
              radius: Style.space(3)
              anchors.top: parent.top; anchors.topMargin: Style.space(2)
              color: notifyItem.on ? Color.accent : "transparent"
              border.width: notifyItem.on ? 0 : 1
              border.color: Util.alpha(Color.popups.text, 0.4)
              Txt { anchors.centerIn: parent; visible: notifyItem.on; text: Icons.check; color: Color.background; font.pixelSize: Style.font.caption }
            }
            Column {
              width: parent.width - Style.space(22)
              spacing: Style.space(1)
              Txt { width: parent.width; text: I18n.t("notify_" + notifyItem.modelData); dim: !notifyItem.on; font.pixelSize: Style.font.bodySmall }
              Txt { width: parent.width; text: I18n.t("notifyHint_" + notifyItem.modelData); faint: true; font.pixelSize: Style.font.caption }
            }
          }
            MouseArea {
              anchors.fill: parent
              cursorShape: Qt.PointingHandCursor
              onClicked: {
                var n = Object.assign({}, root.a.notify || {})
                n[notifyItem.modelData] = !notifyItem.on
                root.save({ notify: n })
              }
            }
          }
        }
      }
    }
  }
}
