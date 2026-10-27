import QtQuick
import QtQuick.Controls
import qs.Commons
import "ProxyModels.js" as Models

// Header of the proxies page: title and tabs, search and filters, bulk
// actions on the selection, and the import and create buttons.
Column {
  id: bar
  // The proxies page whose filters, selection and actions this drives.
  property var page
  property alias searchField: searchField
  spacing: Style.space(9)
  Row {
    width: parent.width
    spacing: Style.space(8)
    Column {
      width: parent.width - importButton.width - createButton.width - Style.space(16)
      spacing: Style.space(4)
      Txt {
        text: I18n.t("navProxies")
        bold: true
        font.pixelSize: Style.font.heading
      }
      Txt {
        width: parent.width
        text: (bar.page.metadata.profile_name || I18n.t("proxyNoProfile")) + " · " + I18n.t("proxyProfileHint")
        dim: true
        elide: Text.ElideRight
        font.pixelSize: Style.font.caption
      }
    }
    Btn {
      id: importButton
      text: I18n.t("proxyImport")
      icon: Icons.down
      enabled: !!bar.page.metadata.profile_id
      onClicked: bar.page.stage = "import"
    }
    Btn {
      id: createButton
      text: I18n.t(bar.page.group ? "proxyGroup" : "proxyNode")
      icon: Icons.add
      primary: true
      enabled: !!bar.page.metadata.profile_id && !bar.page.busy
      onClicked: bar.page.create()
    }
  }
  Row {
    width: parent.width
    spacing: Style.space(8)
    Seg {
      current: bar.page.group ? "groups" : "nodes"
      model: [
        {
          key: "groups",
          label: I18n.t("proxyGroups") + " · " + bar.page.groupCount
        },
        {
          key: "nodes",
          label: I18n.t("proxyNodes") + " · " + bar.page.nodeCount
        }
      ]
      onPicked: function (k) {
        bar.page.switchTab(k === "groups");
      }
    }
    Btn {
      visible: bar.page.group
      text: I18n.t("testAll")
      icon: Icons.test
      onClicked: bar.page.visibleItems.filter(function (i) {
        return i.source !== "builtin";
      }).forEach(function (i) {
        bar.page.test(i);
      })
    }
  }
  Row {
    width: parent.width
    spacing: Style.space(6)
    Field {
      id: searchField
      width: bar.page.group ? parent.width : (parent.width - Style.space(12)) * 0.46
      placeholder: I18n.t(bar.page.group ? "proxyGroupSearch" : "proxyNodeSearch")
      icon: Icons.search
      text: bar.page.search
      input.onTextEdited: bar.page.search = text
    }
    ProxyChoice {
      visible: !bar.page.group
      width: (parent.width - Style.space(12)) * 0.25
      model: [I18n.t("proxyAllTypes")].concat(bar.page.catalog.nodes.map(function (s) {
        return s.type;
      }))
      currentIndex: Math.max(0, model.indexOf(bar.page.protocolFilter))
      onActivated: bar.page.protocolFilter = currentIndex ? currentText : ""
    }
    ProxyChoice {
      visible: !bar.page.group
      width: (parent.width - Style.space(12)) * 0.29
      model: [I18n.t("proxyAllSources"), I18n.t("proxySourceProfile"), I18n.t("proxySourceModified"), I18n.t("proxySourceLocal"), I18n.t("proxySourceProvider")]
      currentIndex: ["", "profile", "modified", "local", "provider"].indexOf(bar.page.sourceFilter)
      onActivated: bar.page.sourceFilter = ["", "profile", "modified", "local", "provider"][currentIndex]
    }
  }
  Row {
    visible: !bar.page.group
    spacing: Style.space(8)
    Btn {
      text: I18n.t("proxyUnresponsive") + " · " + (bar.page.metadata.items || []).filter(function (i) {
        return i.delay === 0;
      }).length
      primary: bar.page.failedOnly
      onClicked: bar.page.failedOnly = !bar.page.failedOnly
    }
    Btn {
      text: I18n.t("proxySortDelay")
      primary: bar.page.sortDelay
      onClicked: bar.page.sortDelay = !bar.page.sortDelay
    }
  }
  Row {
    visible: !bar.page.group && bar.page.selection.length > 0
    spacing: Style.space(8)
    Txt {
      text: I18n.t("proxySelected").arg(bar.page.selection.length)
      anchors.verticalCenter: parent.verticalCenter
      font.pixelSize: Style.font.bodySmall
    }
    Btn {
      text: I18n.t("proxyCheckNow")
      onClicked: bar.page.selection.forEach(function (i) {
        bar.page.test(i);
      })
    }
    Btn {
      text: I18n.t("proxyAddToGroup")
      onClicked: {
        bar.page.addGroup = (bar.page.metadata.groups || [])[0] || "";
        bar.page.addingToGroup = true;
      }
    }
    Btn {
      text: I18n.t("delete")
      enabled: bar.page.selection.some(function (i) {
        return i.source !== "provider";
      })
      onClicked: bar.page.removeSelected()
    }
  }
  Row {
    visible: !bar.page.group
    width: parent.width
    leftPadding: Style.space(28)
    Repeater {
      model: [
        {
          label: "proxyNode",
          fraction: 0.35
        },
        {
          label: "proxyProtocolColumn",
          fraction: 0.16
        },
        {
          label: "proxyDelay",
          fraction: 0.16
        },
        {
          label: "proxyGroupColumn",
          fraction: 0.16
        },
        {
          label: "proxySourceColumn",
          fraction: 0.17
        }
      ]
      Txt {
        required property var modelData
        width: (parent.width - Style.space(28)) * modelData.fraction
        text: I18n.t(modelData.label).toUpperCase()
        faint: true
        font.pixelSize: Style.font.caption
        elide: Text.ElideRight
      }
    }
  }
  Banner {
    width: parent.width
    visible: bar.page.svc.coreRunning === false || bar.page.pendingApply
    text: I18n.t(bar.page.pendingApply ? "nodeSavedOffline" : "nodeCoreStoppedHint")
  }
  Btn {
    visible: bar.page.svc.coreRunning === false
    text: I18n.t("nodeStartCore")
    onClicked: bar.page.svc.request("core.start", null, function (r, e) {
      if (e)
        bar.page.error = e.message;
      else {
        bar.page.pendingApply = false;
        bar.page.reload();
      }
    })
  }
  Row {
    visible: bar.page.busy
    width: parent.width
    spacing: Style.space(8)
    Repeater {
      model: 3
      Rectangle {
        width: (parent.width - Style.space(16)) / 3
        height: Style.space(24)
        radius: Style.cornerRadius
        color: Util.alpha(Color.popups.text, 0.08)
      }
    }
  }
  Txt {
    text: bar.page.error
    visible: text !== ""
    width: parent.width
    wrapMode: Text.Wrap
    color: Color.urgent
    font.pixelSize: Style.font.bodySmall
  }
  Btn {
    visible: !bar.page.metadata.profile_id
    text: I18n.t("navProfiles")
    onClicked: bar.page.panel.showPage("profiles")
  }
  Row {
    visible: (bar.page.metadata.removed || []).length > 0
    spacing: Style.space(7)
    ProxyChoice {
      id: removed
      model: bar.page.metadata.removed || []
      width: Style.space(220)
    }
    Btn {
      text: I18n.t("proxyRestore")
      enabled: !bar.page.busy
      onClicked: bar.page.mutate("reset", removed.currentText, "")
    }
  }
}
