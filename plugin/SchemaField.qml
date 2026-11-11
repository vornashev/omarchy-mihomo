pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import qs.Commons
import qs.Commons as Commons
import "ProxyModels.js" as Models

// Recursive schema-driven input; unset is different from false, zero or empty.
Column {
  id: root
  objectName: "proxy-field-" + field.key
  property var field: ({
      key: "",
      kind: "string"
    })
  property string label: MihomoI18n.t("nodeField_" + field.key) === "nodeField_" + field.key ? field.key : MihomoI18n.t("nodeField_" + field.key)
  property bool compact: false
  property var value
  property var context: ({})
  property var targets: []
  property var targetGroups: []
  property var targetDetails: ({})
  property var providers: []
  property bool expanded: false
  property bool reveal: false
  property string localError: ""
  property var childErrors: ({})
  property int nextChildId: 0
  property bool opened: expanded
  readonly property string errorText: localError || Object.values(childErrors).filter(function (e) {
    return e !== "";
  }).join(" · ")
  onErrorTextChanged: issue(errorText)
  onExpandedChanged: if (expanded)
    opened = true
  property var mapKeys: []
  onValueChanged: {
    localError = "";
    if (field.kind === "map") {
      var next = Object.keys(value || {});
      if (JSON.stringify(next) !== JSON.stringify(mapKeys))
        mapKeys = next;
    }
  }
  onFieldChanged: if (field.kind === "map")
    mapKeys = Object.keys(value || {})
  // Mihomo also accepts base64 strings for byte lists such as WireGuard reserved.
  readonly property bool scalarList: field.kind === "list" && present && typeof value === "string"
  readonly property bool inlineList: compact && field.kind === "list" && ["string", "number"].indexOf((field.item || {}).kind) >= 0
  readonly property bool complex: !scalarList && !inlineList && ["object", "list", "map"].indexOf(field.kind) >= 0
  readonly property bool present: value !== undefined && value !== null
  readonly property var options: field.key === "use" ? providers : (["dialer-proxy", "default-selected", "empty-fallback"].indexOf(field.key) >= 0 ? targets : (field.choices || []))
  signal edited(var next)
  signal issue(string message)
  spacing: Style.space(5)
  function setError(message) {
    localError = message;
  }
  function setChildError(key, message) {
    childErrors = Models.put(childErrors, key, message || undefined);
  }
  function update(next) {
    setError("");
    if (next === undefined)
      childErrors = {};
    edited(next);
  }
  function modify(key, next) {
    update(Models.put(value, key, next));
  }
  function editItem(index, next) {
    var a = Models.clone(value) || [];
    a[index] = next;
    update(a);
  }
  function removeItem(index) {
    var a = Models.clone(value) || [];
    a.splice(index, 1);
    update(a);
  }
  function moveItem(index, step) {
    var a = Models.clone(value) || [];
    var other = index + step;
    if (other < 0 || other >= a.length)
      return;
    var t = a[index];
    a[index] = a[other];
    a[other] = t;
    update(a);
  }

  Row {
    width: parent.width
    height: root.compact ? Style.space(18) : implicitHeight
    spacing: Style.space(5)
    Btn {
      height: root.compact ? Style.space(18) : implicitHeight
      visible: root.complex
      text: root.expanded ? "−" : "+"
      tooltip: MihomoI18n.t(root.expanded ? "hintCollapse" : "hintExpand")
      ghost: true
      onClicked: root.expanded = !root.expanded
    }
    Txt {
      text: root.label + (root.field.required && !root.compact ? " *" : "")
      width: parent.width - Style.space(root.complex ? 120 : 90)
      anchors.verticalCenter: parent.verticalCenter
      dim: true
      font.pixelSize: Style.font.body
    }
    Btn {
      height: root.compact ? Style.space(18) : implicitHeight
      visible: !!root.field.secret
      text: root.reveal ? "◉" : "◎"
      tooltip: MihomoI18n.t(root.reveal ? "hintHideSecret" : "hintShowSecret")
      ghost: true
      onClicked: root.reveal = !root.reveal
    }
    Btn {
      height: root.compact ? Style.space(18) : implicitHeight
      visible: root.present && !root.field.required
      text: "×"
      tooltip: MihomoI18n.t("hintRemoveField")
      ghost: true
      onClicked: {
        root.expanded = false;
        root.update(undefined);
      }
    }
  }

  Loader {
    width: parent.width
    active: !root.complex || root.opened
    visible: !root.complex || root.expanded
    sourceComponent: root.scalarList ? stringInput : root.inlineList ? inlineListInput : root.field.kind === "boolean" ? boolInput : root.field.kind === "object" ? objectInput : root.field.kind === "list" ? listInput : root.field.kind === "map" ? mapInput : root.options.length ? choiceInput : root.field.multiline || root.field.kind === "json" ? multilineInput : stringInput
  }
  Txt {
    visible: root.errorText !== ""
    text: root.errorText
    width: parent.width
    wrapMode: Text.Wrap
    color: Commons.Color.urgent
    font.pixelSize: Style.font.body
  }

  Component {
    id: inlineListInput
    Field {
      width: parent.width
      text: root.present ? Array.prototype.join.call(root.value, ", ") : ""
      placeholder: MihomoI18n.t("nodeDefault")
      input.objectName: "proxy-input-" + root.field.key
      input.onTextEdited: {
        var values = text.split(",").map(function (v) {
          return v.trim();
        }).filter(function (v) {
          return v !== "";
        });
        if ((root.field.item || {}).kind === "number") {
          if (values.some(function (v) {
            return !/^-?[0-9]+$/.test(v) || !Number.isSafeInteger(Number(v));
          })) {
            root.setError(MihomoI18n.t("proxyInteger"));
            return;
          }
          values = values.map(Number);
        }
        root.update(text.trim() ? values : undefined);
      }
    }
  }
  Component {
    id: stringInput
    Field {
      width: parent.width
      text: root.present ? String(root.value) : ""
      placeholder: MihomoI18n.t("nodeDefault")
      input.echoMode: root.field.secret && !root.reveal ? TextInput.Password : TextInput.Normal
      input.selectByMouse: true
      input.objectName: "proxy-input-" + root.field.key
      input.onTextEdited: {
        if (text === "") {
          root.update(undefined);
          return;
        }
        if (root.field.kind === "number") {
          if (!/^-?[0-9]+$/.test(text) || !Number.isSafeInteger(Number(text))) {
            root.setError(MihomoI18n.t("proxyInteger"));
            return;
          }
          root.update(Number(text));
        } else
          root.update(text);
      }
    }
  }
  Component {
    id: boolInput
    ProxyChoice {
      width: parent.width
      model: [MihomoI18n.t("nodeDefault"), MihomoI18n.t("on"), MihomoI18n.t("off")]
      currentIndex: !root.present ? 0 : root.value ? 1 : 2
      onActivated: root.update(currentIndex === 0 ? undefined : currentIndex === 1)
    }
  }
  Component {
    id: choiceInput
    ProxyChoice {
      id: choice
      width: parent.width
      editable: true
      searchable: true
      groupNames: root.targetGroups.filter(function(n) { return root.options.indexOf(n) >= 0; })
      optionDetails: root.targetDetails
      model: [""].concat(root.options)
      currentIndex: Math.max(0, model.indexOf(root.value || ""))
      editText: root.present ? String(root.value) : ""
      onActivated: root.update(currentText || undefined)
      onAccepted: root.update(editText || undefined)
      Connections {
        target: choice.contentItem
        function onTextEdited() {
          root.update(choice.editText || undefined);
        }
      }
    }
  }
  Component {
    id: multilineInput
    Column {
      width: parent.width
      spacing: Style.space(4)
      TextArea {
        width: parent.width
        height: Style.space(100)
        wrapMode: TextEdit.WrapAnywhere
        selectByMouse: true
        color: Commons.Color.popups.text
        font.family: Style.font.family
        font.pixelSize: Style.font.body
        background: Rectangle {
          color: Util.alpha(Commons.Color.popups.text, 0.04)
          radius: Style.cornerRadius
          border.width: 1
          border.color: Util.alpha(Commons.Color.popups.text, 0.16)
        }
        text: root.field.secret && !root.reveal && root.present ? "••••••••" : !root.present ? "" : root.field.kind === "json" ? JSON.stringify(root.value, null, 2) : String(root.value)
        readOnly: !!root.field.secret && !root.reveal
        onTextChanged: if (activeFocus && !readOnly) {
          if (root.field.kind === "json") {
            try {
              root.update(text.trim() ? JSON.parse(text) : undefined);
            } catch (e) {
              root.setError(MihomoI18n.t("proxyInvalidJson"));
            }
          } else
            root.update(text || undefined);
        }
      }
    }
  }
  Component {
    id: objectInput
    Column {
      width: parent.width
      spacing: Style.space(10)
      Repeater {
        model: root.field.fields || []
        Loader {
          required property var modelData
          property int issueId: -1
          width: parent.width
          active: Models.visible(modelData, root.value || {})
          onActiveChanged: if (!active)
            root.setChildError(issueId, "")
          Component.onDestruction: root.setChildError(issueId, "")
          source: "SchemaField.qml"
          onLoaded: {
            root.setChildError(issueId, "");
            issueId = root.nextChildId++;
            item.compact = root.compact;
            item.field = modelData;
            item.value = Qt.binding(function () {
              return (root.value || {})[modelData.key];
            });
            item.context = Qt.binding(function () {
              return root.value || {};
            });
            item.targets = root.targets;
            item.targetGroups = Qt.binding(function() { return root.targetGroups; });
            item.targetDetails = Qt.binding(function() { return root.targetDetails; });
            item.providers = root.providers;
            item.edited.connect(function (v) {
              root.modify(modelData.key, v);
            });
            item.issue.connect(function (e) {
              root.setChildError(issueId, e);
            });
          }
        }
      }
    }
  }
  Component {
    id: listInput
    Column {
      width: parent.width
      spacing: Style.space(7)
      Repeater {
        model: (root.value || []).length
        Column {
          id: listRow
          required property int index
          width: parent.width
          spacing: Style.space(4)
          Row {
            spacing: Style.space(5)
            Txt {
              text: String(listRow.index + 1)
              dim: true
              anchors.verticalCenter: parent.verticalCenter
            }
            Btn {
              text: "↑"
              tooltip: MihomoI18n.t("hintMoveUp")
              ghost: true
              enabled: listRow.index > 0
              onClicked: root.moveItem(listRow.index, -1)
            }
            Btn {
              text: "↓"
              tooltip: MihomoI18n.t("hintMoveDown")
              ghost: true
              enabled: listRow.index < (root.value || []).length - 1
              onClicked: root.moveItem(listRow.index, 1)
            }
            Btn {
              text: "×"
              tooltip: MihomoI18n.t("hintRemoveItem")
              ghost: true
              onClicked: root.removeItem(listRow.index)
            }
          }
          Loader {
            property int issueId: -1
            width: parent.width
            source: "SchemaField.qml"
            Component.onDestruction: root.setChildError(issueId, "")
            onLoaded: {
              issueId = root.nextChildId++;
              var f = Object.assign({
                key: root.field.key === "proxies" ? MihomoI18n.t("proxyMember") : MihomoI18n.t("proxyValue")
              }, root.field.item || {
                kind: "string"
              });
              if (root.field.key === "proxies")
                f.choices = root.targets;
              if (root.field.key === "use")
                f.choices = root.providers;
              item.compact = root.compact;
              item.field = f;
              item.value = Qt.binding(function () {
                return (root.value || [])[listRow.index];
              });
              item.targets = root.targets;
              item.targetGroups = Qt.binding(function() { return root.targetGroups; });
              item.targetDetails = Qt.binding(function() { return root.targetDetails; });
              item.providers = root.providers;
              item.expanded = true;
              item.edited.connect(function (v) {
                root.editItem(listRow.index, v);
              });
              item.issue.connect(function (e) {
                root.setChildError(issueId, e);
              });
            }
          }
        }
      }
      Btn {
        text: MihomoI18n.t("proxyAddItem")
        onClicked: {
          var a = Models.clone(root.value) || [];
          a.push(Models.fresh(root.field.item || {
            kind: "string"
          }));
          root.update(a);
        }
      }
    }
  }
  Component {
    id: mapInput
    Column {
      width: parent.width
      spacing: Style.space(8)
      Repeater {
        model: root.mapKeys
        Loader {
          required property string modelData
          property int issueId: -1
          width: parent.width
          source: "SchemaField.qml"
          Component.onDestruction: root.setChildError(issueId, "")
          onLoaded: {
            issueId = root.nextChildId++;
            item.field = Object.assign({
              key: modelData
            }, root.field.item || {
              kind: "string"
            });
            item.value = Qt.binding(function () {
              return (root.value || {})[modelData];
            });
            item.targets = root.targets;
            item.targetGroups = Qt.binding(function() { return root.targetGroups; });
            item.targetDetails = Qt.binding(function() { return root.targetDetails; });
            item.providers = root.providers;
            item.edited.connect(function (v) {
              root.modify(modelData, v);
            });
            item.issue.connect(function (e) {
              root.setChildError(issueId, e);
            });
          }
        }
      }
      Row {
        width: parent.width
        spacing: Style.space(7)
        Field {
          id: mapKey
          width: parent.width - addKey.width - Style.space(7)
          placeholder: MihomoI18n.t("proxyKey")
        }
        Btn {
          id: addKey
          text: "+"
          tooltip: MihomoI18n.t("hintAddField")
          enabled: mapKey.text.trim() !== ""
          onClicked: {
            root.modify(mapKey.text.trim(), Models.fresh(root.field.item || {
              kind: "string"
            }));
            mapKey.text = "";
          }
        }
      }
    }
  }
}
