pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import qs.Commons
import "." as Mihomo
import "ProxyModels.js" as Models

// Group form → Members: by hand (ordered list and available nodes) or
// automatically (filters over every node), with the resulting set.
Column {
  id: section
  // The group form: its draft, members, nodes and actions.
  required property var form
  width: parent.width
  spacing: Style.space(12)
  Row {
    spacing: Style.space(14)
    Txt {
      text: I18n.t("proxyCollect")
      anchors.verticalCenter: parent.verticalCenter
    }
    Seg {
      model: [
        {
          key: "manual",
          label: I18n.t("proxyManual")
        },
        {
          key: "auto",
          label: I18n.t("proxyAutomatic")
        }
      ]
      current: section.form.automatic ? "auto" : "manual"
      onPicked: function (k) {
        section.form.setAutomatic(k === "auto");
      }
    }
  }
  Row {
    visible: !section.form.automatic
    width: parent.width
    spacing: Style.space(12)
    Column {
      width: (parent.width - Style.space(12)) / 2
      spacing: Style.space(8)
      Txt {
        text: I18n.t("proxyAvailable")
        faint: true
        font.pixelSize: Style.font.caption
      }
      Field {
        width: parent.width
        placeholder: I18n.t("searchHint")
        input.onTextEdited: section.form.search = text
      }
      ProxyTabs {
        width: parent.width
        current: section.form.availableTab
        model: [
          {
            key: "nodes",
            label: I18n.t("proxyNodes")
          },
          {
            key: "groups",
            label: I18n.t("proxyGroups")
          },
          {
            key: "special",
            label: I18n.t("proxySpecial")
          }
        ]
        onPicked: function (k) {
          section.form.availableTab = k;
        }
      }
      Repeater {
        model: section.form.available
        ProxyCheck {
          required property string modelData
          width: parent.width
          text: modelData
          checked: section.form.members.indexOf(modelData) >= 0
          onToggled: section.form.toggle(modelData, checked)
        }
      }
    }
    Column {
      width: (parent.width - Style.space(12)) / 2
      spacing: Style.space(8)
      Row {
        width: parent.width
        Txt {
          width: parent.width - clearButton.width
          text: I18n.t("proxyInGroup")
          font.pixelSize: Style.font.caption
          elide: Text.ElideRight
          anchors.verticalCenter: parent.verticalCenter
        }
        Btn {
          id: clearButton
          text: I18n.t("proxyClear")
          ghost: true
          onClicked: section.form.editor.setField("proxies", [])
        }
      }
      Repeater {
        model: section.form.members
        delegate: ProxyGroupMemberRow {
          form: section.form
        }
      }
      Txt {
        text: I18n.t("proxyProviders")
        faint: true
        font.pixelSize: Style.font.caption
      }
      Repeater {
        model: section.form.editor.metadata.providers || []
        ProxyCheck {
          required property string modelData
          width: parent.width
          text: modelData + " · " + section.form.nodes.filter(function (n) {
            return n.provider === modelData;
          }).length
          checked: (section.form.draft.use || []).indexOf(modelData) >= 0
          onToggled: section.form.editor.setField("use", Models.toggleMember(section.form.draft.use, modelData, checked))
        }
      }
    }
  }
  Btn {
    visible: !section.form.automatic
    ghost: true
    text: I18n.t("proxyAutomaticLink")
    onClicked: section.form.setAutomatic(true)
  }
  Row {
    visible: section.form.automatic
    width: parent.width
    spacing: Style.space(16)
    Column {
      width: (parent.width - Style.space(16)) * 0.62
      spacing: Style.space(10)
      Txt {
        text: I18n.t("proxySources")
      }
      ProxyCheck {
        text: I18n.t("proxyProfileNodes")
        checked: !!(section.form.draft["include-all"] || section.form.draft["include-all-proxies"])
        onToggled: {
          if (section.form.draft["include-all"])
            section.form.editor.setField("include-all-providers", true);
          section.form.editor.setField("include-all", undefined);
          section.form.editor.setField("include-all-proxies", checked);
        }
      }
      ProxyCheck {
        text: I18n.t("proxyAllProviders")
        checked: !!(section.form.draft["include-all"] || section.form.draft["include-all-providers"])
        onToggled: {
          if (section.form.draft["include-all"])
            section.form.editor.setField("include-all-proxies", true);
          section.form.editor.setField("include-all", undefined);
          section.form.editor.setField("include-all-providers", checked);
        }
      }
      Txt {
        text: I18n.t("proxyIncludeName")
      }
      Field {
        width: parent.width
        text: section.form.draft.filter || ""
        placeholder: "(?i)FI|NL|DE"
        input.onTextEdited: section.form.editor.setField("filter", text || undefined)
      }
      Flow {
        width: parent.width
        spacing: Style.space(4)
        Repeater {
          model: ["DE", "NL", "FI", "US", "JP"]
          Btn {
            required property string modelData
            text: modelData
            onClicked: section.form.quick("filter", modelData)
          }
        }
      }
      Txt {
        text: I18n.t("proxyExcludeName")
      }
      Field {
        width: parent.width
        text: section.form.draft["exclude-filter"] || ""
        input.onTextEdited: section.form.editor.setField("exclude-filter", text || undefined)
      }
      Flow {
        width: parent.width
        spacing: Style.space(4)
        Repeater {
          model: ["Expired", "Test", "Трафик", "Сайт"]
          Btn {
            required property string modelData
            text: modelData
            onClicked: section.form.quick("exclude-filter", modelData)
          }
        }
      }
      Txt {
        text: I18n.t("proxyExcludeTypes")
      }
      Flow {
        width: parent.width
        spacing: Style.space(4)
        Repeater {
          model: ["ss", "vmess", "vless", "trojan", "hysteria2", "wireguard"]
          Btn {
            required property string modelData
            text: modelData
            primary: String(section.form.draft["exclude-type"] || "").split("|").indexOf(modelData) >= 0
            onClicked: section.form.editor.setField("exclude-type", Models.toggleMember(String(section.form.draft["exclude-type"] || "").split("|").filter(function (t) {
              return t !== "";
            }), modelData, !primary).join("|") || undefined)
          }
        }
      }
    }
    Column {
      width: (parent.width - Style.space(16)) * 0.38
      spacing: Style.space(10)
      Txt {
        width: parent.width
        text: I18n.t("proxyFilterPreview").arg(section.form.preview.items.filter(function (n) {
          return !n.reason;
        }).length).arg(section.form.preview.items.length)
        bold: true
        wrapMode: Text.Wrap
      }
      Txt {
        visible: !!section.form.preview.error
        width: parent.width
        text: section.form.preview.error
        wrapMode: Text.Wrap
        color: Color.urgent
      }
      Repeater {
        model: section.form.preview.items
        Column {
          required property var modelData
          width: parent.width
          Txt {
            width: parent.width
            text: (parent.modelData.reason ? "× " : "✓ ") + parent.modelData.name
            dim: !!parent.modelData.reason
            elide: Text.ElideRight
            font.pixelSize: Style.font.bodySmall
          }
          Txt {
            visible: !!parent.modelData.reason
            text: I18n.t(parent.modelData.reason)
            faint: true
            font.pixelSize: Style.font.caption
          }
        }
      }
    }
  }
  Txt {
    visible: section.form.automatic
    width: parent.width
    text: I18n.t("proxyAutoHint")
    wrapMode: Text.Wrap
    dim: true
    font.pixelSize: Style.font.caption
  }
}
