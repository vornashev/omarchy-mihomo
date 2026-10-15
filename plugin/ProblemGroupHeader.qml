import QtQuick
import qs.Commons

Item {
  id: root
  property var group: ({ route: {}, items: [], kinds: {}, apps: {} })
  property var svc
  property bool collapsed: false
  signal toggled()
  signal routeClicked()
  implicitHeight: Style.space(58)

  MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.toggled() }
  Txt {
    id: chevron
    anchors.left: parent.left; anchors.leftMargin: Style.space(10)
    anchors.verticalCenter: parent.verticalCenter
    text: root.collapsed ? Icons.collapsed : Icons.expanded
    faint: true
  }
  Column {
    anchors.left: chevron.right; anchors.leftMargin: Style.space(10)
    anchors.right: total.left; anchors.rightMargin: Style.space(10)
    anchors.verticalCenter: parent.verticalCenter
    spacing: Style.space(3)
    Row {
      width: parent.width
      spacing: Style.space(8)
      Txt { id: routeIcon; text: Icons.route; color: Color.accent }
      Txt {
        width: Math.min(implicitWidth, parent.width - routeIcon.width - parent.spacing)
        text: (root.group.route.rule ? root.group.route.rule + " → " : "") + (root.group.route.target || "—")
        bold: true
      }
      Txt {
        width: Math.max(0, parent.width - x)
        text: root.group.route.payload || ""
        faint: true; font.pixelSize: Style.font.bodySmall
      }
    }
    Row {
      width: parent.width
      spacing: Style.space(8)
      Row {
        id: kinds
        spacing: Style.space(8)
        Repeater {
          model: Object.keys(root.group.kinds).slice(0, 2)
          Row {
            required property string modelData
            spacing: Style.space(4)
            Rectangle { anchors.verticalCenter: parent.verticalCenter; width: Style.space(6); height: width; radius: width / 2; color: root.svc.kindColor(modelData) }
            Txt { text: I18n.kind(modelData); dim: true; font.pixelSize: Style.font.caption }
          }
        }
      }
      Txt {
        width: Math.max(0, parent.width - kinds.width - parent.spacing)
        text: "· " + I18n.t("problemAddresses").arg(root.group.items.length) + " · " + (Object.keys(root.group.apps).join(", ") || I18n.t("unknownApp"))
        faint: true; font.pixelSize: Style.font.caption
      }
    }
  }
  Txt {
    id: total
    anchors.right: routeButton.left; anchors.rightMargin: Style.space(10)
    anchors.verticalCenter: parent.verticalCenter
    text: "×" + root.group.count
    bold: true
    color: root.group.unresolved ? root.svc.red : Color.popups.text
  }
  Btn {
    id: routeButton
    anchors.right: parent.right; anchors.rightMargin: Style.space(10)
    anchors.verticalCenter: parent.verticalCenter
    icon: Icons.route; text: I18n.t("route")
    onClicked: root.routeClicked()
  }
  Rectangle { visible: !root.collapsed; anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Util.alpha(Color.popups.text, 0.15) }
}
