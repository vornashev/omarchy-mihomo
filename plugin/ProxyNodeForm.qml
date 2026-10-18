pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import qs.Commons
import "." as Mihomo
import "ProxyModels.js" as Models
import "ProxyNodeModels.js" as Nodes

Column {
  id: root
  required property var editor
  readonly property var draft: editor.draft
  readonly property var fields: editor.schema.fields
  readonly property var sections: Nodes.sections(editor.schema, draft.type)
  property bool choosingGroup: false
  property bool networkExpanded: false
  property bool securityExpanded: false
  property bool signatures: false
  property bool transportExpanded: false
  property bool muxExpanded: false
  property string confError: ""
  property bool importing: false
  property bool awgEnabled: !!draft["amnezia-wg-option"]
  spacing: Style.space(12)
  function has(key) {
    return !!Nodes.field(editor.schema, key);
  }
  function setTls(mode) {
    if (has("tls"))
      editor.setField("tls", mode !== "none");
    if (mode === "reality")
      editor.setField("reality-opts", draft["reality-opts"] || {});
    else
      editor.setField("reality-opts", undefined);
  }
  function importConf(path) {
    importing = true;
    confError = "";
    editor.svc.request("proxy_import.preview", {
      path: path
    }, function (r, e) {
      importing = false;
      if (e) {
        confError = e.message;
        return;
      }
      var rows = (r || []).filter(function (n) {
        return n.definition && n.definition.type === "wireguard";
      });
      if (!rows.length) {
        confError = I18n.t("proxyNoResults");
        return;
      }
      var imported = rows[0].definition;
      editor.draft = Object.assign({}, root.draft, imported, {
        name: root.draft.name || imported.name
      });
      editor.changed();
      confError = (rows[0].warnings || []).join("\n");
    });
  }
  FileDialog {
    id: confDialog
    nameFilters: ["WireGuard (*.conf)"]
    onAccepted: root.importConf(decodeURIComponent(String(selectedFile).replace(/^file:\/\//, "")))
  }

  Column {
    visible: root.editor.section === "basic"
    width: parent.width
    spacing: Style.space(12)
    Banner {
      visible: Nodes.service(root.draft.type)
      width: parent.width
      text: I18n.t("nodeDescription_" + root.draft.type)
    }
    Repeater {
      model: Nodes.basics[root.draft.type] || [["name"], ["server", "port"]]
      Row {
        id: fieldRow
        required property var modelData
        width: parent.width
        spacing: Style.space(10)
        Repeater {
          model: fieldRow.modelData.filter(function (k) {
            return root.has(k);
          })
          ProxyNodeField {
            required property string modelData
            width: fieldRow.modelData[0] === "server" && fieldRow.modelData.length === 2 ? (fieldRow.width - Style.space(10)) * (modelData === "server" ? 0.75 : 0.25) : (fieldRow.width - Style.space(10) * (fieldRow.modelData.length - 1)) / fieldRow.modelData.length
            editor: root.editor
            fieldKey: modelData
            expanded: field.kind === "list"
          }
        }
      }
    }
    Column {
      width: parent.width
      spacing: Style.space(5)
      Repeater {
        model: root.sections.filter(function (s) {
          return s !== "basic";
        })
        Rectangle {
          id: summary
          required property string modelData
          width: parent.width
          height: Style.space(36)
          radius: Style.cornerRadius
          color: Util.alpha(Color.popups.text, 0.04)
          border.width: 1
          border.color: Util.alpha(Color.popups.text, 0.15)
          Row {
            anchors.fill: parent
            anchors.margins: Style.space(6)
            spacing: Style.space(8)
            Txt {
              width: Math.min(implicitWidth, parent.width * 0.28)
              text: root.editor.sectionLabel(summary.modelData)
              elide: Text.ElideRight
              font.pixelSize: Style.font.bodySmall
              anchors.verticalCenter: parent.verticalCenter
            }
            Txt {
              width: Math.max(0, parent.width - configure.width - parent.children[0].width - Style.space(16))
              text: Nodes.summary(root.draft, summary.modelData) || I18n.t("nodeParameters").arg(root.fields.filter(function (f) {
                return Nodes.section(f, root.draft.type) === summary.modelData;
              }).length)
              dim: true
              font.pixelSize: Style.font.caption
              elide: Text.ElideRight
              anchors.verticalCenter: parent.verticalCenter
            }
            Btn {
              id: configure
              text: I18n.t("nodeConfigure")
              ghost: true
              onClicked: root.editor.section = summary.modelData
            }
          }
        }
      }
    }
    Flow {
      visible: !Nodes.service(root.draft.type)
      width: parent.width
      spacing: Style.space(6)
      Txt {
        text: I18n.t("proxyMemberOf")
        dim: true
        font.pixelSize: Style.font.caption
        height: Style.space(28)
        verticalAlignment: Text.AlignVCenter
      }
      Repeater {
        model: root.editor.memberOf
        Btn {
          required property string modelData
          text: modelData + " ×"
          onClicked: {
            root.editor.memberOf = Models.toggleMember(root.editor.memberOf, modelData, false);
            root.editor.changed();
          }
        }
      }
      Btn {
        text: I18n.t("nodeAddGroup")
        ghost: true
        onClicked: root.choosingGroup = !root.choosingGroup
      }
    }
    Flow {
      visible: root.choosingGroup
      width: parent.width
      spacing: Style.space(8)
      Repeater {
        model: root.editor.metadata.groups || []
        ProxyCheck {
          required property string modelData
          text: modelData
          checked: root.editor.memberOf.indexOf(modelData) >= 0
          onToggled: {
            root.editor.memberOf = Models.toggleMember(root.editor.memberOf, modelData, checked);
            root.editor.changed();
          }
        }
      }
    }
  }
  Column {
    visible: root.editor.section === "security"
    width: parent.width
    spacing: Style.space(12)
    Txt {
      visible: root.has("tls") || root.has("reality-opts")
      text: I18n.t("nodeEncryption")
    }
    Row {
      visible: root.has("tls") || root.has("reality-opts")
      width: parent.width
      spacing: Style.space(8)
      Repeater {
        model: (root.has("tls") ? ["none", "tls"] : ["tls"]).concat(root.has("reality-opts") ? ["reality"] : [])
        Rectangle {
          id: mode
          required property string modelData
          width: (parent.width - Style.space(8) * (parent.children.length - 2)) / (parent.children.length - 1)
          height: Style.space(65)
          radius: Style.cornerRadius
          color: Nodes.tlsMode(root.draft) === modelData ? Style.selectedFillFor(Color.popups.text, Color.accent) : Util.alpha(Color.popups.text, 0.04)
          border.width: 1
          border.color: Nodes.tlsMode(root.draft) === modelData ? Color.accent : Util.alpha(Color.popups.text, 0.15)
          Column {
            anchors.fill: parent
            anchors.margins: Style.space(8)
            spacing: Style.space(5)
            Txt {
              text: mode.modelData === "none" ? I18n.t("nodeNone") : mode.modelData.toUpperCase()
              font.pixelSize: Style.font.bodySmall
            }
            Txt {
              width: parent.width
              text: I18n.t(mode.modelData === "none" ? "nodeNoneHint" : mode.modelData === "tls" ? "nodeTlsHint" : "nodeRealityHint")
              wrapMode: Text.Wrap
              dim: true
              font.pixelSize: Style.font.caption
            }
          }
          MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: root.setTls(mode.modelData)
          }
        }
      }
    }
    ProxyNodeField {
      visible: root.has("servername") || root.has("sni")
      width: parent.width
      editor: root.editor
      fieldKey: root.has("servername") ? "servername" : "sni"
    }
    Row {
      visible: root.has("reality-opts") && Nodes.tlsMode(root.draft) === "reality"
      width: parent.width
      spacing: Style.space(10)
      ProxyNodeField {
        width: (parent.width - Style.space(10)) / 2
        editor: root.editor
        parentKey: "reality-opts"
        fieldKey: "public-key"
      }
      ProxyNodeField {
        width: (parent.width - Style.space(10)) / 2
        editor: root.editor
        parentKey: "reality-opts"
        fieldKey: "short-id"
      }
    }
    Row {
      width: parent.width
      spacing: Style.space(10)
      ProxyNodeField {
        visible: root.has("client-fingerprint")
        width: root.has("alpn") ? (parent.width - Style.space(10)) / 2 : parent.width
        editor: root.editor
        fieldKey: "client-fingerprint"
      }
      ProxyNodeField {
        visible: root.has("alpn")
        width: root.has("client-fingerprint") ? (parent.width - Style.space(10)) / 2 : parent.width
        editor: root.editor
        fieldKey: "alpn"
        expanded: true
      }
    }
    Column {
      visible: root.has("skip-cert-verify")
      width: parent.width
      spacing: Style.space(6)
      ProxyCheck {
        text: I18n.t("nodeInsecure")
        checked: !!root.draft["skip-cert-verify"]
        onToggled: root.editor.setField("skip-cert-verify", checked)
      }
      Txt {
        width: parent.width
        text: I18n.t("nodeInsecureHint")
        color: Color.urgent
        wrapMode: Text.Wrap
        font.pixelSize: Style.font.caption
      }
    }
    Btn {
      text: I18n.t("proxySection_advanced")
      ghost: true
      icon: root.securityExpanded ? Icons.expanded : Icons.collapsed
      onClicked: root.securityExpanded = !root.securityExpanded
    }
    Repeater {
      model: root.fields.filter(function (f) {
        return Nodes.section(f, root.draft.type) === "security" && ["tls", "servername", "sni", "reality-opts", "client-fingerprint", "alpn", "skip-cert-verify"].indexOf(f.key) < 0;
      })
      ProxyNodeField {
        required property var modelData
        visible: root.securityExpanded
        width: parent.width
        editor: root.editor
        fieldKey: modelData.key
      }
    }
  }
  Column {
    visible: root.editor.section === "transport"
    width: parent.width
    spacing: Style.space(12)
    Txt {
      text: I18n.t("nodeTransport")
    }
    Txt {
      text: I18n.t("nodeTransportHint")
      dim: true
      font.pixelSize: Style.font.caption
    }
    Flow {
      visible: root.has("network")
      width: parent.width
      spacing: Style.space(6)
      Repeater {
        model: root.has("network") ? Nodes.field(root.editor.schema, "network").choices || ["tcp", "ws", "grpc", "h2", "http", "xhttp"] : []
        Btn {
          required property string modelData
          text: ({
              tcp: "TCP",
              ws: "WebSocket",
              grpc: "gRPC",
              h2: "HTTP/2",
              xhttp: "xHTTP",
              http: "HTTP"
            })[modelData] || modelData
          primary: (root.draft.network || "tcp") === modelData
          onClicked: root.editor.setField("network", modelData)
        }
      }
    }
    Banner {
      visible: root.draft.network === "ws"
      width: parent.width
      text: I18n.t("nodeWsHint")
    }
    Column {
      visible: root.draft.network === "ws" && root.has("ws-opts")
      width: parent.width
      spacing: Style.space(12)
      ProxyNodeField {
        width: parent.width
        editor: root.editor
        parentKey: "ws-opts"
        fieldKey: "path"
      }
      ProxyNodeField {
        width: parent.width
        editor: root.editor
        parentKey: "ws-opts"
        fieldKey: "headers"
        expanded: true
      }
      Txt {
        text: I18n.t("nodeField_max-early-data")
      }
      Row {
        width: parent.width
        spacing: Style.space(10)
        ProxyNodeField {
          width: (parent.width - Style.space(10)) * 0.35
          editor: root.editor
          parentKey: "ws-opts"
          fieldKey: "max-early-data"
        }
        ProxyNodeField {
          width: (parent.width - Style.space(10)) * 0.65
          editor: root.editor
          parentKey: "ws-opts"
          fieldKey: "early-data-header-name"
        }
      }
      Btn {
        text: I18n.t("proxySection_advanced")
        ghost: true
        onClicked: root.transportExpanded = !root.transportExpanded
      }
      Repeater {
        model: ["v2ray-http-upgrade", "v2ray-http-upgrade-fast-open"]
        ProxyNodeField {
          required property string modelData
          visible: root.transportExpanded
          width: parent.width
          editor: root.editor
          parentKey: "ws-opts"
          fieldKey: modelData
        }
      }
    }
    Repeater {
      model: root.fields.filter(function (f) {
        return Nodes.section(f, root.draft.type) === "transport" && ["network", "ws-opts"].indexOf(f.key) < 0;
      })
      ProxyNodeField {
        required property var modelData
        width: parent.width
        editor: root.editor
        fieldKey: modelData.key
        visible: Models.visible(modelData, root.draft)
        expanded: true
      }
    }
  }
  Column {
    visible: root.editor.section === "advanced"
    width: parent.width
    spacing: Style.space(12)
    ProxyNodeField {
      visible: root.has("dialer-proxy")
      width: parent.width
      editor: root.editor
      fieldKey: "dialer-proxy"
    }
    Txt {
      visible: !!root.draft["dialer-proxy"]
      width: parent.width
      text: I18n.t("nodeYou") + " → " + (root.draft["dialer-proxy"] || "") + " → " + (root.draft.name || "") + " → " + I18n.t("nodeSite")
      wrapMode: Text.Wrap
      color: Color.accent
      font.pixelSize: Style.font.bodySmall
    }
    Column {
      visible: root.has("smux")
      width: parent.width
      spacing: Style.space(8)
      ProxyCheck {
        text: I18n.t("nodeMux")
        checked: !!(root.draft.smux || {}).enabled
        onToggled: root.editor.setField("smux", Models.put(root.draft.smux, "enabled", checked))
      }
      Txt {
        text: I18n.t("nodeMuxHint")
        dim: true
        font.pixelSize: Style.font.caption
      }
      Seg {
        visible: !!(root.draft.smux || {}).enabled
        model: [
          {
            key: "h2mux",
            label: "h2mux"
          },
          {
            key: "smux",
            label: "smux"
          },
          {
            key: "yamux",
            label: "yamux"
          }
        ]
        current: (root.draft.smux || {}).protocol || "h2mux"
        onPicked: function (k) {
          root.editor.setField("smux", Models.put(root.draft.smux, "protocol", k));
        }
      }
      Row {
        visible: !!(root.draft.smux || {}).enabled
        width: parent.width
        spacing: Style.space(10)
        Repeater {
          model: ["max-connections", "min-streams", "max-streams"]
          ProxyNodeField {
            required property string modelData
            width: (parent.width - Style.space(20)) / 3
            editor: root.editor
            parentKey: "smux"
            fieldKey: modelData
          }
        }
      }
      ProxyCheck {
        visible: !!(root.draft.smux || {}).enabled
        text: I18n.t("nodeField_padding")
        checked: !!(root.draft.smux || {}).padding
        onToggled: root.editor.setField("smux", Models.put(root.draft.smux, "padding", checked))
      }
      Btn {
        visible: !!(root.draft.smux || {}).enabled
        text: I18n.t("proxySection_advanced")
        ghost: true
        onClicked: root.muxExpanded = !root.muxExpanded
      }
      Repeater {
        model: root.has("smux") ? Nodes.field(root.editor.schema, "smux").fields.filter(function (f) {
          return ["enabled", "protocol", "max-connections", "min-streams", "max-streams", "padding"].indexOf(f.key) < 0;
        }) : []
        ProxyNodeField {
          required property var modelData
          visible: root.muxExpanded && !!(root.draft.smux || {}).enabled
          width: parent.width
          editor: root.editor
          parentKey: "smux"
          fieldKey: modelData.key
        }
      }
    }
    Btn {
      text: I18n.t("nodeNetwork")
      icon: root.networkExpanded ? Icons.expanded : Icons.collapsed
      ghost: true
      onClicked: root.networkExpanded = !root.networkExpanded
    }
    Repeater {
      model: root.fields.filter(function (f) {
        return Nodes.section(f, root.draft.type) === "advanced" && ["dialer-proxy", "smux"].indexOf(f.key) < 0;
      })
      ProxyNodeField {
        required property var modelData
        visible: root.networkExpanded || ["ip-version", "interface-name", "routing-mark", "tfo", "mptcp"].indexOf(modelData.key) < 0
        width: parent.width
        editor: root.editor
        fieldKey: modelData.key
      }
    }
  }
  Column {
    visible: root.editor.section === "peers"
    width: parent.width
    spacing: Style.space(12)
    Txt {
      width: parent.width
      text: I18n.t("nodePeerHint")
      wrapMode: Text.Wrap
      dim: true
      font.pixelSize: Style.font.bodySmall
    }
    ProxyPeers {
      width: parent.width
      editor: root.editor
    }
  }
  Column {
    visible: root.editor.section === "amnezia"
    width: parent.width
    spacing: Style.space(12)
    ProxyCheck {
      text: I18n.t("nodeAwg")
      checked: root.awgEnabled
      onToggled: {
        root.awgEnabled = checked;
        root.editor.setField("amnezia-wg-option", checked ? {} : undefined);
      }
    }
    Txt {
      width: parent.width
      text: I18n.t("nodeAwgHint")
      wrapMode: Text.Wrap
      dim: true
      font.pixelSize: Style.font.caption
    }
    Flow {
      width: parent.width
      spacing: Style.space(6)
      Btn {
        text: I18n.t("nodeAwgDefault")
        enabled: root.awgEnabled
        onClicked: root.editor.setField("amnezia-wg-option", {
          jc: 4,
          jmin: 40,
          jmax: 70,
          s1: 0,
          s2: 0,
          h1: "1",
          h2: "2",
          h3: "3",
          h4: "4"
        })
      }
      Btn {
        text: I18n.t("nodeConf")
        busy: root.importing
        onClicked: confDialog.open()
      }
    }
    Txt {
      visible: root.confError !== ""
      text: root.confError
      width: parent.width
      wrapMode: Text.Wrap
      color: Color.urgent
    }
    Repeater {
      model: [
        {
          label: "nodeJunk",
          keys: ["jc", "jmin", "jmax"]
        },
        {
          label: "nodeOffsets",
          keys: ["s1", "s2"]
        },
        {
          label: "nodeHeaders",
          keys: ["h1", "h2", "h3", "h4"]
        }
      ]
      Column {
        id: awgRow
        required property var modelData
        visible: root.awgEnabled
        width: parent.width
        spacing: Style.space(6)
        Txt {
          text: I18n.t(awgRow.modelData.label)
        }
        Row {
          width: parent.width
          spacing: Style.space(10)
          Repeater {
            model: awgRow.modelData.keys
            ProxyNodeField {
              required property string modelData
              width: (parent.width - Style.space(10) * (awgRow.modelData.keys.length - 1)) / awgRow.modelData.keys.length
              editor: root.editor
              parentKey: "amnezia-wg-option"
              fieldKey: modelData
            }
          }
        }
      }
    }
    Btn {
      visible: root.awgEnabled
      text: I18n.t("nodeSignatures")
      ghost: true
      onClicked: root.signatures = !root.signatures
    }
    Repeater {
      model: root.has("amnezia-wg-option") ? Nodes.field(root.editor.schema, "amnezia-wg-option").fields.filter(function (f) {
        return ["jc", "jmin", "jmax", "s1", "s2", "h1", "h2", "h3", "h4"].indexOf(f.key) < 0;
      }) : []
      ProxyNodeField {
        required property var modelData
        visible: root.awgEnabled && root.signatures
        width: parent.width
        editor: root.editor
        parentKey: "amnezia-wg-option"
        fieldKey: modelData.key
      }
    }
  }
}
