pragma Singleton
import QtQuick
QtObject {
  property int cornerRadius: 6
  property var font: ({ family: "JetBrainsMono Nerd Font", caption: 10, bodySmall: 11, body: 12, subtitle: 13, heading: 16, display: 24, icon: 14 })
  function space(n) { return n }
  function hoverFillFor(fg, accent) { return Qt.rgba(fg.r, fg.g, fg.b, 0.08) }
  function selectedFillFor(fg, accent) { return Qt.rgba(accent.r, accent.g, accent.b, 0.18) }
  function selectionFillFor(fg, accent) { return selectedFillFor(fg, accent) }
}
