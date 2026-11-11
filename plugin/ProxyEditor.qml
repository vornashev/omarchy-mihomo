pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import qs.Commons
import qs.Commons as Commons
import "ProxyModels.js" as Models
import "ProxyNodeModels.js" as Nodes

Item {
  id: root
  property var svc
  property var metadata: ({})
  property var targetDetails: Models.choiceDetails(metadata.items, [])
  property var panel
  property var originalDraft: ({})
  property var baselineMembers: []
  property var conflict: null
  property bool compareYaml: false
  property string remoteYaml: ""
  property string localYaml: ""
  property string applyState: "idle"
  property bool lastReloaded: false
  property bool leaveAfterSave: false
  property bool testing: false
  property string testResult: ""
  property bool renaming: false
  property string renameValue: ""
  property var history: []
  property bool historyBusy: false
  readonly property var changes: Nodes.diff(originalDraft, draft)
  readonly property var missingMembers: (draft.proxies || []).filter(function (n) {
    return (metadata.targets || []).indexOf(n) < 0;
  })
  function sectionLabel(key) {
    return key === "peers" ? MihomoI18n.t("nodePeers") + " · " + (draft.peers || []).length : key === "amnezia" ? "AmneziaWG" : MihomoI18n.t("proxySection_" + key);
  }
  function captureBaseline() {
    originalDraft = Models.clone(draft);
    baselineMembers = memberOf.slice();
  }
  Component.onCompleted: captureBaseline()
  function testNode() {
    if (!original || dirty) {
      validate(function () {
        testResult = MihomoI18n.t("proxyValid");
      });
      return;
    }
    testing = true;
    svc.request("proxies.delay", {
      name: original
    }, function (r, e) {
      testing = false;
      testResult = e ? e.message : r.delay === 0 ? MihomoI18n.t("proxyNoResponse") : r.delay + " " + MihomoI18n.t("ms");
    });
  }
  function fetchConflict() {
    svc.request(group ? "proxy_groups.get" : "proxy_nodes.get", {
      profile_id: metadata.profile_id,
      name: original
    }, function (r, e) {
      if (e) {
        error = e.message;
        return;
      }
      conflict = r;
      remoteYaml = r.items[0].yaml || "";
      compareYaml = false;
      localYaml = yamlMode ? yamlText : "";
      if (!yamlMode)
        svc.request("proxy_config.convert", {
          definition: draft
        }, function (converted, err) {
          if (!err)
            localYaml = converted.yaml;
        });
    });
  }
  function resolveConflict(keepMine) {
    if (!conflict)
      return;
    metadata = conflict;
    if (!keepMine) {
      draft = Models.clone(conflict.items[0].definition);
      memberOf = conflict.member_of || [];
      yamlText = remoteYaml;
      dirty = false;
      issues = {};
      captureBaseline();
    }
    conflict = null;
    error = "";
    applyState = "idle";
    if (keepMine)
      save();
  }
  function loadHistory() {
    renaming = true;
    renameValue = draft.name || "";
    historyBusy = true;
    svc.request(group ? "proxy_groups.history" : "proxy_nodes.history", {
      profile_id: metadata.profile_id,
      name: original
    }, function (r, e) {
      historyBusy = false;
      if (e)
        error = e.message;
      else
        history = r.items || [];
    });
  }
  function restoreVersion(version) {
    draft = Models.clone(version.definition);
    memberOf = version.member_of || memberOf;
    yamlMode = false;
    issues = {};
    changed();
    renaming = false;
  }
  function copyError() {
    clipboard.text = error;
    clipboard.selectAll();
    clipboard.copy();
    clipboard.deselect();
  }
  TextEdit {
    id: clipboard
    visible: false
  }
  function undo() {
    if (!original || !metadata.revision)
      return;
    if (!originalDraft.name) {
      busy = true;
      svc.request(group ? "proxy_groups.remove" : "proxy_nodes.remove", {
        profile_id: metadata.profile_id,
        revision: metadata.revision,
        name: original
      }, function (r, e) {
        busy = false;
        if (e)
          saveFailed(e);
        else
          saved(r.reloaded);
      });
      return;
    }
    draft = Models.clone(originalDraft);
    memberOf = baselineMembers.slice();
    yamlMode = false;
    dirty = true;
    save();
  }

  property var catalog: ({})
  property bool group: false
  property string original: ""
  property var draft: ({})
  property var memberOf: []
  property string yamlText: ""
  property bool yamlMode: false
  property bool dirty: false
  property bool busy: false
  property bool discarding: false
  property string error: ""
  property var issues: ({})
  property string section: "basic"
  property string fieldSearch: ""
  readonly property bool typing: true
  readonly property var schema: Models.schema(catalog, group, draft.type)
  readonly property var choices: (group ? catalog.groups : catalog.nodes) || []
  signal saved(bool reloaded)
  signal cancelled
  signal draftChangedByUser

  function changed() {
    dirty = true;
    applyState = "idle";
    draftChangedByUser();
  }
  function setField(key, value) {
    draft = Models.put(draft, key, value);
    changed();
  }
  function params() {
    var p = {
      profile_id: metadata.profile_id,
      revision: metadata.revision,
      name: original,
      operation: "save",
      group: group
    };
    if (yamlMode)
      p.yaml = yamlText;
    else
      p.definition = draft;
    if (!group)
      p.member_of = memberOf;
    return p;
  }
  function validate(callback) {
    if (Object.keys(issues).some(function (k) {
      return issues[k] !== "";
    })) {
      error = MihomoI18n.t("proxyFixFields");
      return;
    }
    busy = true;
    error = "";
    svc.request("proxy_config.preview", params(), function (r, e) {
      busy = false;
      if (e) {
        error = e.message;
        return;
      }
      if (callback)
        callback(r);
    });
  }
  function switchMode() {
    if (Object.keys(issues).some(function (k) {
      return issues[k] !== "";
    })) {
      error = MihomoI18n.t("proxyFixFields");
      return;
    }
    // Syntax conversion is independent of full configuration validity.
    busy = true;
    error = "";
    var p = yamlMode ? {
      yaml: yamlText
    } : {
      definition: draft
    };
    svc.request("proxy_config.convert", p, function (r, e) {
      busy = false;
      if (e) {
        error = e.message;
        return;
      }
      draft = r.definition;
      yamlText = r.yaml;
      yamlMode = !yamlMode;
      issues = {};
    });
  }
  function save() {
    if (Object.keys(issues).some(function (k) {
      return issues[k] !== "";
    })) {
      error = MihomoI18n.t("proxyFixFields");
      return;
    }
    busy = true;
    error = "";
    applyState = "checking";
    var request = params();
    svc.request("proxy_config.preview", request, function (valid, e) {
      if (e) {
        root.saveFailed(e);
        return;
      }
      if (root.yamlMode && valid.definition)
        root.draft = valid.definition;
      applyState = "applying";
      svc.request(group ? "proxy_groups.save" : "proxy_nodes.save", request, function (r, e) {
        busy = false;
        if (e) {
          root.saveFailed(e);
          return;
        }
        dirty = false;
        lastReloaded = r.reloaded;
        applyState = r.reloaded ? "success" : "offline";
        if (r.revision)
          metadata = Models.put(metadata, "revision", r.revision);
        original = draft.name || original;
        if (leaveAfterSave) {
          leaveAfterSave = false;
          saved(r.reloaded);
        }
      });
    });
  }
  function saveFailed(e) {
    busy = false;
    error = e.message;
    applyState = e.message.indexOf("restoration failed") >= 0 ? "restore-failed" : "error";
    if (e.code === "conflict")
      fetchConflict();
  }

  function leave() {
    if (dirty) {
      discarding = true;
      return false;
    }
    if (applyState === "success" || applyState === "offline")
      saved(lastReloaded);
    else
      cancelled();
    return true;
  }

  Column {
    id: head
    width: parent.width
    spacing: Style.space(9)
    Row {
      spacing: Style.space(6)
      Txt {
        text: MihomoI18n.t("navProxies")
        color: Commons.Color.accent
        font.pixelSize: Style.font.bodySmall
        MouseArea {
          anchors.fill: parent
          cursorShape: Qt.PointingHandCursor
          onClicked: root.leave()
        }
      }
      Txt {
        text: "› " + MihomoI18n.t(root.group ? "proxyGroups" : "proxyNodes") + " › " + (root.draft.name || "")
        dim: true
        anchors.verticalCenter: parent.verticalCenter
        font.pixelSize: Style.font.bodySmall
      }
    }
    Row {
      width: parent.width
      spacing: Style.space(8)
      Column {
        width: Math.max(0, parent.width - yamlButton.width - unsaved.width - testButton.width - renameButton.width - Style.space(32))
        spacing: Style.space(4)
        Txt {
          text: root.draft.name || MihomoI18n.t("proxyCreate")
          width: parent.width
          elide: Text.ElideRight
          bold: true
          font.pixelSize: Style.font.heading
        }
        Txt {
          width: parent.width
          text: root.group ? MihomoI18n.t("proxyTypeHint_" + root.draft.type) : MihomoI18n.t("nodeDescription_" + root.draft.type)
          dim: true
          font.pixelSize: Style.font.bodySmall
          wrapMode: Text.Wrap
        }
      }
      Txt {
        id: unsaved
        text: root.dirty ? MihomoI18n.t("proxyUnsavedBadge") : ""
        color: Commons.Color.urgent
        anchors.verticalCenter: parent.verticalCenter
        font.pixelSize: Style.font.bodySmall
      }
      Btn {
        id: testButton
        visible: !root.group && !Nodes.service(root.draft.type)
        width: visible ? implicitWidth : 0
        text: MihomoI18n.t("check")
        icon: Icons.test
        busy: root.testing
        enabled: !root.busy
        onClicked: root.testNode()
      }
      Btn {
        id: renameButton
        visible: !!root.original
        width: visible ? implicitWidth : 0
        ghost: true
        icon: Icons.history
        tooltip: MihomoI18n.t("nodeHistory")
        onClicked: root.loadHistory()
      }
      Btn {
        id: yamlButton
        text: root.yamlMode ? MihomoI18n.t("proxyForm") : "YAML"
        enabled: !root.busy
        onClicked: root.switchMode()
      }
    }
    ProxyTabs {
      visible: !root.yamlMode
      width: parent.width
      current: root.section
      model: root.group ? [
        {
          key: "basic",
          label: MihomoI18n.t("proxySection_basic")
        },
        {
          key: "members",
          label: MihomoI18n.t("proxyMembers").arg((root.draft.proxies || []).length)
        },
        {
          key: "health",
          label: MihomoI18n.t("proxyHealthTab")
        },
        {
          key: "advanced",
          label: MihomoI18n.t("proxySection_advanced")
        }
      ].concat([]) : Nodes.sections(root.schema, root.draft.type).map(function (k) {
        return {
          key: k,
          label: root.sectionLabel(k)
        };
      })
      onPicked: function (k) {
        root.section = k;
      }
    }
    Txt {
      visible: root.catalog.supported === false
      text: MihomoI18n.t("proxyOldCore")
      width: parent.width
      wrapMode: Text.Wrap
      color: Commons.Color.urgent
    }
    Txt {
      text: root.error
      visible: text !== ""
      width: parent.width
      wrapMode: Text.Wrap
      color: Commons.Color.urgent
      font.pixelSize: Style.font.body
    }
    Flow {
      visible: root.error !== "" && root.applyState !== "idle"
      width: parent.width
      spacing: Style.space(6)
      Btn {
        text: "YAML"
        onClicked: if (!root.yamlMode)
          root.switchMode()
      }
      Btn {
        text: MihomoI18n.t("nodeCopyLog")
        onClicked: root.copyError()
      }
      Btn {
        visible: root.group && root.missingMembers.length > 0
        text: MihomoI18n.t("nodeMissing")
        onClicked: root.setField("proxies", (root.draft.proxies || []).filter(function (n) {
          return root.missingMembers.indexOf(n) < 0;
        }))
      }
      Btn {
        visible: root.applyState === "restore-failed"
        text: MihomoI18n.t("navLogs")
        onClicked: if (root.panel)
          root.panel.showPage("logs")
      }
      Btn {
        visible: root.applyState === "restore-failed"
        text: MihomoI18n.t("nodeStartCore")
        onClicked: root.svc.request("core.restart", null, function (r, e) {
          if (e)
            root.error = e.message;
        })
      }
    }
    Txt {
      visible: root.testResult !== ""
      text: root.testResult
      dim: true
      font.pixelSize: Style.font.bodySmall
    }
    Txt {
      visible: root.original !== "" && root.draft.name !== root.original
      text: MihomoI18n.t("proxyRenameReferences") + " " + ((root.metadata.items || [])[0] || {
          dependencies: []
        }).dependencies.map(function (d) {
        return d.owner;
      }).join(", ")
      width: parent.width
      wrapMode: Text.Wrap
      dim: true
      font.pixelSize: Style.font.body
    }
  }
  ScrollView {
    id: scroll
    enabled: !root.busy
    anchors.top: head.bottom
    anchors.topMargin: Style.space(12)
    anchors.bottom: foot.top
    anchors.bottomMargin: Style.space(12)
    anchors.left: parent.left
    anchors.right: parent.right
    clip: true
    contentWidth: availableWidth
    Column {
      width: scroll.availableWidth
      spacing: Style.space(12)
      TextArea {
        visible: root.yamlMode
        width: parent.width
        height: Math.max(Style.space(350), implicitHeight)
        text: root.yamlText
        wrapMode: TextEdit.WrapAnywhere
        selectByMouse: true
        font.family: Style.font.family
        font.pixelSize: Style.font.body
        color: Commons.Color.popups.text
        background: Rectangle {
          color: Util.alpha(Commons.Color.popups.text, 0.04)
          radius: Style.cornerRadius
        }
        onTextChanged: if (activeFocus) {
          root.yamlText = text;
          root.changed();
        }
      }
      ProxyGroupForm {
        visible: root.group && !root.yamlMode
        width: parent.width
        editor: root
      }
      ProxyNodeForm {
        visible: !root.group && !root.yamlMode
        width: parent.width
        editor: root
      }
      Column {
        visible: !root.yamlMode && root.group && root.section === "advanced"
        width: parent.width
        spacing: Style.space(12)
        Txt {
          visible: !root.schema.fields.length
          text: MihomoI18n.t("proxyUnknownType")
          width: parent.width
          wrapMode: Text.Wrap
          color: Commons.Color.urgent
        }
        Repeater {
          model: root.schema.fields
          SchemaField {
            required property var modelData
            width: parent.width
            field: modelData
            visible: Models.visible(field, root.draft) && (root.fieldSearch ? field.key.indexOf(root.fieldSearch) >= 0 : Models.section(field, root.group) === root.section)
            value: root.draft[field.key]
            context: root.draft
            targets: root.metadata.targets || []
            targetGroups: root.metadata.groups || []
            targetDetails: root.targetDetails
            providers: root.metadata.providers || []
            onEdited: function (v) {
              root.setField(field.key, v);
            }
            onIssue: function (e) {
              root.issues = Models.put(root.issues, field.key, e);
            }
          }
        }
      }
    }
  }
  Rectangle {
    anchors.bottom: foot.top
    anchors.bottomMargin: Style.space(10)
    width: parent.width
    height: 1
    color: Util.alpha(Commons.Color.popups.text, 0.15)
  }
  Row {
    id: foot
    anchors.bottom: parent.bottom
    width: parent.width
    spacing: Style.space(8)
    Txt {
      width: Math.max(0, parent.width - cancelButton.width - saveButton.width - Style.space(16))
      text: MihomoI18n.t(({
          checking: "nodeChecking",
          applying: "nodeApplying",
          success: "nodeApplied",
          offline: "nodeSavedOffline",
          error: "nodeDraftKept",
          "restore-failed": "nodeRestoreFailed"
        })[root.applyState] || (root.dirty ? "proxyChanges" : "proxyNoChanges"))
      dim: true
      elide: Text.ElideRight
      font.pixelSize: Style.font.bodySmall
      anchors.verticalCenter: parent.verticalCenter
    }
    Btn {
      id: cancelButton
      text: MihomoI18n.t(root.applyState === "success" ? "nodeUndo" : root.applyState === "offline" ? "nodeStartCore" : "cancel")
      enabled: !root.busy
      onClicked: {
        if (root.applyState === "success")
          root.undo();
        else if (root.applyState === "offline")
          root.svc.request("core.start", null, function (r, e) {
            if (e)
              root.error = e.message;
            else
              root.saved(true);
          });
        else
          root.leave();
      }
    }
    Btn {
      id: saveButton
      text: MihomoI18n.t(root.applyState === "success" || root.applyState === "offline" ? "nodeDone" : "saveApply")
      primary: true
      busy: root.busy
      enabled: !root.busy && (root.yamlMode || root.catalog.supported !== false)
      onClicked: {
        if (root.applyState === "success" || root.applyState === "offline")
          root.saved(root.lastReloaded);
        else
          root.save();
      }
    }
  }
  ProxyDialog {
    id: discardDialog
    objectName: "proxy-discard-dialog"
    visible: root.discarding
    title: MihomoI18n.t("nodeDiscardTitle").arg(root.draft.name || "")
    onClosed: root.discarding = false
    onConfirmRequested: {
      root.discarding = false;
      root.leaveAfterSave = true;
      root.save();
    }
    Txt {
      width: parent.width
      text: MihomoI18n.t("nodeDiscardHint")
      wrapMode: Text.Wrap
      dim: true
    }
    ProxyDiff {
      width: parent.width
      rows: root.changes
    }
    Flow {
      width: parent.width
      spacing: Style.space(8)
      Btn {
        text: MihomoI18n.t("nodeDontSave")
        onClicked: {
          root.dirty = false;
          root.discarding = false;
          root.cancelled();
        }
      }
      Btn {
        text: MihomoI18n.t("nodeStay")
        onClicked: root.discarding = false
      }
      Btn {
        text: MihomoI18n.t("saveApply")
        primary: true
        onClicked: {
          root.discarding = false;
          root.leaveAfterSave = true;
          root.save();
        }
      }
    }
  }
  ProxyDialog {
    visible: !!root.conflict
    title: MihomoI18n.t("nodeConflict")
    onClosed: root.conflict = null
    Txt {
      width: parent.width
      text: MihomoI18n.t("nodeConflictHint")
      wrapMode: Text.Wrap
      dim: true
    }
    ProxyDiff {
      visible: !root.compareYaml
      width: parent.width
      leftTitle: MihomoI18n.t("nodeMine")
      rightTitle: MihomoI18n.t("nodeProfile")
      rows: root.conflict ? Nodes.diff(root.draft, root.conflict.items[0].definition) : []
    }
    Row {
      visible: root.compareYaml
      width: parent.width
      spacing: Style.space(8)
      Repeater {
        model: [MihomoI18n.t("nodeMine"), MihomoI18n.t("nodeProfile")]
        Column {
          required property int index
          required property string modelData
          width: (parent.width - Style.space(8)) / 2
          spacing: Style.space(6)
          Txt {
            text: parent.modelData
            dim: true
          }
          TextArea {
            width: parent.width
            height: Style.space(220)
            readOnly: true
            selectByMouse: true
            text: parent.index === 0 ? root.localYaml : root.remoteYaml
            color: Commons.Color.popups.text
            font.family: Style.font.family
            wrapMode: TextEdit.WrapAnywhere
            background: Rectangle {
              color: Util.alpha(Commons.Color.popups.text, 0.04)
            }
          }
        }
      }
    }
    Txt {
      width: parent.width
      text: MihomoI18n.t("nodeOverrideHint")
      wrapMode: Text.Wrap
      dim: true
      font.pixelSize: Style.font.bodySmall
    }
    Flow {
      width: parent.width
      spacing: Style.space(8)
      Btn {
        text: MihomoI18n.t("nodeCompareYaml")
        onClicked: root.compareYaml = !root.compareYaml
      }
      Btn {
        text: MihomoI18n.t("nodeTakeProfile")
        onClicked: root.resolveConflict(false)
      }
      Btn {
        text: MihomoI18n.t("nodeKeepMine")
        primary: true
        onClicked: root.resolveConflict(true)
      }
    }
  }
  ProxyDialog {
    visible: root.renaming
    title: MihomoI18n.t("nodeRename")
    onClosed: root.renaming = false
    Field {
      width: parent.width
      text: root.renameValue
      input.onTextEdited: root.renameValue = text
    }
    Txt {
      text: MihomoI18n.t("nodeReferences")
      dim: true
    }
    Txt {
      width: parent.width
      text: ((root.metadata.items || [])[0] || {
          dependencies: []
        }).dependencies.map(function (d) {
        return d.owner;
      }).join("\n")
      wrapMode: Text.Wrap
      font.pixelSize: Style.font.bodySmall
      dim: true
    }
    Txt {
      text: MihomoI18n.t("nodeHistory")
      bold: true
    }
    Repeater {
      model: root.history
      Row {
        required property var modelData
        width: parent.width
        spacing: Style.space(8)
        Txt {
          width: parent.width - restoreButton.width - Style.space(8)
          text: modelData.original ? MihomoI18n.t("nodeOriginal") : new Date(modelData.at * 1000).toLocaleString()
          elide: Text.ElideRight
          anchors.verticalCenter: parent.verticalCenter
          font.pixelSize: Style.font.bodySmall
        }
        Btn {
          id: restoreButton
          text: MihomoI18n.t("proxyRestore")
          onClicked: root.restoreVersion(parent.modelData)
        }
      }
    }
    Flow {
      width: parent.width
      spacing: Style.space(8)
      Btn {
        text: MihomoI18n.t("cancel")
        onClicked: root.renaming = false
      }
      Btn {
        text: MihomoI18n.t("nodeRename")
        primary: true
        enabled: root.renameValue.trim() !== ""
        onClicked: {
          root.setField("name", root.renameValue.trim());
          root.renaming = false;
        }
      }
    }
  }
}
