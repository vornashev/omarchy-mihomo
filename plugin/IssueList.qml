import QtQuick

// Setup issues from the daemon; `only` limits them to some ids.
Column {
  id: root
  property var svc
  property var only: []
  readonly property var shown: (svc.issues || []).filter(function(i) { return root.only.length === 0 || root.only.indexOf(i.id) !== -1 })
  spacing: 6
  visible: shown.length > 0
  Repeater {
    model: root.shown
    IssueRow { required property var modelData; width: root.width; svc: root.svc; issue: modelData }
  }
}
