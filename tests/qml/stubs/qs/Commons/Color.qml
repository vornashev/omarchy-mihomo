pragma Singleton
import QtQuick
QtObject {
  property color background: "#1e1e2e"
  property color accent: "#89b4fa"
  property color urgent: "#f38ba8"
  property color muted: "#585b70"
  property QtObject popups: QtObject { property color text: "#cdd6f4"; property color background: "#1e1e2e" }
}
