import QtQuick
import qs.Commons

// A list setting of the DNS page: the user's own value, or the profile's
// shown as the placeholder while the field is empty.
Row {
  id: root
  property string label: ""
  property string hint: ""
  // The value the core uses now, for the placeholder.
  property string profile: ""
  property alias text: field.text
  property alias input: field.input
  signal submit(string value)
  spacing: Style.space(16)

  Column {
    width: Style.space(170)
    anchors.verticalCenter: parent.verticalCenter
    spacing: Style.space(2)
    Txt { text: root.label }
    Txt { width: parent.width; text: root.hint; faint: true; font.pixelSize: Style.font.bodySmall; wrapMode: Text.Wrap }
  }
  Field {
    id: field
    width: root.width - Style.space(186)
    anchors.verticalCenter: parent.verticalCenter
    placeholder: root.profile
    onAccepted: { root.submit(text); input.focus = false }
  }
}
