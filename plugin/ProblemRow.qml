import QtQuick
import qs.Commons

// One problem resource: host, error kinds, app, route, repeats.
Rectangle {
  id: root
  property var problem: ({})
  property var svc: null
  property bool selected: false
  property bool compact: false
  signal routeClicked()
  signal hideClicked()
  signal clicked()

  readonly property var kinds: problem && problem.kinds ? Object.keys(problem.kinds) : []
  readonly property var apps: problem && problem.apps ? Object.keys(problem.apps) : []
  readonly property var route: problem && problem.route ? problem.route : ({})

  implicitHeight: Style.space(46)
  radius: Style.cornerRadius
  color: selected ? Style.hoverFillFor(Color.popups.text, Color.accent) : (ma.containsMouse ? Util.alpha(Color.popups.text, 0.04) : "transparent")
  border.width: selected ? 1 : 0
  border.color: Util.alpha(Color.popups.text, 0.15)

  MouseArea { id: ma; anchors.fill: parent; hoverEnabled: true; onClicked: root.clicked(); onDoubleClicked: root.routeClicked() }

  Rectangle {
    id: dot
    anchors.left: parent.left; anchors.leftMargin: Style.space(10)
    anchors.verticalCenter: parent.verticalCenter
    width: Style.space(6); height: width; radius: width / 2
    color: root.problem && root.problem["new"] ? Color.urgent : "transparent"
  }

  Column {
    anchors.left: dot.right; anchors.leftMargin: Style.space(10)
    anchors.right: tail.left; anchors.rightMargin: Style.space(10)
    anchors.verticalCenter: parent.verticalCenter
    spacing: Style.space(3)
    Row {
      spacing: Style.space(8)
      width: parent.width
      Txt {
        id: hostText
        text: root.problem.host + (root.problem.domain_hint ? "  (" + root.problem.domain_hint + ")" : "")
        bold: true
        width: Math.min(implicitWidth, parent.width - chips.width - Style.space(8))
      }
      Row {
        id: chips
        spacing: Style.space(4)
        Repeater {
          model: root.kinds.slice(0, 2)
          Chip {
            required property string modelData
            text: I18n.kind(modelData)
            dot: root.svc ? root.svc.kindColor(modelData) : Color.urgent
          }
        }
      }
    }
    Txt {
      width: parent.width
      dim: true
      font.pixelSize: Style.font.bodySmall
      text: {
        var parts = []
        parts.push(Icons.app + " " + (root.apps.length ? root.apps.slice(0, 2).join(", ") : I18n.t("unknownApp")))
        var r = root.route.rule ? root.route.rule + " → " + root.route.target : root.route.target
        if (r) parts.push(Icons.route + " " + r)
        if (root.problem.ok_since_error > 0) parts.push(I18n.t("okSince").arg(root.problem.ok_since_error))
        else if (root.svc) parts.push(root.svc.ago(root.problem.last_seen))
        return parts.join("  ·  ")
      }
    }
  }

  Row {
    id: tail
    anchors.right: parent.right; anchors.rightMargin: Style.space(8)
    anchors.verticalCenter: parent.verticalCenter
    spacing: Style.space(10)
    Column {
      anchors.verticalCenter: parent.verticalCenter
      Txt { anchors.right: parent.right; text: "×" + root.problem.count; bold: true; font.pixelSize: Style.font.subtitle }
      Txt { anchors.right: parent.right; text: root.svc ? I18n.t("since").arg(root.svc.clock(root.problem.first_seen)) : ""; faint: true; font.pixelSize: Style.font.caption }
    }
    Btn { visible: !root.compact; anchors.verticalCenter: parent.verticalCenter; icon: Icons.route; text: I18n.t("route"); onClicked: root.routeClicked() }
    Btn { visible: !root.compact; anchors.verticalCenter: parent.verticalCenter; ghost: true; icon: root.problem.hidden ? Icons.show : Icons.hide; tooltip: I18n.t(root.problem.hidden ? "hintShowProblem" : "hintHideProblem"); onClicked: root.hideClicked() }
  }
}
