import QtQuick
import QtQuick.Effects
import qs.Commons
import qs.Commons as Commons

// Shared surface for AJFj6 menus and select popups.
Item {
  RectangularShadow {
    anchors.fill: surface
    radius: surface.radius
    blur: Style.space(24)
    offset: Qt.vector2d(0, Style.space(8))
    color: "#80000000"
  }
  Rectangle {
    id: surface
    anchors.fill: parent
    radius: Style.space(6)
    color: Qt.tint(Commons.Color.popups.background, Util.alpha(Commons.Color.popups.text, 0.11))
    border.width: 1
    border.color: Util.alpha(Commons.Color.popups.text, 0.15)
  }
}
