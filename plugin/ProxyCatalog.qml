pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import qs.Commons

Item {
  id: root
  property var catalog: ({
      nodes: []
    })
  property string search: ""
  property bool all: false
  signal picked(string protocol)
  signal importRequested
  signal cancelled
  Column {
    id: head
    width: parent.width
    spacing: Style.space(12)
    Btn {
      ghost: true
      text: I18n.t("navProxies") + " › " + I18n.t("proxyNodes")
      onClicked: root.cancelled()
    }
    Txt {
      text: I18n.t("nodeNew")
      bold: true
      font.pixelSize: Style.font.heading
    }
    Txt {
      width: parent.width
      text: I18n.t("nodeChoose")
      dim: true
      wrapMode: Text.Wrap
    }
    Row {
      width: parent.width
      spacing: Style.space(8)
      Txt {
        width: parent.width - importButton.width - Style.space(8)
        text: I18n.t("nodeImportHint")
        wrapMode: Text.Wrap
        dim: true
        font.pixelSize: Style.font.caption
        anchors.verticalCenter: parent.verticalCenter
      }
      Btn {
        id: importButton
        text: I18n.t("proxyImport")
        onClicked: root.importRequested()
      }
    }
    Field {
      width: parent.width
      placeholder: I18n.t("nodeFind")
      input.onTextEdited: root.search = text
    }
  }
  ScrollView {
    id: scroll
    anchors.top: head.bottom
    anchors.topMargin: Style.space(12)
    anchors.bottom: foot.top
    anchors.bottomMargin: Style.space(10)
    anchors.left: parent.left
    anchors.right: parent.right
    contentWidth: availableWidth
    clip: true
    Column {
      width: scroll.availableWidth
      spacing: Style.space(14)
      Repeater {
        model: [
          {
            label: "nodePopular",
            types: ["vless", "hysteria2", "trojan", "ss", "wireguard", "vmess"]
          },
          {
            label: "nodeVpn",
            types: ["openvpn", "tailscale", "masque"]
          },
          {
            label: "nodeService",
            types: ["direct", "reject", "socks5", "http"]
          },
          {
            label: "nodeAll",
            types: []
          }
        ]
        Column {
          id: category
          required property var modelData
          readonly property var protocols: (root.catalog.nodes || []).filter(function (n) {
            return (category.modelData.types.length ? category.modelData.types.indexOf(n.type) >= 0 : ["vless", "hysteria2", "trojan", "ss", "wireguard", "vmess", "openvpn", "tailscale", "masque", "direct", "reject", "socks5", "http"].indexOf(n.type) < 0) && (!root.search || (n.type + " " + I18n.t("nodeDescription_" + n.type)).toLowerCase().indexOf(root.search.toLowerCase()) >= 0);
          })
          visible: protocols.length > 0
          width: parent.width
          spacing: Style.space(8)
          Btn {
            visible: category.modelData.label === "nodeAll"
            text: I18n.t("nodeAll")
            ghost: true
            icon: root.all ? Icons.expanded : Icons.collapsed
            onClicked: root.all = !root.all
          }
          Txt {
            visible: category.modelData.label !== "nodeAll"
            text: I18n.t(category.modelData.label)
            faint: true
            font.pixelSize: Style.font.caption
          }
          Grid {
            visible: category.modelData.label !== "nodeAll" || root.all || root.search !== ""
            width: parent.width
            columns: 3
            spacing: Style.space(8)
            Repeater {
              model: category.protocols
              Rectangle {
                required property var modelData
                width: (parent.width - Style.space(16)) / 3
                height: Style.space(64)
                radius: Style.cornerRadius
                color: Util.alpha(Color.popups.text, 0.04)
                border.width: 1
                border.color: Util.alpha(Color.popups.text, 0.15)
                Column {
                  anchors.fill: parent
                  anchors.margins: Style.space(8)
                  spacing: Style.space(4)
                  Txt {
                    text: ({
                        ss: "Shadowsocks",
                        vmess: "VMess",
                        vless: "VLESS",
                        hysteria2: "Hysteria 2",
                        trojan: "Trojan",
                        wireguard: "WireGuard",
                        openvpn: "OpenVPN",
                        tailscale: "Tailscale"
                      })[parent.parent.modelData.type] || parent.parent.modelData.type.toUpperCase()
                    bold: true
                  }
                  Txt {
                    width: parent.width
                    text: I18n.t("nodeDescription_" + parent.parent.modelData.type)
                    wrapMode: Text.Wrap
                    maximumLineCount: 2
                    elide: Text.ElideRight
                    dim: true
                    font.pixelSize: Style.font.caption
                  }
                }
                MouseArea {
                  anchors.fill: parent
                  cursorShape: Qt.PointingHandCursor
                  onClicked: root.picked(parent.modelData.type)
                }
              }
            }
          }
        }
      }
    }
  }
  Txt {
    id: foot
    anchors.bottom: parent.bottom
    width: parent.width
    text: I18n.t("nodeCatalogHint")
    dim: true
    font.pixelSize: Style.font.caption
  }
}
