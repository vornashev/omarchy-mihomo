pragma Singleton
import QtQuick
import "i18n/ru.js" as Ru
import "i18n/en.js" as En

// UI strings. Russian by default; `locale: "en"` in the widget settings.
// Placeholders use Qt's %1, %2... filled with String.arg().
QtObject {
  id: root

  property string language: "ru"

  function t(key) {
    var table = language === "en" ? en : ru
    var v = table[key]
    if (v === undefined) v = en[key]
    return v === undefined ? key : v
  }

  function kind(k) { return t("kind_" + k) }
  function mode(m) { return m ? t("mode_" + m) : "" }
  function matchKind(k) { return t("match_" + k) }
  function matchKindLong(k) { return t("matchLong_" + k) }
  function ignoreKind(k) { return t("ignore_" + k) }
  function action(a) { return t("action_" + a) }

  readonly property var ru: Ru.strings
  readonly property var en: En.strings
}
