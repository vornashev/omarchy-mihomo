import QtQuick
import qs.Commons

// A setup issue with its fix. Fixes that touch other programs ask first;
// privileged ones open a terminal where sudo can ask for the password.
Rectangle {
  id: root
  property var svc
  property var issue
  readonly property var fix: issue ? issue.fix : null
  readonly property bool busy: !!(fix && svc.fixing[fix.id]) || issue.id === "core_installing"
  property bool asking: false
  readonly property color toneColor: issue.level === "error" ? svc.red : (issue.level === "warn" ? svc.yellow : Color.accent)

  implicitHeight: col.implicitHeight + Style.space(16)
  radius: Style.cornerRadius
  color: Util.alpha(toneColor, 0.09)
  border.width: issue.level === "info" ? 0 : 1
  border.color: Util.alpha(toneColor, 0.3)

  Row {
    id: row
    anchors.fill: parent
    anchors.leftMargin: Style.space(10)
    anchors.rightMargin: Style.space(8)
    spacing: Style.space(8)
    Txt { id: ic; anchors.verticalCenter: parent.verticalCenter; text: issue.level === "info" ? Icons.info : Icons.problems; color: root.toneColor; font.pixelSize: Style.font.icon }
    Column {
      id: col
      anchors.verticalCenter: parent.verticalCenter
      width: parent.width - ic.width - buttons.width - Style.space(16)
      spacing: Style.space(3)
      Txt {
        width: parent.width
        text: I18n.t("issue_" + issue.id).arg(issue.detail || "")
        color: issue.level === "info" ? Util.alpha(Color.popups.text, 0.68) : root.toneColor
        font.pixelSize: Style.font.bodySmall
        wrapMode: Text.Wrap
        maximumLineCount: 3
      }
      Txt {
        visible: root.asking || !!(root.fix && root.fix.privileged)
        width: parent.width
        text: root.asking ? I18n.t("confirm_" + root.fix.id) : I18n.t("privilegedHint")
        dim: true; font.pixelSize: Style.font.caption; wrapMode: Text.Wrap
      }
    }
    Row {
      id: buttons
      anchors.verticalCenter: parent.verticalCenter
      spacing: Style.space(4)
      Btn { visible: root.asking; ghost: true; text: I18n.t("cancel"); onClicked: root.asking = false }
      Btn {
        visible: !!root.fix
        primary: true
        busy: root.busy
        text: root.fix ? (root.asking ? I18n.t("confirm") : I18n.t("fix_" + root.fix.id)) : ""
        onClicked: {
          if (root.fix.confirm && !root.asking) { root.asking = true; return }
          root.asking = false
          root.svc.runFix(root.fix)
        }
      }
    }
  }
}
