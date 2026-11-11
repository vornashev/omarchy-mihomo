pragma Singleton
import QtQuick

// Omarchy 4.5 exports this singleton. Its catalog is not the plugin catalog.
QtObject {
  readonly property string language: "en"
  function t(key) { return key }
}
