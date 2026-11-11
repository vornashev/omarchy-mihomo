pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons
import qs.Commons as Commons

// One member of a group in its order: drag handle, position, name and
// kind, move up and remove.
Rectangle {
  id: memberRow
  // The group form, for labels, reordering and removal.
  required property var form
  required property string modelData
  required property int index
  width: parent.width
  height: Style.space(40)
  radius: Style.cornerRadius
  color: Util.alpha(Commons.Color.popups.text, 0.04)
  Row {
    anchors.fill: parent
    anchors.margins: Style.space(4)
    spacing: Style.space(4)
    Txt {
      text: "⠿"
      width: Style.space(14)
      anchors.verticalCenter: parent.verticalCenter
      MouseArea {
        anchors.fill: parent
        property real startY: 0
        onPressed: function (m) {
          startY = mapToItem(memberRow.form, 0, m.y).y;
        }
        onReleased: function (m) {
          var delta = mapToItem(memberRow.form, 0, m.y).y - startY;
          memberRow.form.reorder(memberRow.index, Math.max(0, Math.min(memberRow.form.members.length - 1, memberRow.index + Math.round(delta / Style.space(48)))));
        }
        cursorShape: Qt.OpenHandCursor
      }
    }
    Txt {
      text: String(memberRow.index + 1)
      faint: true
      width: Style.space(14)
      anchors.verticalCenter: parent.verticalCenter
    }
    Column {
      width: parent.width - Style.space(130)
      anchors.verticalCenter: parent.verticalCenter
      Txt {
        width: parent.width
        text: memberRow.modelData
        elide: Text.ElideRight
        font.pixelSize: Style.font.body
      }
      Txt {
        text: memberRow.form.label(memberRow.modelData)
        dim: true
        font.pixelSize: Style.font.bodySmall
      }
    }
    Btn {
      ghost: true
      icon: Icons.up
      enabled: memberRow.index > 0
      tooltip: MihomoI18n.t("proxyOrder")
      onClicked: memberRow.form.reorder(memberRow.index, memberRow.index - 1)
    }
    Btn {
      ghost: true
      icon: Icons.close
      tooltip: MihomoI18n.t("delete")
      onClicked: memberRow.form.toggle(memberRow.modelData, false)
    }
  }
}
