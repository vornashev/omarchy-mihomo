import QtQuick
import "." as Mihomo
import qs.Commons

Row {
  id: root
  property var svc
  Column {
    width: parent.width - systemProxySwitch.width
    spacing: Style.space(2)
    Txt { text: I18n.t("systemProxy") }
    Txt {
      width: parent.width - Style.space(12)
      text: root.svc.systemProxyError || (root.svc.systemProxyBusy ? I18n.t("systemProxyChanging") : root.svc.systemProxyWanted && !root.svc.systemProxy ? I18n.t("systemProxyWaiting") : I18n.t("systemProxyHint"))
      color: root.svc.systemProxyError ? root.svc.red : Color.muted
      font.pixelSize: Style.font.caption
      wrapMode: Text.Wrap
    }
  }
  Mihomo.Switch {
    id: systemProxySwitch
    objectName: "systemProxySwitch"
    anchors.verticalCenter: parent.verticalCenter
    checked: root.svc.systemProxy
    enabled: root.svc.connected && !root.svc.systemProxyBusy && (root.svc.systemProxy || root.svc.systemProxyWanted || (root.svc.systemProxyAvailable && root.svc.coreRunning))
    opacity: enabled ? 1 : 0.5
    onToggled: root.svc.toggleSystemProxy()
  }
}
