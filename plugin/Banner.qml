import QtQuick
import qs.Commons
import qs.Commons as Commons

// One-line notice. `tone`: info | ok | warn | error.
Rectangle {
  id: root
  property string tone: "info"
  property string text: ""
  property string icon: tone === "ok" ? Icons.check : (tone === "info" ? Icons.info : Icons.problems)
  property string action: ""
  property color toneColor: Commons.Color.accent
  signal actionClicked()

  implicitHeight: Math.max(Style.space(30), body.implicitHeight + Style.space(14))
  radius: Style.cornerRadius
  color: Util.alpha(toneColor, 0.09)
  border.width: tone === "info" ? 0 : 1
  border.color: Util.alpha(toneColor, 0.3)

  Row {
    anchors.fill: parent
    anchors.leftMargin: Style.space(10)
    anchors.rightMargin: Style.space(10)
    spacing: Style.space(8)
    Txt { id: ic; anchors.verticalCenter: parent.verticalCenter; text: root.icon; color: root.toneColor; font.pixelSize: Style.font.icon }
    Txt {
      id: body
      anchors.verticalCenter: parent.verticalCenter
      width: parent.width - ic.width - (act.visible ? act.width + Style.space(8) : 0) - Style.space(8)
      text: root.text
      color: root.tone === "info" ? Util.alpha(Commons.Color.popups.text, 0.68) : root.toneColor
      font.pixelSize: Style.font.body
      wrapMode: Text.Wrap
      maximumLineCount: 3
    }
    Txt {
      id: act
      visible: root.action !== ""
      anchors.verticalCenter: parent.verticalCenter
      text: root.action; color: root.toneColor; bold: true; font.underline: true; font.pixelSize: Style.font.body
      MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.actionClicked() }
    }
  }
}
