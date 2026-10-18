import QtQuick
import QtQuick.Controls
import qs.Commons

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
  readonly property real gap: Style.space(8)
  readonly property real kindWidth: Style.space(26)
  readonly property real appWidth: Style.space(104)
  readonly property real routeWidth: Style.space(132)
  readonly property real countWidth: Style.space(42)
  readonly property real actionWidth: Style.space(56)

  implicitHeight: Style.space(header ? 26 : (grouped ? 30 : 36))
  radius: Style.cornerRadius
  color: header ? "transparent" : selected ? Style.hoverFillFor(Color.popups.text, Color.accent) : ma.containsMouse ? Util.alpha(Color.popups.text, 0.04) : "transparent"
  border.width: selected && !header ? 1 : 0
  border.color: Util.alpha(Color.popups.text, 0.15)

  MouseArea {
    id: ma
    anchors.fill: parent
    hoverEnabled: true
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
        ? Math.min(hostLabel.implicitWidth + Style.space(11), Math.max(0, cells.width - root.kindWidth - root.appWidth - Style.space(74) - root.countWidth - tail.width - root.gap * 5))
        : Math.max(0, cells.width - root.kindWidth - appCell.width - root.routeWidth - root.countWidth - tail.width - root.gap * 5)
      height: root.height
      Rectangle {
        id: dot
        visible: !root.header
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        width: Style.space(5); height: width; radius: width / 2
        color: root.problem["new"] ? Color.urgent : "transparent"
      }
      Txt {
        id: hostLabel
        anchors.left: parent.left
        anchors.leftMargin: root.header ? 0 : Style.space(11)
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        text: root.header ? I18n.t("problemHost") : (root.problem.host || "")
        bold: !root.header && !root.grouped
        faint: root.header
        dim: !root.header && root.recovered
        font.pixelSize: root.header ? Style.font.caption : Style.font.bodySmall
      }
      Hint {
        visible: hostHover.containsMouse && !root.header
        text: (root.problem.host || "") + (root.problem.domain_hint ? " · " + root.problem.domain_hint : "")
      }
      MouseArea { id: hostHover; anchors.fill: parent; acceptedButtons: Qt.NoButton; hoverEnabled: true }
    }

    Item {
      width: root.kindWidth; height: root.height
      Txt { visible: root.header; anchors.verticalCenter: parent.verticalCenter; text: I18n.t("problemKind"); faint: true; font.pixelSize: Style.font.caption }
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
        visible: kindHover.containsMouse && !root.header
        text: root.kinds.map(function(k) { return I18n.kind(k) + " ×" + root.problem.kinds[k] }).join("\n")
      }
      MouseArea { id: kindHover; anchors.fill: parent; acceptedButtons: Qt.NoButton; hoverEnabled: true }
    }

    Txt {
      id: appCell
      width: root.grouped ? Math.max(0, cells.width - hostCell.width - root.kindWidth - Style.space(74) - root.countWidth - tail.width - root.gap * 5) : root.appWidth
      height: root.height
      text: root.header ? I18n.t("problemApp") : (root.apps.length ? (root.grouped ? root.apps.join(", ") : root.apps[0] + (root.apps.length > 1 ? " +" + (root.apps.length - 1) : "")) : "—")
      faint: root.header || root.grouped || !root.apps.length
      dim: !faint
      font.pixelSize: root.header || root.grouped ? Style.font.caption : Style.font.bodySmall
      Hint {
        visible: appHover.containsMouse && !root.header
        text: root.apps.join(", ") || I18n.t("unknownApp")
      }
      MouseArea { id: appHover; anchors.fill: parent; acceptedButtons: Qt.NoButton; hoverEnabled: true }
    }

    Txt {
      visible: !root.grouped
      width: root.routeWidth; height: root.height
      text: root.header ? I18n.t("problemRoute") : (root.route.rule ? root.route.rule + " → " : "") + (root.route.target || "—")
      faint: root.header; dim: !root.header
      font.pixelSize: root.header ? Style.font.caption : Style.font.bodySmall
      Hint {
        visible: routeHover.containsMouse && !root.header
        text: [root.route.rule, root.route.payload, root.route.target].filter(function(v) { return !!v }).join(" · ")
      }
      MouseArea { id: routeHover; anchors.fill: parent; acceptedButtons: Qt.NoButton; hoverEnabled: true }
    }

    Txt {
      visible: root.grouped
      width: Style.space(74); height: root.height
      text: root.recovered ? I18n.t("problemOkCount").arg(root.problem.ok_since_error) : root.svc.ago(root.problem.last_seen)
      color: root.recovered ? root.svc.green : Util.alpha(Color.popups.text, 0.42)
      font.pixelSize: Style.font.caption
      horizontalAlignment: Text.AlignRight
    }

    Txt {
      width: root.countWidth; height: root.height
      text: root.header ? I18n.t("problemFailures") : "×" + (root.problem.count || 0)
      color: root.header ? Util.alpha(Color.popups.text, 0.42) : (!root.grouped && !root.recovered ? root.svc.red : Color.popups.text)
      bold: !root.header
      font.pixelSize: root.header ? Style.font.caption : Style.font.body
      horizontalAlignment: Text.AlignRight
    }

    Item {
      id: tail
      width: root.grouped ? Style.space(24) : root.actionWidth
      height: root.height
      Txt {
        visible: !root.header && !root.grouped && !root.selected
        anchors.fill: parent
        text: root.recovered ? Icons.check + " " + I18n.t("problemOk") : root.svc.ago(root.problem.last_seen)
        color: root.recovered ? root.svc.green : Util.alpha(Color.popups.text, 0.42)
        font.pixelSize: Style.font.caption
        horizontalAlignment: Text.AlignRight
      }
      Row {
        visible: !root.header && (root.grouped || root.selected)
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(4)
        Btn { visible: !root.grouped; width: Style.space(24); height: width; icon: Icons.route; tooltip: I18n.t("hintRoute"); onClicked: root.routeClicked() }
        Btn { width: Style.space(24); height: width; ghost: root.grouped; icon: root.problem.hidden ? Icons.show : Icons.hide; tooltip: I18n.t(root.problem.hidden ? "hintShowProblem" : "hintHideProblem"); onClicked: root.hideClicked() }
      }
    }
  }

  Rectangle { visible: root.header; anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Util.alpha(Color.popups.text, 0.15) }
}
