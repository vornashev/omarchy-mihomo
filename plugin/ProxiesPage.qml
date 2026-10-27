pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import qs.Commons
import "ProxyModels.js" as Models

Item {
  id: root
  property var svc
  property var panel
  property bool group: true
  property var runtimeGroups: []
  // Bytes per group over the last week: {name: {up, down}}.
  property var groupTraffic: ({})
  // Delay history per node over the last day, from latency.query.
  property var latency: ({})
  readonly property var choiceDetails: Models.choiceDetails(metadata.items, runtimeGroups)
  property var testing: ({})
  property int groupCount: 0
  property int nodeCount: 0
  property bool failedOnly: false
  property bool sortDelay: false
  property var quickSwitchCard: null
  property bool addingToGroup: false
  property string addGroup: ""
  property var deleteQueue: []
  readonly property var selection: (metadata.items || []).filter(function (i) {
    return !!selected[i.name];
  })
  property var metadata: ({
      items: []
    })
  property var catalog: ({
      nodes: [],
      groups: []
    })
  property string search: ""
  property string protocolFilter: ""
  property string sourceFilter: ""
  property string stage: "list"
  property var providerEntry: ({})
  property bool pendingApply: false
  property string error: ""
  property bool busy: false
  property int cursor: 0
  property int requestGeneration: 0
  property var deleting: null
  property string replacement: ""
  property var selected: ({})
  property var editorState: ({})
  property int dragFrom: -1
  property int dropAt: -1
  property real dropY: 0
  property real dragViewY: 0
  property point dragScene
  // Reordering a filtered or delay-sorted view would be ambiguous.
  readonly property bool canReorder: group && !search && !protocolFilter && !sourceFilter && !failedOnly && !sortDelay && (metadata.items || []).length > 1
  readonly property bool typing: stage !== "list" || head.searchField.input.activeFocus || !!deleting || addingToGroup || entryMenu.visible || (quickSwitchCard && quickSwitchCard.popupOpen)
  readonly property var listItems: (metadata.items || []).concat(group && runtimeGroups.some(function (g) {
    return g.name === "GLOBAL";
  }) && !(metadata.items || []).some(function (g) {
    return g.name === "GLOBAL";
  }) ? [
    {
      name: "GLOBAL",
      type: "select",
      source: "builtin",
      members: [],
      dependencies: []
    }
  ] : [])
  // QML's Array.sort is unstable: a comparator returning 0 still shuffles, so
  // the config order survives only when nothing is sorted, and ties keep it.
  readonly property var visibleItems: {
    var items = listItems.filter(function (i) {
      return (!search || (i.name + " " + (i.server || "") + " " + i.type).toLowerCase().indexOf(search.toLowerCase()) >= 0) && (!protocolFilter || i.type === protocolFilter) && (!sourceFilter || i.source === sourceFilter) && (!failedOnly || i.delay === 0);
    });
    if (!sortDelay)
      return items;
    return items.map(function (item, index) {
      return { item: item, index: index };
    }).sort(function (a, b) {
      return (a.item.delay || 999999) - (b.item.delay || 999999) || a.index - b.index;
    }).map(function (e) {
      return e.item;
    });
  }

  function reload() {
    var generation = ++requestGeneration;
    svc.request("proxies.list", null, function (r, e) {
      if (!e && generation === requestGeneration)
        runtimeGroups = r || [];
    });
    svc.request("latency.query", { hours: 24, points: 24 }, function (r, e) {
      if (!e && r && generation === requestGeneration)
        latency = r;
    });
    svc.request("traffic.totals", { by: "group" }, function (r, e) {
      if (!e && r && generation === requestGeneration)
        groupTraffic = r;
    });
    svc.request(group ? "proxy_nodes.list" : "proxy_groups.list", null, function (r, e) {
      if (!e && r && generation === requestGeneration) {
        if (group)
          nodeCount = r.items.length;
        else
          groupCount = r.items.length;
      }
    });
    busy = true;
    error = "";
    svc.request(group ? "proxy_groups.list" : "proxy_nodes.list", null, function (r, e) {
      if (generation !== requestGeneration)
        return;
      busy = false;
      if (e) {
        error = e.message;
        return;
      }
      keepScroll(function () {
        metadata = r;
      });
      if (group)
        groupCount = r.items.length;
      else
        nodeCount = r.items.length;
      selected = {};
      cursor = Math.min(cursor, Math.max(0, r.items.length - 1));
    });
  }
  // Replacing the model resets a ListView to the top; refreshes must not.
  function keepScroll(change) {
    var y = list.contentY;
    change();
    Qt.callLater(function () {
      list.contentY = y;
      list.returnToBounds();
    });
  }
  function dragTo(from, scene) {
    var p = list.mapFromItem(null, scene.x, scene.y);
    dragScene = scene;
    dragFrom = from;
    dragViewY = p.y;
    var last = (metadata.items || []).length;
    var at = list.indexAt(Style.space(10), p.y + list.contentY);
    if (at < 0)
      at = p.y + list.contentY < list.originY ? 0 : last;
    else {
      var item = list.itemAtIndex(at);
      if (item && p.y + list.contentY > item.y + item.height / 2)
        at++;
    }
    dropAt = Math.min(at, last);
    var anchor = list.itemAtIndex(Math.min(dropAt, last - 1));
    if (anchor)
      dropY = dropAt < last ? anchor.y - list.spacing / 2 : anchor.y + anchor.height + list.spacing / 2;
  }
  function dragDone() {
    var from = dragFrom, to = dropAt > dragFrom ? dropAt - 1 : dropAt;
    dragFrom = -1;
    dropAt = -1;
    if (from >= 0 && to >= 0 && to !== from && !busy)
      reorder(from, to);
  }
  function reorder(from, to) {
    var items = Models.moveMember(metadata.items, from, to);
    var p = {
      profile_id: metadata.profile_id,
      revision: metadata.revision,
      order: items.map(function (i) {
        return i.name;
      })
    };
    var m = Models.clone(metadata);
    m.items = items;
    keepScroll(function () {
      metadata = m;
    });
    cursor = to;
    busy = true;
    error = "";
    svc.request("proxy_groups.order", p, function (r, e) {
      busy = false;
      if (e)
        error = e.message;
      else
        pendingApply = !r.reloaded;
      reload();
    });
  }
  function switchTab(isGroup) {
    group = isGroup;
    failedOnly = false;
    sortDelay = false;
    search = "";
    protocolFilter = "";
    sourceFilter = "";
    reload();
  }
  function begin(state) {
    editorState = state;
    stage = "editor";
  }
  function create(protocol) {
    if (!group && !protocol) {
      stage = "catalog";
      return;
    }
    begin({
      original: "",
      draft: {
        name: "",
        type: group ? "select" : protocol || "vless"
      },
      memberOf: [],
      dirty: false
    });
  }
  function edit(item, copy) {
    if (item.source === "builtin")
      return;
    if (item.source === "provider" && !copy) {
      providerEntry = item;
      stage = "provider";
      return;
    }
    if (item.definition_available === false) {
      error = I18n.t("proxyDefinitionUnavailable");
      return;
    }
    busy = true;
    error = "";
    svc.request(group ? "proxy_groups.get" : "proxy_nodes.get", {
      profile_id: metadata.profile_id,
      name: item.name,
      provider: item.provider || ""
    }, function (r, e) {
      busy = false;
      if (e) {
        error = e.message;
        return;
      }
      metadata = r;
      var d = r.items[0].definition;
      var isCopy = copy || item.source === "provider";
      if (isCopy)
        d.name = Models.suggestedName(d.name + " (copy)", r.targets);
      begin({
        original: isCopy ? "" : item.name,
        draft: d,
        memberOf: isCopy ? [] : r.member_of || [],
        dirty: isCopy
      });
    });
  }
  function done(reloaded) {
    stage = "list";
    pendingApply = !reloaded;
    panel.notify(I18n.t(reloaded ? "configApplied" : "proxySavedOffline"));
    if (panel.proxyDraft !== undefined)
      panel.proxyDraft = null;
    reload();
  }
  function mutate(operation, name, replace) {
    busy = true;
    error = "";
    var p = {
      profile_id: metadata.profile_id,
      revision: metadata.revision,
      name: name
    };
    if (replace)
      p.replacement = replace;
    svc.request((group ? "proxy_groups." : "proxy_nodes.") + operation, p, function (r, e) {
      busy = false;
      if (e) {
        error = e.message;
        return;
      }
      deleting = null;
      if (operation === "remove" && deleteQueue.length) {
        var next = deleteQueue.shift();
        svc.request("proxy_nodes.list", null, function (fresh, e) {
          if (e) {
            error = e.message;
            deleteQueue = [];
            return;
          }
          metadata = fresh;
          deleting = next;
          replacement = "";
        });
      } else
        done(r.reloaded);
    });
  }
  function remove() {
    if (visibleItems[cursor] && visibleItems[cursor].source !== "builtin") {
      deleting = visibleItems[cursor];
      replacement = "";
    }
  }
  function move(dx, dy) {
    cursor = Math.max(0, Math.min(visibleItems.length - 1, cursor + dy));
    list.positionViewAtIndex(cursor, ListView.Contain);
  }
  function activate() {
    if (visibleItems[cursor])
      edit(visibleItems[cursor], false);
  }
  function textKey(t) {
    if (t === "/")
      head.searchField.input.forceActiveFocus();
    else if (t === "r")
      reload();
    else if (t === "n")
      create();
  }
  function canLeave() {
    if (stage === "editor" && editor.item && editor.item.busy)
      return false;
    if (stage === "editor" && editor.item && editor.item.dirty) {
      editor.item.discarding = true;
      return false;
    }
    return true;
  }
  function closeEditor() {
    stage = "list";
    if (panel.proxyDraft !== undefined)
      panel.proxyDraft = null;
    reload();
  }
  function test(item) {
    if (testing[item.name])
      return;
    var isGroup = group, generation = requestGeneration;
    testing = Models.put(testing, item.name, true);
    svc.request(group ? "group.delay" : "proxies.delay", {
      name: item.name
    }, function (r, e) {
      testing = Models.put(testing, item.name, undefined);
      if (generation !== requestGeneration)
        return;
      if (e) {
        error = e.message;
        return;
      }
      if (!isGroup) {
        var m = Models.clone(metadata);
        m.items.forEach(function (n) {
          if (n.name === item.name)
            n.delay = r.delay;
        });
        metadata = m;
      }
      svc.request("proxies.list", null, function (r, e) {
        if (!e)
          runtimeGroups = r || [];
      });
    });
  }
  function selectNode(groupName, nodeName) {
    svc.request("proxies.select", {
      group: groupName,
      name: nodeName
    }, function (r, e) {
      if (e) {
        error = e.message;
        return;
      }
      // Only the runtime selection changed; a full reload would rebuild the list.
      svc.request("proxies.list", null, function (r, e) {
        if (!e)
          runtimeGroups = r || [];
      });
    });
  }
  function groupsFor(item) {
    var names = (item.dependencies || []).map(function (d) {
      var match = /^proxy-groups\[(\d+)\]\.proxies/.exec(d.owner);
      return match ? (metadata.groups || [])[Number(match[1])] : "";
    }).filter(function (n, i, a) {
      return !!n && a.indexOf(n) === i;
    });
    return names.length ? names[0] + (names.length > 1 ? " +" + (names.length - 1) : "") : "—";
  }
  function addSelection() {
    var names = selection.map(function (i) {
      return i.name;
    });
    if (!addGroup || !names.length)
      return;
    busy = true;
    svc.request("proxy_groups.get", {
      profile_id: metadata.profile_id,
      name: addGroup
    }, function (r, e) {
      if (e) {
        busy = false;
        error = e.message;
        return;
      }
      var d = r.items[0].definition;
      d.proxies = (d.proxies || []).concat(names).filter(function (n, i, a) {
        return a.indexOf(n) === i;
      });
      svc.request("proxy_groups.save", {
        profile_id: r.profile_id,
        revision: r.revision,
        name: addGroup,
        definition: d
      }, function (r, e) {
        busy = false;
        if (e)
          error = e.message;
        else {
          addingToGroup = false;
          done(r.reloaded);
        }
      });
    });
  }
  function removeSelected() {
    deleteQueue = selection.filter(function (i) {
      return i.source !== "provider";
    });
    if (deleteQueue.length) {
      deleting = deleteQueue.shift();
      replacement = "";
    }
  }

  Component.onCompleted: {
    svc.request("proxy_catalog.get", null, function (r, e) {
      if (!e)
        catalog = r;
    });
    if (panel.proxyDraft) {
      var d = panel.proxyDraft;
      group = d.group;
      metadata = d.metadata;
      editorState = d.state;
      stage = "editor";
    } else
      reload();
  }
  Component.onDestruction: {
    if (stage === "editor" && editor.item && editor.item.dirty && panel.proxyDraft !== undefined) {
      var e = editor.item;
      panel.proxyDraft = {
        group: group,
        metadata: metadata,
        state: {
          original: e.original,
          originalDraft: e.originalDraft,
          baselineMembers: e.baselineMembers,
          draft: e.draft,
          memberOf: e.memberOf,
          yamlText: e.yamlText,
          yamlMode: e.yamlMode,
          dirty: true
        }
      };
    }
  }

  Item {
    anchors.fill: parent
    visible: root.stage === "list"
    ProxiesToolbar {
      id: head
      width: parent.width
      page: root
    }
    ListView {
      id: list
      anchors.top: head.bottom
      anchors.topMargin: Style.space(12)
      anchors.bottom: parent.bottom
      anchors.left: parent.left
      anchors.right: parent.right
      clip: true
      spacing: Style.space(7)
      model: root.visibleItems
      interactive: root.dragFrom < 0
      ScrollBar.vertical: ScrollBar {}
      Rectangle {
        parent: list.contentItem
        visible: root.dropAt >= 0
        x: 0
        y: root.dropY - height / 2
        width: list.width - Style.space(6)
        height: 2
        radius: 1
        color: Color.accent
      }
      Timer {
        // Scroll while a dragged card hovers near either edge.
        interval: 16
        repeat: true
        running: root.dragFrom >= 0 && (root.dragViewY < Style.space(40) || root.dragViewY > list.height - Style.space(40))
        onTriggered: {
          var step = root.dragViewY < Style.space(40) ? -Style.space(12) : Style.space(12);
          list.contentY = Math.max(list.originY, Math.min(list.originY + list.contentHeight - list.height, list.contentY + step));
          root.dragTo(root.dragFrom, root.dragScene);
        }
      }
      delegate: Loader {
        id: entry
        required property var modelData
        required property int index
        width: list.width - Style.space(6)
        sourceComponent: root.group ? groupCard : nodeRow
        Component {
          id: groupCard
          ProxyGroupCard {
            id: card
            onPopupOpenChanged: if (popupOpen) root.quickSwitchCard = card
            width: entry.width
            entry: entry.modelData
            runtime: root.runtimeGroups.filter(function (g) {
              return g.name === entry.modelData.name;
            })[0] || ({})
            svc: root.svc
            traffic: root.groupTraffic[entry.modelData.name] || null
            latency: root.latency
            testing: !!root.testing[entry.modelData.name]
            draggable: root.canReorder && entry.modelData.source !== "builtin"
            opacity: root.dragFrom === entry.index ? 0.5 : 1
            onDragMoved: function (scene) {
              root.dragTo(entry.index, scene);
            }
            onDragEnded: root.dragDone()
            onEditRequested: root.edit(entry.modelData, false)
            onTestRequested: root.test(entry.modelData)
            onMenuRequested: function(anchor) {
              entryMenu.entry = entry.modelData;
              entryMenu.popup(anchor, Qt.point(anchor.width - entryMenu.width, anchor.height + Style.space(4)));
            }
            onSelected: function (n) {
              root.selectNode(entry.modelData.name, n);
            }
          }
        }
        Component {
          id: nodeRow
          ProxyNodeRow {
            width: entry.width
            page: root
            item: entry.modelData
            index: entry.index
            onMenuRequested: {
              entryMenu.entry = entry.modelData;
              entryMenu.popup();
            }
          }
        }
      }
      Column {
        visible: !root.busy && root.visibleItems.length === 0
        anchors.centerIn: parent
        width: parent.width - Style.space(30)
        spacing: Style.space(12)
        Txt {
          width: parent.width
          text: !root.metadata.profile_id ? I18n.t("proxyNoProfile") : root.search || root.protocolFilter || root.sourceFilter || root.failedOnly ? I18n.t("proxyNoResults") : I18n.t("proxyEmpty")
          bold: true
          horizontalAlignment: Text.AlignHCenter
        }
        Txt {
          width: parent.width
          text: I18n.t(!root.metadata.profile_id ? "nodeNoProfileHint" : "nodeEmptyHint")
          wrapMode: Text.Wrap
          dim: true
          horizontalAlignment: Text.AlignHCenter
        }
        Row {
          anchors.horizontalCenter: parent.horizontalCenter
          spacing: Style.space(8)
          Btn {
            visible: !root.metadata.profile_id
            text: I18n.t("navProfiles")
            onClicked: root.panel.showPage("profiles")
          }
          Btn {
            visible: !!root.metadata.profile_id
            text: I18n.t("proxyImport")
            onClicked: root.stage = "import"
          }
          Btn {
            visible: !!root.metadata.profile_id
            text: I18n.t("nodeClearFilters")
            onClicked: {
              root.search = "";
              root.protocolFilter = "";
              root.sourceFilter = "";
              root.failedOnly = false;
            }
          }
        }
      }
    }
  }

  Loader {
    id: editor
    active: root.stage === "editor"
    anchors.fill: parent
    source: "ProxyEditor.qml"
    onLoaded: {
      item.svc = root.svc;
      item.panel = root.panel;
      item.metadata = root.metadata;
      item.targetDetails = Qt.binding(function() { return root.choiceDetails; });
      item.catalog = Qt.binding(function () {
        return root.catalog;
      });
      item.group = root.group;
      for (var k in root.editorState)
        item[k] = root.editorState[k];
      if (root.editorState.originalDraft === undefined)
        item.captureBaseline();
      item.saved.connect(root.done);
      item.cancelled.connect(root.closeEditor);
    }
  }
  Loader {
    active: root.stage === "import"
    anchors.fill: parent
    source: "ProxyImport.qml"
    onLoaded: {
      item.svc = root.svc;
      item.metadata = root.metadata;
      item.saved.connect(root.done);
      item.cancelled.connect(function () {
        root.stage = "list";
      });
    }
  }
  ProxyDialog {
    visible: !!root.deleting
    busy: root.busy
    title: I18n.t("nodeDelete").arg(root.deleting ? root.deleting.name : "")
    onClosed: {
      if (root.deleting) {
        root.deleting = null;
        root.deleteQueue = [];
      }
    }
    Txt {
      width: parent.width
      text: I18n.t("nodeDeleteHint").arg(root.deleting ? (root.deleting.dependencies || []).length : 0)
      wrapMode: Text.Wrap
      dim: true
    }
    Repeater {
      model: root.deleting ? root.deleting.dependencies || [] : []
      Rectangle {
        required property var modelData
        width: parent.width
        height: Style.space(36)
        radius: Style.cornerRadius
        color: Util.alpha(Color.popups.text, 0.04)
        Txt {
          anchors.fill: parent
          anchors.margins: Style.space(8)
          text: parent.modelData.owner + " · " + parent.modelData.field
          elide: Text.ElideRight
          font.pixelSize: Style.font.caption
        }
      }
    }
    Txt {
      text: I18n.t("nodeReplaceAll")
    }
    ProxyChoice {
      width: parent.width
      searchable: true
      groupNames: root.metadata.groups || []
      optionDetails: root.choiceDetails
      model: [I18n.t("proxyNoReplacement")].concat((root.metadata.targets || []).filter(function (n) {
        return !root.deleting || n !== root.deleting.name;
      }))
      currentIndex: Math.max(0, model.indexOf(root.replacement))
      onActivated: root.replacement = currentIndex ? currentText : ""
    }
    ProxyCheck {
      text: I18n.t("nodeRemoveOnly")
      checked: !root.replacement
      enabled: !!root.deleting && !(root.deleting.dependencies || []).some(function (d) {
        return d.field !== "proxies";
      })
      onToggled: root.replacement = ""
    }
    Txt {
      visible: !root.replacement && !!root.deleting && (root.deleting.dependencies || []).some(function (d) {
        return d.field !== "proxies";
      })
      width: parent.width
      text: I18n.t("nodeRemoveBlocked")
      wrapMode: Text.Wrap
      color: Color.urgent
      font.pixelSize: Style.font.caption
    }
    Txt {
      visible: root.error !== ""
      width: parent.width
      text: root.error
      wrapMode: Text.Wrap
      color: Color.urgent
    }
    Row {
      spacing: Style.space(8)
      Btn {
        text: I18n.t("cancel")
        enabled: !root.busy
        onClicked: {
          root.deleting = null;
          root.deleteQueue = [];
        }
      }
      Btn {
        text: I18n.t(root.replacement ? "nodeDeleteReplace" : "delete")
        primary: true
        busy: root.busy
        enabled: !root.busy && !!root.deleting && (!!root.replacement || !(root.deleting.dependencies || []).some(function (d) {
            return d.field !== "proxies";
          }))
        onClicked: root.mutate("remove", root.deleting.name, root.replacement)
      }
    }
  }
  Loader {
    active: root.stage === "catalog"
    anchors.fill: parent
    source: "ProxyCatalog.qml"
    onLoaded: {
      item.catalog = root.catalog;
      item.picked.connect(function (p) {
        root.create(p);
      });
      item.cancelled.connect(function () {
        root.stage = "list";
      });
      item.importRequested.connect(function () {
        root.stage = "import";
      });
    }
  }
  Loader {
    active: root.stage === "provider"
    anchors.fill: parent
    source: "ProxyProvider.qml"
    onLoaded: {
      item.svc = root.svc;
      item.entry = root.providerEntry;
      item.metadata = root.metadata;
      item.cancelled.connect(function () {
        root.stage = "list";
      });
      item.copyRequested.connect(function () {
        root.edit(root.providerEntry, true);
      });
      item.importRequested.connect(function () {
        root.stage = "import";
      });
      item.subscriptionRequested.connect(function (provider) {
        root.panel.showPage("profiles");
        root.panel.notify(provider);
      });
    }
  }
  ActionMenu {
    id: entryMenu
    objectName: "proxyEntryMenu"
    property var entry: ({})
    ActionMenuItem {
      id: editAction
      glyph: Icons.edit
      keyHint: "Enter"
      text: I18n.t("proxyEdit")
      onTriggered: root.edit(entryMenu.entry, false)
    }
    ActionMenuItem {
      id: testAction
      glyph: Icons.test
      keyHint: "T"
      text: I18n.t("proxyCheckNow")
      onTriggered: root.test(entryMenu.entry)
    }
    ActionMenuItem {
      id: duplicateAction
      glyph: Icons.copy
      keyHint: "Ctrl+D"
      text: I18n.t("proxyDuplicate")
      enabled: entryMenu.entry.definition_available !== false
      onTriggered: root.edit(entryMenu.entry, true)
    }
    ActionMenuSeparator {}
    ActionMenuItem {
      glyph: Icons.undo
      text: I18n.t("proxyResetProfile")
      hint: enabled ? "" : I18n.t("proxyNoLocalChanges")
      enabled: !!entryMenu.entry.resettable
      onTriggered: root.mutate("reset", entryMenu.entry.name, "")
    }
    ActionMenuSeparator {}
    ActionMenuItem {
      id: deleteAction
      glyph: Icons.remove
      dangerous: true
      keyHint: "Del"
      text: I18n.t("delete") + "…"
      enabled: entryMenu.entry.source !== "provider"
      onTriggered: {
        root.deleteQueue = [];
        root.deleting = entryMenu.entry;
        root.replacement = "";
      }
    }
    Shortcut {
      sequence: "T"
      enabled: entryMenu.visible && testAction.enabled
      onActivated: { entryMenu.close(); testAction.triggered(); }
    }
    Shortcut {
      sequence: "Ctrl+D"
      enabled: entryMenu.visible && duplicateAction.enabled
      onActivated: { entryMenu.close(); duplicateAction.triggered(); }
    }
    Shortcut {
      sequence: "Del"
      enabled: entryMenu.visible && deleteAction.enabled
      onActivated: { entryMenu.close(); deleteAction.triggered(); }
    }
  }
  Rectangle {
    visible: root.addingToGroup
    anchors.fill: parent
    z: 20
    color: Color.popups.background
    Column {
      anchors.centerIn: parent
      width: parent.width - Style.space(24)
      spacing: Style.space(12)
      Txt {
        text: I18n.t("proxyAddToGroup")
        bold: true
      }
      ProxyChoice {
        width: parent.width
        model: root.metadata.groups || []
        onActivated: root.addGroup = currentText
      }
      Txt {
        width: parent.width
        text: root.error
        visible: text !== ""
        color: Color.urgent
        wrapMode: Text.Wrap
      }
      Row {
        spacing: Style.space(8)
        Btn {
          text: I18n.t("cancel")
          enabled: !root.busy
          onClicked: root.addingToGroup = false
        }
        Btn {
          text: I18n.t("saveApply")
          primary: true
          enabled: !root.busy && !!root.addGroup
          busy: root.busy
          onClicked: root.addSelection()
        }
      }
    }
  }
}
