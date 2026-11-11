import QtQuick
import qs.Commons
import qs.Commons as Commons

// Footer of the large window: the keys that work everywhere and the version.
Rectangle {
  id: root
  property var svc

  implicitHeight: Style.space(36)
  color: Util.alpha(Commons.Color.popups.background, 0.6)

  Rectangle {
    anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
    height: 1
    color: Util.alpha(Commons.Color.popups.text, 0.1)
  }

  Row {
    anchors.left: parent.left
    anchors.leftMargin: Style.space(16)
    anchors.verticalCenter: parent.verticalCenter
    spacing: Style.space(16)
    Repeater {
      model: [["↑↓", MihomoI18n.t("footerPick")], ["1–9", MihomoI18n.t("footerSection")], ["/", MihomoI18n.t("footerSearch")], ["Enter", MihomoI18n.t("footerOpen")], ["esc", MihomoI18n.t("footerClose")]]
      KeyHint {
        required property var modelData
        key: modelData[0]
        label: modelData[1]
      }
    }
  }

  Txt {
    objectName: "footerVersion"
    anchors.right: parent.right
    anchors.rightMargin: Style.space(16)
    anchors.verticalCenter: parent.verticalCenter
    text: root.svc.status && root.svc.status.daemon_version ? "omarchy-mihomo " + root.svc.status.daemon_version : ""
    faint: true; font.pixelSize: Style.font.bodySmall
  }
}
