pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons
import "ProxyModels.js" as Models

Column {
  id: root
  objectName: "proxy-group-form"
  required property var editor
  readonly property var draft: editor.draft
  property var nodes: []
  property var delays: ({})
  property string search: ""
  property string availableTab: "nodes"
  property bool automatic: !!(draft["include-all"] || draft["include-all-proxies"] || draft["include-all-providers"] || draft.filter)
  property bool healthAdvanced: false
  property bool customUrl: false
  property bool testing: false
  property string checkedAt: ""
  readonly property var members: draft.proxies || []
  readonly property var preview: Models.filterNodes(nodes, draft)
  readonly property var available: (availableTab === "nodes" ? nodes.filter(function (n) {
      return n.source !== "provider";
    }).map(function (n) {
      return n.name;
    }) : availableTab === "groups" ? (editor.metadata.groups || []) : ["DIRECT", "REJECT", "REJECT-DROP", "PASS", "COMPATIBLE"]).filter(function (n) {
    return n !== draft.name && n !== editor.original && (!search || n.toLowerCase().indexOf(search.toLowerCase()) >= 0);
  })
  readonly property var resultNames: members.concat(nodes.filter(function (n) {
    return (draft.use || []).indexOf(n.provider) >= 0;
  }).map(function (n) {
    return n.name;
  })).concat(preview.items.filter(function (n) {
    return !n.reason;
  }).map(function (n) {
    return n.name;
  })).filter(function (n, i, a) {
    return a.indexOf(n) === i;
  })
  spacing: Style.space(12)

  function delay(name) {
    if (delays[name] !== undefined)
      return delays[name];
    var n = nodes.filter(function (n) {
      return n.name === name;
    })[0];
    return n ? n.delay : null;
  }
  function label(name) {
    var d = delay(name);
    return d === 0 ? I18n.t("proxyNoResponse") : d === null || d === undefined ? "—" : d + " " + I18n.t("ms");
  }
  function reorder(from, to) {
    editor.setField("proxies", Models.moveMember(members, from, to));
  }
  function toggle(name, enabled) {
    editor.setField("proxies", Models.toggleMember(members, name, enabled));
  }
  function setAutomatic(value) {
    automatic = value;
    if (value) {
      editor.setField("include-all-proxies", true);
    } else {
      ["include-all", "include-all-proxies", "include-all-providers", "filter", "exclude-filter", "exclude-type"].forEach(function (k) {
        editor.setField(k, undefined);
      });
    }
  }
  function quick(key, value) {
    var old = String(draft[key] || "");
    editor.setField(key, old ? old + "|" + value : value);
  }
  function check() {
    if (testing)
      return;
    testing = true;
    var names = resultNames.slice(), pending = names.length, results = {};
    if (!pending) {
      testing = false;
      return;
    }
    names.forEach(function (name) {
      editor.svc.request("proxies.delay", {
        name: name,
        url: draft.url || "https://www.gstatic.com/generate_204",
        timeout: draft.timeout || 5000
      }, function (r, e) {
        if (e)
          editor.error = e.message;
        else
          results[name] = r.delay;
        if (--pending === 0) {
          delays = results;
          testing = false;
          checkedAt = new Date().toLocaleTimeString();
        }
      });
    });
  }
  function reloadNodes() {
    if (editor.svc)
      editor.svc.request("proxy_nodes.list", null, function (r, e) {
        if (!e && r)
          nodes = r.items || [];
      });
  }
  Component.onCompleted: reloadNodes()
  Connections {
    target: root.editor
    function onSvcChanged() {
      root.reloadNodes();
    }
  }

  Column {
    visible: root.editor.section === "basic"
    width: parent.width
    spacing: Style.space(12)
    Row {
      width: parent.width
      Txt {
        width: parent.width / 2
        text: I18n.t("proxyName")
      }
      Txt {
        width: parent.width / 2
        text: I18n.t("proxyNameHint")
        dim: true
        horizontalAlignment: Text.AlignRight
        font.pixelSize: Style.font.caption
      }
    }
    Field {
      width: parent.width
      text: root.draft.name || ""
      input.objectName: "proxy-group-name"
      input.onTextEdited: root.editor.setField("name", text)
    }
    Txt {
      text: I18n.t("proxyHowSelect")
    }
    Row {
      width: parent.width
      spacing: Style.space(8)
      Repeater {
        model: ["select", "url-test", "fallback", "load-balance"]
        Rectangle {
          id: typeCard
          objectName: "proxy-mode-" + modelData
          required property string modelData
          width: (parent.width - Style.space(24)) / 4
          height: Style.space(68)
          radius: Style.cornerRadius
          color: root.draft.type === modelData ? Style.selectedFillFor(Color.popups.text, Color.accent) : Util.alpha(Color.popups.text, 0.04)
          border.width: 1
          border.color: root.draft.type === modelData ? Color.accent : Util.alpha(Color.popups.text, 0.15)
          Column {
            anchors.fill: parent
            anchors.margins: Style.space(8)
            spacing: Style.space(5)
            Txt {
              width: parent.width
              text: I18n.t("proxyType_" + typeCard.modelData)
              font.pixelSize: Style.font.bodySmall
              elide: Text.ElideRight
            }
            Txt {
              width: parent.width
              text: I18n.t("proxyTypeHint_" + typeCard.modelData)
              font.pixelSize: Style.font.caption
              dim: true
              wrapMode: Text.Wrap
            }
          }
          MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: {
              root.editor.setField("type", typeCard.modelData);
              root.editor.issues = {};
            }
          }
        }
      }
    }
    Column {
      visible: root.draft.type === "select"
      width: parent.width
      spacing: Style.space(5)
      Txt {
        text: I18n.t("proxyDefault")
      }
      Txt {
        text: I18n.t("proxyDefaultHint")
        dim: true
        font.pixelSize: Style.font.caption
      }
      Repeater {
        model: root.resultNames
        Rectangle {
          required property string modelData
          width: parent.width
          height: Style.space(30)
          radius: Style.cornerRadius
          color: (root.draft["default-selected"] || root.members[0]) === modelData ? Style.selectedFillFor(Color.popups.text, Color.accent) : Util.alpha(Color.popups.text, 0.04)
          Txt {
            anchors.left: parent.left
            anchors.leftMargin: Style.space(10)
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - Style.space(90)
            text: ((root.draft["default-selected"] || root.members[0]) === modelData ? "◉  " : "○  ") + modelData
            elide: Text.ElideRight
          }
          Txt {
            anchors.right: parent.right
            anchors.rightMargin: Style.space(10)
            anchors.verticalCenter: parent.verticalCenter
            text: root.label(modelData)
            dim: true
            font.pixelSize: Style.font.caption
          }
          MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: root.editor.setField("default-selected", modelData)
          }
        }
      }
    }
    Column {
      visible: root.draft.type === "url-test"
      width: parent.width
      spacing: Style.space(8)
      Row {
        spacing: Style.space(10)
        Txt {
          text: I18n.t("proxyTolerance")
          anchors.verticalCenter: parent.verticalCenter
        }
        Field {
          width: Style.space(80)
          text: String(root.draft.tolerance === undefined ? 50 : root.draft.tolerance)
          input.validator: IntValidator {
            bottom: 0
            top: 60000
          }
          input.onTextEdited: {
            root.editor.issues = Models.put(root.editor.issues, "tolerance", input.acceptableInput ? "" : I18n.t("proxyFixFields"));
            if (input.acceptableInput)
              root.editor.setField("tolerance", Number(text));
          }
        }
        Txt {
          text: I18n.t("ms")
          anchors.verticalCenter: parent.verticalCenter
          dim: true
        }
      }
      Txt {
        width: parent.width
        text: I18n.t("proxyToleranceHint").arg(root.draft.tolerance === undefined ? 50 : root.draft.tolerance)
        wrapMode: Text.Wrap
        dim: true
        font.pixelSize: Style.font.bodySmall
      }
      Btn {
        text: I18n.t("proxyCheckNow")
        icon: Icons.test
        busy: root.testing
        onClicked: root.check()
      }
      Repeater {
        model: root.resultNames.slice().sort(function (a, b) {
          return (root.delay(a) || 999999) - (root.delay(b) || 999999);
        })
        Row {
          required property string modelData
          required property int index
          width: parent.width
          Txt {
            width: parent.width - Style.space(100)
            text: (parent.index + 1) + "   " + parent.modelData
            elide: Text.ElideRight
          }
          Txt {
            text: root.label(parent.modelData)
            dim: true
          }
        }
      }
    }
    Column {
      visible: root.draft.type === "fallback"
      width: parent.width
      spacing: Style.space(8)
      Txt {
        text: I18n.t("proxyOrder")
      }
      Txt {
        text: I18n.t("proxyOrderHint")
        dim: true
        font.pixelSize: Style.font.caption
      }
      Repeater {
        model: root.members
        delegate: ProxyGroupMemberRow {
          form: root
        }
      }
      Txt {
        width: parent.width
        text: I18n.t("proxyFallbackHint")
        wrapMode: Text.Wrap
        dim: true
        font.pixelSize: Style.font.bodySmall
      }
    }
    Column {
      visible: root.draft.type === "load-balance"
      width: parent.width
      spacing: Style.space(8)
      Txt {
        text: I18n.t("proxyStrategy")
      }
      Repeater {
        model: ["consistent-hashing", "round-robin", "sticky-sessions"]
        Rectangle {
          id: strategyCard
          required property string modelData
          readonly property bool selected: (root.draft.strategy || "consistent-hashing") === modelData
          width: parent.width
          height: strategyBody.implicitHeight + Style.space(20)
          radius: Style.cornerRadius
          color: selected ? Style.selectedFillFor(Color.popups.text, Color.accent) : Util.alpha(Color.popups.text, 0.04)
          border.width: 1
          border.color: selected ? Color.accent : Util.alpha(Color.popups.text, 0.15)
          Column {
            id: strategyBody
            x: Style.space(10)
            y: Style.space(10)
            width: parent.width - Style.space(20)
            spacing: Style.space(5)
            Txt {
              text: (strategyCard.selected ? "◉  " : "○  ") + I18n.t("proxyStrategy_" + strategyCard.modelData)
            }
            Txt {
              width: parent.width
              text: I18n.t("proxyStrategyHint_" + strategyCard.modelData)
              wrapMode: Text.Wrap
              dim: true
              font.pixelSize: Style.font.caption
            }
            Txt {
              text: strategyCard.modelData
              faint: true
              font.pixelSize: Style.font.caption
            }
          }
          MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: root.editor.setField("strategy", strategyCard.modelData)
          }
        }
      }
    }
    Rectangle {
      width: parent.width
      height: Style.space(42)
      radius: Style.cornerRadius
      color: Util.alpha(Color.popups.text, 0.04)
      Txt {
        anchors.left: parent.left
        anchors.leftMargin: Style.space(12)
        anchors.verticalCenter: parent.verticalCenter
        text: I18n.t("proxyMembers").arg(root.resultNames.length)
      }
      Btn {
        anchors.right: parent.right
        anchors.rightMargin: Style.space(6)
        anchors.verticalCenter: parent.verticalCenter
        ghost: true
        text: I18n.t("proxyEditMembers")
        onClicked: root.editor.section = "members"
      }
    }
  }

  ProxyGroupMembers {
    form: root
    visible: root.editor.section === "members"
    width: parent.width
  }

  ProxyGroupHealth {
    form: root
    visible: root.editor.section === "health"
    width: parent.width
  }
}
