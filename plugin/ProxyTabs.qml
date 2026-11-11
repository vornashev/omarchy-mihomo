import QtQuick
import qs.Commons
import qs.Commons as Commons

Item {
  id: root
  property var model: []
  property string current: ""
  signal picked(string key)
  implicitHeight: Style.space(22)
  Rectangle {
    anchors.bottom: parent.bottom
    width: parent.width
    height: 1
    color: Util.alpha(Commons.Color.popups.text, 0.15)
  }
  Row {
    spacing: Style.space(16)
    Repeater {
      model: root.model
      Item {
        required property var modelData
        activeFocusOnTab: true
        Accessible.role: Accessible.PageTab
        Accessible.name: modelData.label
        Keys.onReturnPressed: root.picked(modelData.key)
        Keys.onSpacePressed: root.picked(modelData.key)
        width: label.implicitWidth
        height: root.height
        Txt {
          id: label
          text: modelData.label
          color: modelData.key === root.current ? Commons.Color.accent : Commons.Color.popups.text
          font.pixelSize: Style.font.body
        }
        Rectangle {
          anchors.bottom: parent.bottom
          width: parent.width
          height: 2
          visible: modelData.key === root.current
          color: Commons.Color.accent
        }
        MouseArea {
          anchors.fill: parent
          cursorShape: Qt.PointingHandCursor
          onClicked: root.picked(modelData.key)
        }
      }
    }
  }
}
