pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons
import qs.Commons as Commons
import "ProxyModels.js" as Models
import "ProxyNodeModels.js" as Nodes

Column {
  id: root
  required property var editor
  readonly property var peers: editor.draft.peers || []
  readonly property var schema: Nodes.field(editor.schema, "peers")
  function setPeer(index, key, value) {
    var a = Models.clone(peers);
    a[index] = Models.put(a[index], key, value);
    editor.setField("peers", a);
  }
  spacing: Style.space(10)
  Repeater {
    model: root.peers.length
    Rectangle {
      id: peer
      required property int index
      property bool expanded: index === 0
      width: parent.width
      height: body.implicitHeight + Style.space(20)
      radius: Style.cornerRadius
      color: Util.alpha(Commons.Color.popups.text, 0.04)
      border.width: 1
      border.color: Util.alpha(Commons.Color.popups.text, 0.15)
      Column {
        id: body
        x: Style.space(10)
        y: Style.space(10)
        width: parent.width - Style.space(20)
        spacing: Style.space(10)
        Row {
          width: parent.width
          spacing: Style.space(6)
          Btn {
            width: parent.width - removeButton.width - Style.space(6)
            text: MihomoI18n.t("nodePeer").arg(peer.index + 1)
            icon: peer.expanded ? Icons.expanded : Icons.collapsed
            ghost: true
            onClicked: peer.expanded = !peer.expanded
          }
          Btn {
            id: removeButton
            icon: Icons.remove
            ghost: true
            tooltip: MihomoI18n.t("delete")
            onClicked: {
              var a = Models.clone(root.peers);
              a.splice(peer.index, 1);
              root.editor.setField("peers", a);
            }
          }
        }
        Txt {
          visible: !peer.expanded
          width: parent.width
          text: (root.peers[peer.index].server || "") + ":" + (root.peers[peer.index].port || "") + " · " + (root.peers[peer.index]["allowed-ips"] || []).join(", ")
          elide: Text.ElideRight
          dim: true
          font.pixelSize: Style.font.bodySmall
        }
        Repeater {
          model: [["server", "port"], ["public-key"], ["pre-shared-key"], ["allowed-ips"], ["reserved"]]
          Row {
            id: fields
            required property var modelData
            visible: peer.expanded
            width: parent.width
            spacing: Style.space(10)
            Repeater {
              model: fields.modelData
              SchemaField {
                required property string modelData
                width: (parent.width - Style.space(10) * (fields.modelData.length - 1)) / fields.modelData.length
                compact: true
                field: root.schema && root.schema.item.fields ? root.schema.item.fields.filter(function (f) {
                  return f.key === modelData;
                })[0] || {
                  key: modelData,
                  kind: "string"
                } : {
                  key: modelData,
                  kind: "string"
                }
                value: (root.peers[peer.index] || {})[modelData]
                label: MihomoI18n.t("nodeField_" + modelData)
                expanded: field.kind === "list"
                onEdited: function (v) {
                  root.setPeer(peer.index, modelData, v);
                }
                onIssue: function (e) {
                  root.editor.issues = Models.put(root.editor.issues, "peer." + peer.index + "." + modelData, e);
                }
              }
            }
          }
        }
      }
    }
  }
  Btn {
    text: MihomoI18n.t("nodeAddPeer")
    icon: Icons.add
    onClicked: root.editor.setField("peers", root.peers.concat([
      {}
    ]))
  }
  ProxyNodeField {
    visible: !!Nodes.field(root.editor.schema, "persistent-keepalive")
    width: parent.width
    editor: root.editor
    fieldKey: "persistent-keepalive"
  }
}
