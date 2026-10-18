import QtQuick
import QtQuick.Controls
import qs.Commons

Menu {
  id: root
  popupType: Popup.Item
  width: Style.space(236)
  padding: Style.space(4)
  margins: Style.space(8)
  spacing: Style.space(1)
  modal: false
  dim: false
  closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
  onOpened: {
    if (currentIndex < 0) {
      for (var i = 0; i < count; i++) {
        var item = itemAt(i);
        if (item instanceof MenuItem && item.enabled) {
          currentIndex = i;
          item.forceActiveFocus();
          break;
        }
      }
    }
  }
  background: MenuSurface {}
  enter: Transition {}
  exit: Transition {}
  contentItem: ListView {
    implicitHeight: contentHeight
    model: root.contentModel
    currentIndex: root.currentIndex
    spacing: root.spacing
    clip: true
    interactive: contentHeight > height
    boundsBehavior: Flickable.StopAtBounds
    ScrollIndicator.vertical: ScrollIndicator {}
  }
}
