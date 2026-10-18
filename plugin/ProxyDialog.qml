import QtQuick
import QtQuick.Controls
import qs.Commons

Popup {
  id: root
  property string title: ""
  property bool busy: false
  signal confirmRequested
  Shortcut {
    sequence: "Return"
    enabled: root.visible
    onActivated: root.confirmRequested()
  }
  default property alias contents: body.data
  property alias body: body
  modal: true
  dim: true
  focus: true
  closePolicy: busy ? Popup.NoAutoClose : Popup.CloseOnEscape
  anchors.centerIn: Overlay.overlay
  width: Math.min(Style.space(500), parent ? parent.width - Style.space(24) : Style.space(500))
  height: Math.min(body.implicitHeight + 2 * padding, parent ? parent.height - Style.space(24) : body.implicitHeight + 2 * padding)
  padding: Style.space(18)
  background: Rectangle {
    color: Color.popups.background
    radius: Style.cornerRadius
    border.width: 1
    border.color: Util.alpha(Color.popups.text, 0.2)
  }
  Overlay.modal: Rectangle {
    color: "#99000000"
  }
  contentItem: ScrollView {
    id: dialogScroll
    implicitHeight: body.implicitHeight
    contentHeight: body.implicitHeight
    contentWidth: availableWidth
    clip: true
    Column {
      id: body
      width: root.width - 2 * root.padding
      spacing: Style.space(12)
      Txt {
        width: parent.width
        text: root.title
        bold: true
        font.pixelSize: Style.font.subtitle
        wrapMode: Text.Wrap
      }
    }
  }
}
