import QtQuick
import qs.Commons
import qs.Commons as Commons

Rectangle {
  id: root
  property var problem: ({})
  property var svc
  property bool selected: false
  property bool grouped: false
  property bool header: false
  signal clicked()
  signal routeClicked()
  signal hideClicked()

  readonly property var kinds: Object.keys(problem.kinds || {})
  readonly property var apps: Object.keys(problem.apps || {})
  readonly property var route: problem.route || ({})
  readonly property bool recovered: problem.ok_since_error > 0
  // An IP-only entry reads better by its sniffed name, or at least by the domain its rule matched.
  readonly property string label: problem.name || problem.domain_hint || problem.host || ""
  readonly property bool named: !header && label !== (problem.host || "")
  readonly property real gap: Style.space(8)
  readonly property real kindWidth: Style.space(26)
  readonly property real appWidth: Style.space(104)
  readonly property real routeWidth: Style.space(132)
  readonly property real countWidth: Style.space(42)
  readonly property real actionWidth: Style.space(56)

  implicitHeight: Style.space(header ? 26 : (grouped ? 30 : 36))
  radius: Style.cornerRadius
  color: header ? "transparent" : selected ? Style.hoverFillFor(Commons.Color.popups.text, Commons.Color.accent) : rowHover.hovered ? Util.alpha(Commons.Color.popups.text, 0.06) : "transparent"
  border.width: selected && !header ? 1 : 0
  border.color: Util.alpha(Commons.Color.popups.text, 0.15)

  HoverHandler { id: rowHover; enabled: !root.header; cursorShape: Qt.PointingHandCursor }

  MouseArea {
    id: ma
    anchors.fill: parent
    enabled: !root.header
    onClicked: root.clicked()
    onDoubleClicked: root.routeClicked()
  }

  Row {
    id: cells
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.leftMargin: Style.space(root.grouped ? 33 : 10)
    anchors.rightMargin: Style.space(10)
    anchors.verticalCenter: parent.verticalCenter
    spacing: root.gap

    Item {
      id: hostCell
      width: root.grouped
        ? Math.min(hostLabel.implicitWidth + (root.named ? ipLabel.implicitWidth + Style.space(6) : 0) + Style.space(11), Math.max(0, cells.width - root.kindWidth - root.appWidth - Style.space(74) - root.countWidth - tail.width - root.gap * 5))
        : Math.max(0, cells.width - root.kindWidth - appCell.width - root.routeWidth - root.countWidth - tail.width - root.gap * 5)
      height: root.height
      Rectangle {
        id: dot
        visible: !root.header
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        width: Style.space(5); height: width; radius: width / 2
        color: root.problem["new"] ? Commons.Color.urgent : "transparent"
      }
      Txt {
        id: hostLabel
        anchors.left: parent.left
        anchors.leftMargin: root.header ? 0 : Style.space(11)
        anchors.verticalCenter: parent.verticalCenter
        width: Math.min(implicitWidth, parent.width - anchors.leftMargin - (root.named ? ipLabel.implicitWidth + Style.space(6) : 0))
        text: root.header ? MihomoI18n.t("problemHost") : root.label
        bold: !root.header && !root.grouped
        faint: root.header
        dim: !root.header && root.recovered
        font.pixelSize: root.header ? Style.font.bodySmall : Style.font.body
      }
      Txt {
        id: ipLabel
        visible: root.named
        anchors.left: hostLabel.right
        anchors.leftMargin: Style.space(6)
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        text: root.problem.host || ""
        faint: true
        font.pixelSize: Style.font.bodySmall
      }
      Hint {
        visible: hostHover.hovered && !root.header
        text: [root.problem.host, root.problem.name, root.problem.domain_hint && root.problem.domain_hint !== root.problem.name ? MihomoI18n.t("problemRuleDomain").arg(root.problem.domain_hint) : ""].filter(function(s) { return !!s }).join(" · ")
      }
      HoverHandler { id: hostHover }
    }

    Item {
      width: root.kindWidth; height: root.height
      Txt { visible: root.header; anchors.verticalCenter: parent.verticalCenter; text: MihomoI18n.t("problemKind"); faint: true; font.pixelSize: Style.font.bodySmall }
      Row {
        visible: !root.header
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(4)
        Repeater {
          model: root.kinds.slice(0, 2)
          Rectangle {
            required property string modelData
            width: Style.space(7); height: width; radius: width / 2
            color: root.svc.kindColor(modelData)
          }
        }
      }
      Hint {
        visible: kindHover.hovered && !root.header
        text: root.kinds.map(function(k) { return MihomoI18n.kind(k) + " ×" + root.problem.kinds[k] }).join("\n")
      }
      HoverHandler { id: kindHover }
    }

    Txt {
      id: appCell
      width: root.grouped ? Math.max(0, cells.width - hostCell.width - root.kindWidth - Style.space(74) - root.countWidth - tail.width - root.gap * 5) : root.appWidth
      height: root.height
      text: root.header ? MihomoI18n.t("problemApp") : (root.apps.length ? (root.grouped ? root.apps.join(", ") : root.apps[0] + (root.apps.length > 1 ? " +" + (root.apps.length - 1) : "")) : "—")
      faint: root.header || root.grouped || !root.apps.length
      dim: !faint
      font.pixelSize: root.header || root.grouped ? Style.font.bodySmall : Style.font.body
      Hint {
        visible: appHover.hovered && !root.header
        text: root.apps.join(", ") || MihomoI18n.t("unknownApp")
      }
      HoverHandler { id: appHover }
    }

    Txt {
      visible: !root.grouped
      width: root.routeWidth; height: root.height
      text: root.header ? MihomoI18n.t("problemRoute") : (root.route.rule ? root.route.rule + " → " : "") + (root.route.target || "—")
      faint: root.header; dim: !root.header
      font.pixelSize: root.header ? Style.font.bodySmall : Style.font.body
      Hint {
        visible: routeHover.hovered && !root.header
        text: [root.route.rule, root.route.payload, root.route.target].filter(function(v) { return !!v }).join(" · ")
      }
      HoverHandler { id: routeHover }
    }

    Txt {
      visible: root.grouped
      width: Style.space(74); height: root.height
      text: root.recovered ? MihomoI18n.t("problemOkCount").arg(root.problem.ok_since_error) : root.svc.ago(root.problem.last_seen)
      color: root.recovered ? root.svc.green : Util.alpha(Commons.Color.popups.text, 0.42)
      font.pixelSize: Style.font.bodySmall
      horizontalAlignment: Text.AlignRight
    }

    Txt {
      width: root.countWidth; height: root.height
      text: root.header ? MihomoI18n.t("problemFailures") : "×" + (root.problem.count || 0)
      color: root.header ? Util.alpha(Commons.Color.popups.text, 0.42) : (!root.grouped && !root.recovered ? root.svc.red : Commons.Color.popups.text)
      bold: !root.header
      font.pixelSize: root.header ? Style.font.bodySmall : Style.font.body
      horizontalAlignment: Text.AlignRight
    }

    Item {
      id: tail
      width: root.grouped ? Style.space(24) : root.actionWidth
      height: root.height
      Txt {
        visible: !root.header && !root.grouped && !root.selected
        anchors.fill: parent
        text: root.recovered ? Icons.check + " " + MihomoI18n.t("problemOk") : root.svc.ago(root.problem.last_seen)
        color: root.recovered ? root.svc.green : Util.alpha(Commons.Color.popups.text, 0.42)
        font.pixelSize: Style.font.bodySmall
        horizontalAlignment: Text.AlignRight
      }
      Row {
        visible: !root.header && (root.grouped || root.selected)
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(4)
        Btn { visible: !root.grouped; width: Style.space(24); height: width; icon: Icons.route; tooltip: MihomoI18n.t("hintRoute"); onClicked: root.routeClicked() }
        Btn { width: Style.space(24); height: width; ghost: root.grouped; icon: root.problem.hidden ? Icons.show : Icons.hide; tooltip: MihomoI18n.t(root.problem.hidden ? "hintShowProblem" : "hintHideProblem"); onClicked: root.hideClicked() }
      }
    }
  }

  Rectangle { visible: root.header; anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Util.alpha(Commons.Color.popups.text, 0.15) }
}
