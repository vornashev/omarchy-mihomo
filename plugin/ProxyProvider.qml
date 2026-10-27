pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons
import "ProxyModels.js" as Models

Item {
  id: root
  property var svc
  property var entry: ({})
  property var metadata: ({})
  property string loadedName: ""
  function loadDefinition() {
    if (!svc || !entry.name || entry.definition_available === false || loadedName === entry.name)
      return;
    loadedName = entry.name;
    svc.request("proxy_nodes.get", {
      profile_id: metadata.profile_id,
      name: entry.name,
      provider: entry.provider || ""
    }, function (r, e) {
      if (!e && r.items && r.items[0]) {
        metadata = r;
        entry = Object.assign({}, entry, r.items[0]);
      }
    });
  }
  onEntryChanged: Qt.callLater(loadDefinition)
  onSvcChanged: Qt.callLater(loadDefinition)
  property bool testing: false
  property string result: ""
  property var subscription: null
  property var providerDraft: ({})
  property bool saving: false
  property string providerError: ""
  property var providerIssues: ({})
  function openSubscription() {
    svc.request("proxy_providers.get", {
      profile_id: metadata.profile_id,
      name: entry.provider
    }, function (r, e) {
      if (e) {
        result = e.message;
        return;
      }
      subscription = r;
      providerDraft = r.definition;
    });
  }
  function saveSubscription() {
    saving = true;
    providerError = "";
    svc.request("proxy_providers.save", {
      profile_id: subscription.profile_id,
      revision: subscription.revision,
      name: entry.provider,
      definition: providerDraft
    }, function (r, e) {
      saving = false;
      if (e)
        providerError = e.message;
      else
        subscription = null;
    });
  }
  signal cancelled
  signal copyRequested
  signal subscriptionRequested(string provider)
  signal importRequested
  Column {
    width: parent.width
    spacing: Style.space(14)
    Btn {
      ghost: true
      text: I18n.t("navProxies") + " › " + I18n.t("proxyNodes")
      onClicked: root.cancelled()
    }
    Row {
      width: parent.width
      Column {
        width: parent.width - testButton.width
        Txt {
          width: parent.width
          text: root.entry.name || ""
          elide: Text.ElideRight
          bold: true
          font.pixelSize: Style.font.heading
        }
        Txt {
          text: (root.entry.type || "") + " · " + (root.entry.provider || "provider")
          dim: true
          font.pixelSize: Style.font.caption
        }
      }
      Btn {
        id: testButton
        text: I18n.t("check")
        busy: root.testing
        onClicked: {
          root.testing = true;
          root.svc.request("proxies.delay", {
            name: root.entry.name
          }, function (r, e) {
            root.testing = false;
            root.result = e ? e.message : r.delay + " " + I18n.t("ms");
          });
        }
      }
    }
    Banner {
      width: parent.width
      text: I18n.t("nodeProviderHint").arg(root.entry.provider || "provider")
    }
    Txt {
      text: I18n.t("nodeProviderReadOnly")
      color: Color.accent
    }
    Repeater {
      model: [
        {
          label: "nodeField_server",
          value: (root.entry.server || "—") + (root.entry.port ? ":" + root.entry.port : "")
        },
        {
          label: "proxyProtocol",
          value: root.entry.type || "—"
        },
        {
          label: "nodeSubscription",
          value: root.entry.provider || "—"
        },
        {
          label: "proxyDelay",
          value: root.result || (root.entry.delay ? root.entry.delay + " " + I18n.t("ms") : "—")
        }
      ]
      Row {
        required property var modelData
        width: parent.width
        Txt {
          width: parent.width * 0.3
          text: I18n.t(parent.modelData.label)
          dim: true
        }
        Txt {
          width: parent.width * 0.7
          text: parent.modelData.value
          elide: Text.ElideRight
        }
      }
    }
    Txt {
      visible: !!(root.entry.definition || {}).password
      text: I18n.t("nodeField_password") + "    ••••••••••••"
      dim: true
    }
    Flow {
      width: parent.width
      spacing: Style.space(6)
      Txt {
        text: I18n.t("proxyMemberOf")
        dim: true
      }
      Repeater {
        model: root.metadata.member_of || []
        Chip {
          required property string modelData
          text: modelData
        }
      }
    }
    Btn {
      visible: !!root.entry.provider
      text: I18n.t("nodeOpenSubscription").arg(root.entry.provider || "")
      onClicked: root.openSubscription()
    }
    Txt {
      visible: root.entry.definition_available === false
      width: parent.width
      text: I18n.t("nodeProviderUnavailable")
      wrapMode: Text.Wrap
      dim: true
    }
  }
  Row {
    anchors.bottom: parent.bottom
    anchors.right: parent.right
    spacing: Style.space(8)
    Btn {
      text: I18n.t("cancel")
      onClicked: root.cancelled()
    }
    Btn {
      text: root.entry.definition_available === false ? I18n.t("proxyImport") : I18n.t("proxyLocalCopy")
      primary: true
      onClicked: root.entry.definition_available === false ? root.importRequested() : root.copyRequested()
    }
  }
  ProxyDialog {
    visible: !!root.subscription
    title: I18n.t("nodeSubscription") + " · " + (root.entry.provider || "")
    onClosed: root.subscription = null
    Repeater {
      model: [
        {
          key: "url",
          kind: "string"
        },
        {
          key: "interval",
          kind: "number"
        },
        {
          key: "filter",
          kind: "string"
        },
        {
          key: "exclude-filter",
          kind: "string"
        }
      ]
      SchemaField {
        required property var modelData
        width: parent.width
        field: modelData
        value: root.providerDraft[field.key]
        onIssue: function (e) {
          root.providerIssues = Models.put(root.providerIssues, field.key, e);
        }
        onEdited: function (v) {
          root.providerDraft = Models.put(root.providerDraft, field.key, v);
        }
      }
    }
    Txt {
      visible: root.providerError !== ""
      width: parent.width
      text: root.providerError
      wrapMode: Text.Wrap
      color: Color.urgent
    }
    Row {
      spacing: Style.space(8)
      Btn {
        text: I18n.t("cancel")
        enabled: !root.saving
        onClicked: root.subscription = null
      }
      Btn {
        text: I18n.t("saveApply")
        busy: root.saving
        enabled: !root.saving
        primary: true
        onClicked: if (!Object.values(root.providerIssues).some(function (e) {
          return !!e;
        }))
          root.saveSubscription()
      }
    }
  }
}
