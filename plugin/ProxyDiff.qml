pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons
import qs.Commons as Commons
import "ProxyNodeModels.js" as Nodes

Column {
  id: root
  property var rows: []
  property string leftTitle: ""
  property string rightTitle: ""
  spacing: Style.space(4)
  Row {
    width: parent.width
    visible: root.leftTitle !== ""
    Txt {
      width: parent.width * 0.34
      text: MihomoI18n.t("nodeField")
      faint: true
      font.pixelSize: Style.font.bodySmall
    }
    Txt {
      width: parent.width * 0.33
      text: root.leftTitle
      faint: true
      font.pixelSize: Style.font.bodySmall
    }
    Txt {
      width: parent.width * 0.33
      text: root.rightTitle
      faint: true
      font.pixelSize: Style.font.bodySmall
    }
  }
  Repeater {
    model: root.rows
    Rectangle {
      id: row
      required property var modelData
      width: parent.width
      height: Style.space(34)
      radius: Style.cornerRadius
      color: Util.alpha(Commons.Color.popups.text, 0.04)
      Row {
        anchors.fill: parent
        anchors.margins: Style.space(6)
        spacing: Style.space(6)
        Txt {
          width: (parent.width - Style.space(12)) * 0.34
          text: row.modelData.key
          elide: Text.ElideRight
          dim: true
          font.pixelSize: Style.font.bodySmall
          anchors.verticalCenter: parent.verticalCenter
        }
        Txt {
          width: (parent.width - Style.space(12)) * 0.33
          text: Nodes.display(row.modelData.before, row.modelData.key)
          elide: Text.ElideRight
          font.pixelSize: Style.font.bodySmall
          anchors.verticalCenter: parent.verticalCenter
        }
        Txt {
          width: (parent.width - Style.space(12)) * 0.33
          text: Nodes.display(row.modelData.after, row.modelData.key)
          elide: Text.ElideRight
          color: Commons.Color.accent
          font.pixelSize: Style.font.bodySmall
          anchors.verticalCenter: parent.verticalCenter
        }
      }
    }
  }
}
