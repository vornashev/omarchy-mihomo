import QtQuick
import "ProxyModels.js" as Models
import "ProxyNodeModels.js" as Nodes

SchemaField {
  id: root
  required property var editor
  property string fieldKey: ""
  property string parentKey: ""
  property var definition: Nodes.field(editor.schema, parentKey || fieldKey)
  field: parentKey ? ((definition && definition.fields) || []).filter(function (f) {
    return f.key === fieldKey;
  })[0] || {
    key: fieldKey,
    kind: "string"
  } : definition || {
    key: fieldKey,
    kind: "string"
  }
  label: MihomoI18n.t("nodeField_" + fieldKey) === "nodeField_" + fieldKey ? fieldKey : MihomoI18n.t("nodeField_" + fieldKey)
  compact: true
  value: parentKey ? (editor.draft[parentKey] || {})[fieldKey] : editor.draft[fieldKey]
  context: parentKey ? editor.draft[parentKey] || {} : editor.draft
  targets: (editor.metadata.targets || []).filter(function (n) {
    return n !== editor.draft.name && n !== editor.original;
  })
  targetGroups: editor.metadata.groups || []
  targetDetails: editor.targetDetails
  providers: editor.metadata.providers || []
  onEdited: function (v) {
    editor.setField(parentKey || fieldKey, parentKey ? Models.put(editor.draft[parentKey], fieldKey, v) : v);
  }
  onIssue: function (e) {
    editor.issues = Models.put(editor.issues, parentKey ? parentKey + "." + fieldKey : fieldKey, e);
  }
}
