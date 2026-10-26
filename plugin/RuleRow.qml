import QtQuick
import "." as Mihomo
import qs.Commons

Rectangle {
  id: root
  property var rule: ({})
  property var svc: null
  property bool selected: false
  signal toggled()
  signal editClicked()
  signal removeClicked()
  signal clicked()

  readonly property bool orphan: !!rule.orphan
  readonly property bool on: !!rule.enabled
  readonly property string effectiveTarget: rule.effective_target || rule.target || ""
  readonly property color targetColor: orphan ? (svc ? svc.yellow : Color.urgent) : (!on ? Color.muted : (effectiveTarget === "DIRECT" ? (svc ? svc.green : Color.accent) : Color.accent))

  implicitHeight: Style.space(46)
  radius: Style.cornerRadius
  color: orphan ? Util.alpha(targetColor, 0.06) : (selected ? Style.hoverFillFor(Color.popups.text, Color.accent) : (ma.containsMouse ? Util.alpha(Color.popups.text, 0.04) : "transparent"))
  border.width: selected ? 1 : 0
  border.color: Util.alpha(Color.popups.text, 0.15)

  MouseArea { id: ma; anchors.fill: parent; hoverEnabled: true; onClicked: root.clicked(); onDoubleClicked: root.editClicked() }

  Mihomo.Switch {
    id: sw
    anchors.left: parent.left; anchors.leftMargin: Style.space(10)
    anchors.verticalCenter: parent.verticalCenter
    compact: true
    checked: root.on
    onToggled: root.toggled()
  }

  Column {
    anchors.left: sw.right; anchors.leftMargin: Style.space(12)
    anchors.right: tail.left; anchors.rightMargin: Style.space(10)
    anchors.verticalCenter: parent.verticalCenter
    spacing: Style.space(3)
    Row {
      spacing: Style.space(8)
      Txt { text: I18n.matchKind(root.rule.kind); color: root.svc ? root.svc.mauve : Color.accent; bold: true; font.pixelSize: Style.font.caption }
      Txt {
        width: Math.min(implicitWidth, root.width - Style.space(260))
        text: root.rule.parts && root.rule.parts.length
          ? root.rule.parts.map(function(p) { return I18n.matchKind(p.kind) + " " + p.value }).join(root.rule.kind === "or" ? "  |  " : "  &  ")
          : root.rule.value
        bold: true; font.strikeout: !root.on; dim: !root.on
      }
    }
    Txt {
      width: parent.width
      dim: !root.orphan
      color: root.orphan ? root.targetColor : Util.alpha(Color.popups.text, 0.68)
      font.pixelSize: Style.font.bodySmall
      text: {
        if (root.orphan) return I18n.t("orphanRow")
        var parts = []
        if (root.rule.source && root.rule.source !== "user") parts.push(I18n.t("importedFrom").arg(root.rule.source))
        else if (root.svc) parts.push(I18n.t("added").arg(root.svc.clock(root.rule.created)))
        if (root.rule.test && root.rule.test.ok) parts.push(I18n.t("checkedMs").arg(root.rule.test.delay))
        if (root.rule.hits !== null && root.rule.hits !== undefined) parts.push(I18n.t("hits").arg(root.rule.hits))
        if (root.rule.traffic && root.svc) parts.push(I18n.t("trafficWeek").arg(root.svc.formatBytes(root.rule.traffic.up + root.rule.traffic.down)))
        if (root.rule.boot) parts.push(Icons.timer + " " + I18n.t("ruleUntilBoot"))
        else if (root.rule.expires && root.svc) parts.push(Icons.timer + " " + I18n.t("ruleUntil").arg(root.svc.clock(root.rule.expires)))
        if (root.rule.note) parts.push(root.rule.note)
        return parts.join("  ·  ")
      }
    }
  }

  Row {
    id: tail
    anchors.right: parent.right; anchors.rightMargin: Style.space(8)
    anchors.verticalCenter: parent.verticalCenter
    spacing: Style.space(4)
    Rectangle {
      anchors.verticalCenter: parent.verticalCenter
      implicitWidth: tr.implicitWidth + Style.space(16); implicitHeight: Style.space(24)
      radius: Style.cornerRadius
      color: Util.alpha(root.targetColor, 0.1)
      Row {
        id: tr
        anchors.centerIn: parent
        spacing: Style.space(5)
        Txt { text: root.orphan ? Icons.orphan : Icons.arrow; color: root.targetColor }
        Txt { text: root.effectiveTarget; color: root.targetColor; bold: true }
      }
      Hint {
        visible: routeHover.containsMouse && !routeHover.pressed
        text: I18n.t("hintEditRule")
      }
      MouseArea { id: routeHover; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.editClicked() }
    }
    Btn { anchors.verticalCenter: parent.verticalCenter; ghost: true; icon: Icons.remove; tooltip: I18n.t("hintDeleteRule"); onClicked: root.removeClicked() }
  }
}
