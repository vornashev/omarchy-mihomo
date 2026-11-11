import QtQuick
import QtTest
import "../../plugin" as Plugin
import "../../plugin/ProxyModels.js" as Models
import "../../plugin/ProxyNodeModels.js" as Nodes

Rectangle {
  id: scene
  width: 632
  height: 680
  color: "#1e1e2e"
  QtObject {
    id: mockPanel
    property var proxyDraft: null
    property string toast: ""
    property string page: "proxies"
    function notify(s) {
      toast = s;
    }
    function showPage(s) {
      page = s;
    }
    function forceKeys() {
    }
  }
  QtObject {
    id: mockService
    property var calls: []
    property bool failSave: false
    property bool failPreview: false
    property bool offline: false
    property bool coreRunning: true
    property string profileName: "Test"
    property color red: "#f38ba8"
    property color yellow: "#f9e2af"
    property color green: "#a6e3a1"
    property color mauve: "#cba6f7"
    property var catalog: ({
        nodes: [
          {
            type: "vless",
            fields: [
              {
                key: "name",
                kind: "string",
                required: true
              },
              {
                key: "port",
                kind: "number"
              },
              {
                key: "tls",
                kind: "boolean"
              },
              {
                key: "network",
                kind: "string",
                choices: ["tcp", "ws"]
              },
              {
                key: "ws-opts",
                kind: "object",
                when: {
                  network: "ws"
                },
                fields: [
                  {
                    key: "path",
                    kind: "string"
                  }
                ]
              },
              {
                key: "reality-opts",
                kind: "object",
                fields: [
                  {
                    key: "public-key",
                    kind: "string"
                  },
                  {
                    key: "short-id",
                    kind: "string"
                  }
                ]
              }
            ]
          }
        ],
        groups: [
          {
            type: "select",
            fields: [
              {
                key: "name",
                kind: "string"
              },
              {
                key: "proxies",
                kind: "list",
                item: {
                  kind: "string"
                }
              }
            ]
          }
        ]
      })
    function meta() {
      return {
        profile_id: "p",
        profile_name: "Test",
        revision: "r1",
        targets: ["DIRECT", "FI", "G"],
        providers: ["subscription"],
        groups: ["G"],
        items: [
          {
            name: "FI",
            type: "vless",
            source: "profile",
            server: "example.net",
            port: 443,
            dependencies: [],
            definition: {
              name: "FI",
              type: "vless",
              port: 443,
              network: "tcp",
              future: {
                keep: true
              }
            }
          }
        ]
      };
    }
    function request(method, params, callback) {
      calls.push({
        method: method,
        params: params
      });
      if (method === "proxy_catalog.get")
        callback(catalog, null);
      else if (method === "proxy_config.convert")
        callback({
          definition: params.definition || {
            name: "FI",
            type: "vless",
            future: {
              keep: true
            }
          },
          yaml: "name: FI\ntype: vless\nfuture: {keep: true}"
        }, null);
      else if (method === "proxy_import.preview")
        callback([
          {
            index: 1,
            definition: {
              name: "FI",
              type: "vless"
            },
            warnings: []
          },
          {
            index: 2,
            error: "Invalid link",
            warnings: []
          },
          {
            index: 3,
            definition: {
              name: "NL",
              type: "vless"
            },
            warnings: ["Unsupported parameter"]
          }
        ], null);
      else if (method.indexOf(".history") >= 0)
        callback({
          items: [
            {
              at: 1700000000,
              definition: {
                name: "FI",
                type: "vless",
                port: 443
              },
              member_of: ["G"]
            },
            {
              original: true,
              definition: {
                name: "FI",
                type: "vless",
                port: 80
              }
            }
          ]
        }, null);
      else if (method.indexOf(".save") >= 0)
        callback(failSave ? null : {
          reloaded: !offline,
          revision: "r2"
        }, failSave ? {
          message: "Profile changed",
          code: "conflict"
        } : null);
      else if (method === "proxy_config.preview")
        callback(failPreview ? null : {
          valid: true
        }, failPreview ? {
          code: "config",
          message: "Invalid node"
        } : null);
      else if (method === "proxies.delay")
        callback({
          delay: 48
        }, null);
      else if (method === "proxies.list")
        callback([], null);
      else
        callback(meta(), null);
    }
  }
  Component {
    id: pageComponent
    Plugin.ProxiesPage {
      anchors.fill: parent
      anchors.margins: 18
      svc: mockService
      panel: mockPanel
    }
  }
  Component {
    id: editorComponent
    Plugin.ProxyEditor {
      anchors.fill: parent
      anchors.margins: 18
      svc: mockService
      catalog: mockService.catalog
      metadata: mockService.meta()
      draft: ({
          name: "FI",
          type: "vless",
          port: 443,
          network: "tcp",
          future: {
            keep: true
          }
        })
    }
  }
  Component {
    id: importComponent
    Plugin.ProxyImport {
      anchors.fill: parent
      anchors.margins: 18
      svc: mockService
      metadata: mockService.meta()
    }
  }
  Component {
    id: fieldComponent
    Plugin.SchemaField {
      width: 500
    }
  }
  Component {
    id: catalogComponent
    Plugin.ProxyCatalog {
      anchors.fill: parent
      anchors.margins: 18
      catalog: mockService.catalog
    }
  }
  Component {
    id: providerComponent
    Plugin.ProxyProvider {
      anchors.fill: parent
      anchors.margins: 18
      svc: mockService
      metadata: mockService.meta()
      entry: ({
          name: "Tokyo",
          type: "hysteria2",
          server: "jp.example.net",
          port: 443,
          provider: "asia",
          source: "provider",
          delay: 210
        })
    }
  }
  TestCase {
    name: "ProxyEditor"
    when: windowShown
    function init() {
      mockService.calls = [];
      mockService.failSave = false;
      mockService.failPreview = false;
      mockService.offline = false;
      mockPanel.proxyDraft = null;
    }
    function test_group_action_menu_shortcuts() {
      var p = createTemporaryObject(pageComponent, scene);
      wait(30);
      var button = findChild(p, "proxyGroupMenuButton");
      verify(button);
      mouseClick(button, button.width / 2, button.height / 2);
      var menu = findChild(p, "proxyEntryMenu");
      tryCompare(menu, "opened", true);
      verify(p.typing);
      keyClick(Qt.Key_D, Qt.ControlModifier);
      tryCompare(menu, "visible", false);
      compare(p.stage, "editor");
      compare(p.editorState.original, "");
      verify(p.editorState.draft.name.indexOf("copy") >= 0);
    }
    function groupsMeta() {
      var m = mockService.meta();
      m.groups = ["A", "B", "C"];
      m.items = m.groups.map(function (n) {
        return {
          name: n,
          type: "select",
          source: "profile",
          members: ["DIRECT"],
          dependencies: []
        };
      });
      return m;
    }
    function test_drag_reorders_groups() {
      var p = createTemporaryObject(pageComponent, scene);
      wait(30);
      p.metadata = groupsMeta();
      wait(30);
      verify(p.canReorder);
      var handle = findChild(p, "proxyGroupDragHandle");
      verify(handle);
      var list = handle;
      while (list && !(list.contentItem && list.indexAt))
        list = list.parent;
      var last = list.itemAtIndex(2);
      var target = list.contentItem.mapToItem(handle, 10, last.y + last.height - 4);
      mousePress(handle, handle.width / 2, handle.height / 2);
      mouseMove(handle, handle.width / 2, target.y / 2);
      mouseMove(handle, handle.width / 2, target.y);
      compare(p.dropAt, 3);
      mouseRelease(handle, handle.width / 2, target.y);
      var call = mockService.calls.filter(function (c) {
        return c.method === "proxy_groups.order";
      })[0];
      verify(call);
      compare(call.params.order, ["B", "C", "A"]);
      compare(call.params.revision, "r1");
      p.search = "B";
      verify(!p.canReorder);
    }
    function test_groups_keep_config_order() {
      var p = createTemporaryObject(pageComponent, scene);
      wait(30);
      var m = groupsMeta();
      m.groups = ["AI", "YOUTUBE", "PROXY", "WARP", "TELEGRAM", "TikTok", "Стриминг"];
      m.items = m.groups.map(function (n, i) {
        return { name: n, type: "select", source: "profile", delay: 50, members: [], dependencies: [] };
      });
      p.metadata = m;
      compare(p.visibleItems.map(function (i) { return i.name; }), m.groups);
      p.sortDelay = true;
      compare(p.visibleItems.map(function (i) { return i.name; }), m.groups);
    }
    function test_selecting_node_keeps_list() {
      var p = createTemporaryObject(pageComponent, scene);
      wait(30);
      mockService.calls = [];
      p.selectNode("G", "FI");
      compare(mockService.calls.map(function (c) {
        return c.method;
      }), ["proxies.select", "proxies.list"]);
    }
    function test_lists_editor_and_filter() {
      var p = createTemporaryObject(pageComponent, scene);
      verify(p);
      wait(50);
      compare(p.visibleItems.length, 1);
      p.search = "missing";
      compare(p.visibleItems.length, 0);
      p.search = "";
      p.group = false;
      p.edit(p.visibleItems[0], false);
      compare(p.stage, "editor");
      wait(50);
    }
    function test_node_rows_open_the_editor_and_the_menu() {
      var p = createTemporaryObject(pageComponent, scene);
      p.switchTab(false);
      tryVerify(function () { return p.visibleItems.length > 0; }, 2000);
      waitForRendering(p);
      var row = findChild(p, "proxyNodeRow");
      verify(row, "a node row");
      compare(row.item.name, p.visibleItems[0].name);
      mouseClick(row, row.width / 2, row.height / 2, Qt.RightButton);
      verify(findChild(p, "proxyEntryMenu").visible, "right click opens the menu");
      findChild(p, "proxyEntryMenu").close();
      mouseClick(row, row.width / 2, row.height / 2);
      compare(p.stage, "editor");
    }
    function test_unknown_fields_and_conflict_keep_input() {
      var e = createTemporaryObject(editorComponent, scene);
      verify(e);
      e.setField("port", 8443);
      compare(e.draft.future.keep, true);
      mockService.failSave = true;
      e.save();
      compare(e.error, "Profile changed");
      compare(e.draft.port, 8443);
      verify(e.dirty);
      verify(!e.leave());
      verify(e.discarding);
    }
    function test_yaml_roundtrip() {
      var e = createTemporaryObject(editorComponent, scene);
      e.switchMode();
      verify(e.yamlMode);
      verify(e.yamlText.indexOf("future") >= 0);
      e.switchMode();
      verify(!e.yamlMode);
      compare(e.draft.future.keep, true);
    }
    function test_import_errors_names_and_warnings() {
      var p = createTemporaryObject(importComponent, scene);
      verify(p);
      p.inputText = "links";
      p.parse();
      compare(p.step, 1);
      compare(p.rows[0].definition.name, "FI (2)");
      verify(p.rows[0].selected);
      verify(!p.rows[1].selected);
      verify(!p.rows[2].selected);
      p.changeRow(0, "action", "replace");
      p.memberOf = ["G"];
      p.commit();
      var call = mockService.calls[mockService.calls.length - 1];
      compare(call.method, "proxy_import.commit");
      compare(call.params.items[0].name, "FI");
      compare(call.params.member_of, ["G"]);
    }
    function test_nested_fields_lists_and_maps() {
      var f = createTemporaryObject(fieldComponent, scene, {
        field: {
          key: "peers",
          kind: "list",
          item: {
            kind: "object",
            fields: [
              {
                key: "server",
                kind: "string"
              },
              {
                key: "port",
                kind: "number"
              }
            ]
          }
        },
        value: [
          {
            server: "example.net",
            port: 443
          }
        ],
        expanded: true
      });
      verify(f);
      wait(60);
      var m = createTemporaryObject(fieldComponent, scene, {
        field: {
          key: "headers",
          kind: "map",
          item: {
            kind: "string"
          }
        },
        value: {
          Host: "example.net"
        },
        expanded: true
      });
      verify(m);
      wait(40);
      compare(m.mapKeys, ["Host"]);
    }
    function test_conditional_fields() {
      verify(!Models.visible({
        when: {
          network: "ws"
        }
      }, {
        network: "tcp"
      }));
      verify(Models.visible({
        when: {
          network: "ws"
        }
      }, {
        network: "ws"
      }));
      compare(Models.put({
        a: 1,
        future: 2
      }, "a", undefined), {
        future: 2
      });
    }
    function test_nested_errors_survive_sibling_edits_and_collapse() {
      var f = createTemporaryObject(fieldComponent, scene, {
        field: {
          key: "peer",
          kind: "object",
          fields: [
            {
              key: "port",
              kind: "number"
            },
            {
              key: "server",
              kind: "string"
            }
          ]
        },
        value: {
          port: 443,
          server: "example.net"
        },
        expanded: true
      });
      f.edited.connect(function (next) {
        f.value = next;
      });
      wait(30);
      var port = findChild(f, "proxy-input-port");
      port.forceActiveFocus();
      keyClick(Qt.Key_X);
      verify(f.errorText !== "");
      var server = findChild(f, "proxy-input-server");
      server.forceActiveFocus();
      keyClick(Qt.Key_A);
      verify(f.errorText !== "", "A sibling edit must not clear an invalid port");
      f.expanded = false;
      f.expanded = true;
      verify(f.errorText !== "");
      port.forceActiveFocus();
      keyClick(Qt.Key_A, Qt.ControlModifier);
      keyClick(Qt.Key_8);
      compare(f.value.port, 8);
      compare(f.errorText, "");
    }
    function test_typing_keeps_focus_and_validates_numbers() {
      var e = createTemporaryObject(editorComponent, scene);
      wait(30);
      var name = findChild(e, "proxy-input-name");
      verify(name);
      name.forceActiveFocus();
      keyClick(Qt.Key_A, Qt.ControlModifier);
      keyClick(Qt.Key_N);
      keyClick(Qt.Key_L);
      compare(e.draft.name, "nl");
      verify(name.activeFocus);
      var port = findChild(e, "proxy-input-port");
      port.forceActiveFocus();
      keyClick(Qt.Key_A, Qt.ControlModifier);
      keyClick(Qt.Key_8);
      keyClick(Qt.Key_4);
      keyClick(Qt.Key_4);
      keyClick(Qt.Key_3);
      compare(e.draft.port, 8443);
      keyClick(Qt.Key_X);
      verify(e.issues.port !== "");
      e.save();
      compare(e.error, Plugin.MihomoI18n.t("proxyFixFields"));
    }
    function test_group_modes_and_sections() {
      var req = new XMLHttpRequest();
      req.open("GET", Qt.resolvedUrl("../../crates/omm-config/src/proxy_catalog.json"), false);
      req.send();
      var catalog = JSON.parse(req.responseText);
      var modes = ["select", "url-test", "fallback", "load-balance"];
      for (var i = 0; i < modes.length; i++) {
        var e = createTemporaryObject(editorComponent, scene, {
          group: true,
          catalog: catalog,
          original: "PROXY",
          draft: {
            name: "PROXY",
            type: modes[i],
            proxies: ["Frankfurt 02", "Amsterdam", "DIRECT"],
            future: {
              keep: true
            }
          },
          metadata: {
            profile_id: "p",
            revision: "r1",
            profile_name: "Config.yml",
            groups: ["PROXY", "Auto · Europe"],
            targets: ["Frankfurt 02", "Amsterdam", "DIRECT"],
            providers: ["provider-eu"]
          }
        });
        wait(50);
        verify(e);
        grabImage(scene).save("/tmp/proxy-v2-" + modes[i] + ".png");
        e.section = "members";
        wait(30);
        grabImage(scene).save("/tmp/proxy-v2-members.png");
        e.setField("include-all-proxies", true);
        e.setField("filter", "(?i)FI|NL|DE");
        wait(30);
        grabImage(scene).save("/tmp/proxy-v2-auto.png");
        e.section = "health";
        wait(30);
        grabImage(scene).save("/tmp/proxy-v2-health.png");
        e.section = "advanced";
        wait(20);
        compare(e.draft.future.keep, true);
        e.destroy();
      }
    }
    function test_group_interaction_save_and_health() {
      var e = createTemporaryObject(editorComponent, scene, {
        group: true,
        original: "G",
        draft: {
          name: "G",
          type: "select",
          proxies: ["FI", "DIRECT"],
          future: {
            keep: true
          }
        }
      });
      wait(30);
      var form = findChild(e, "proxy-group-form");
      verify(form);
      var mode = findChild(e, "proxy-mode-fallback");
      mouseClick(mode, mode.width / 2, mode.height / 2);
      compare(e.draft.type, "fallback");
      compare(e.draft.future.keep, true);
      form.reorder(0, 1);
      compare(e.draft.proxies, ["DIRECT", "FI"]);
      form.check();
      compare(form.testing, false);
      compare(form.delays.FI, 48);
      form.setAutomatic(true);
      verify(e.draft["include-all-proxies"]);
      form.setAutomatic(false);
      verify(e.draft["include-all-proxies"] === undefined);
      e.save();
      var call = mockService.calls[mockService.calls.length - 1];
      compare(call.method, "proxy_groups.save");
      compare(call.params.definition.proxies, ["DIRECT", "FI"]);
      compare(call.params.definition.future.keep, true);
    }
    function test_order_and_automatic_membership() {
      compare(Models.moveMember(["FI", "NL", "DIRECT"], 0, 2), ["NL", "DIRECT", "FI"]);
      compare(Models.moveMember(["FI"], 0, -1), ["FI"]);
      compare(Models.toggleMember(["FI", "NL"], "FI", false), ["NL"]);
      var nodes = [
        {
          name: "DE Frankfurt",
          type: "vless",
          source: "profile"
        },
        {
          name: "NL Test",
          type: "vless",
          source: "profile"
        },
        {
          name: "FI Free",
          type: "ss",
          source: "provider",
          provider: "eu"
        },
        {
          name: "US",
          type: "vless",
          source: "profile"
        }
      ];
      var result = Models.filterNodes(nodes, {
        "include-all": true,
        filter: "(?i)DE|NL|FI",
        "exclude-filter": "Test",
        "exclude-type": "ss"
      });
      compare(result.error, "");
      compare(result.items.filter(function (n) {
        return !n.reason;
      }).map(function (n) {
        return n.name;
      }), ["DE Frankfurt"]);
      verify(Models.filterNodes(nodes, {
        filter: "["
      }).error !== "");
      compare(Models.filterNodes(nodes, {
        use: ["eu"]
      }).items.length, 1);
    }
    function test_list_and_import_renders() {
      var p = createTemporaryObject(pageComponent, scene);
      wait(40);
      grabImage(scene).save("/tmp/proxy-v2-groups.png");
      p.switchTab(false);
      wait(40);
      grabImage(scene).save("/tmp/proxy-v2-nodes.png");
      p.destroy();
      var imp = createTemporaryObject(importComponent, scene);
      imp.inputText = "vless://example\ntrojan://example\n[Interface]\nPrivateKey = …";
      imp.parse(false);
      compare(imp.step, 0);
      compare(imp.validRows.length, 2);
      wait(40);
      grabImage(scene).save("/tmp/proxy-v2-import.png");
      imp.destroy();
    }
    function fullCatalog() {
      var req = new XMLHttpRequest();
      req.open("GET", Qt.resolvedUrl("../../crates/omm-config/src/proxy_catalog.json"), false);
      req.send();
      return JSON.parse(req.responseText);
    }
    function test_modal_conflict_discard_history_and_apply() {
      var e = createTemporaryObject(editorComponent, scene, {
        original: "FI"
      });
      wait(20);
      e.setField("port", 8443);
      verify(!e.leave());
      wait(30);
      verify(findChild(e,"proxy-discard-dialog").height > 100);
      grabImage(scene.parent).save("/tmp/node-modal-discard.png");
      verify(e.changes.some(function (d) {
        return d.key === "port";
      }));
      e.discarding = false;
      mockService.failSave = true;
      e.save();
      wait(30);
      verify(!!e.conflict);
      compare(e.draft.port, 8443);
      grabImage(scene.parent).save("/tmp/node-modal-conflict.png");
      mockService.failSave = false;
      e.resolveConflict(true);
      compare(e.applyState, "success");
      compare(e.metadata.revision, "r2");
      e.loadHistory();
      wait(30);
      compare(e.history.length, 2);
      grabImage(scene.parent).save("/tmp/node-modal-history.png");
      e.restoreVersion(e.history[0]);
      compare(e.draft.port, 443);
      verify(e.dirty);
      mockService.offline = true;
      e.save();
      compare(e.applyState, "offline");
      e.setField("port", 8443);
      mockService.failPreview = true;
      e.save();
      compare(e.applyState, "error");
      verify(e.dirty);
      compare(e.draft.port, 8443);
    }
    function test_all_node_designs_render() {
      var catalog = fullCatalog();
      var names = ["vless", "ss", "ssr", "socks5", "http", "vmess", "snell", "trojan", "hysteria", "hysteria2", "wireguard", "tuic", "shadowquic", "gost-relay", "direct", "dns", "reject", "rematch", "ssh", "mieru", "anytls", "sudoku", "masque", "trusttunnel", "openvpn", "tailscale", "zerotier", "easytier"];
      for (var i = 0; i < names.length; i++) {
        var type = names[i];
        var d = {
          name: "Frankfurt · " + type,
          type: type,
          server: "de.example.net",
          port: 443,
          uuid: "3f1c9a2e-8a6d-4abc-9876-8a6d0e4b7b4d",
          password: "secret",
          future: {
            keep: true
          }
        };
        if (type === "easytier")
          d.peers = ["tcp://example.net:11010"];
        var e = createTemporaryObject(editorComponent, scene, {
          catalog: catalog,
          draft: d,
          original: d.name
        });
        wait(15);
        verify(e);
        compare(e.schema.type, type);
        for (var k = 0; k < Nodes.basicKeys(type).length; k++) {
          var key = Nodes.basicKeys(type)[k];
          if (Nodes.field(e.schema, key))
            verify(findChild(e, "proxy-field-" + key), type + " field " + key);
        }
        grabImage(scene).save("/tmp/node-design-" + type + ".png");
        e.destroy();
      }
      var v = createTemporaryObject(editorComponent, scene, {
        catalog: catalog,
        original: "Frankfurt",
        draft: {
          name: "Frankfurt",
          type: "vless",
          server: "de.example.net",
          port: 443,
          tls: true,
          servername: "www.microsoft.com",
          "reality-opts": {
            "public-key": "public-key",
            "short-id": "0123abcd"
          },
          "client-fingerprint": "chrome",
          alpn: ["h2", "http/1.1"],
          network: "ws",
          "ws-opts": {
            path: "/ray",
            headers: {
              Host: "cdn.example.net"
            },
            "max-early-data": 2048
          },
          "dialer-proxy": "WARP",
          smux: {
            enabled: true,
            protocol: "h2mux",
            "max-connections": 4
          }
        }
      });
      for (var section of ["security", "transport", "advanced"]) {
        v.section = section;
        wait(25);
        grabImage(scene).save("/tmp/node-design-" + section + ".png");
      }
      v.destroy();
      var w = createTemporaryObject(editorComponent, scene, {
        catalog: catalog,
        original: "WARP",
        draft: {
          name: "WARP",
          type: "wireguard",
          peers: [
            {
              server: "engage.cloudflareclient.com",
              port: 2408,
              "public-key": "key",
              "allowed-ips": ["0.0.0.0/0", "::/0"],
              reserved: [0, 0, 0]
            },
            {
              server: "home.example.net",
              port: 51820
            }
          ],
          "amnezia-wg-option": {
            jc: 4,
            jmin: 40,
            jmax: 70,
            s1: 0,
            s2: 0,
            h1: "1",
            h2: "2",
            h3: "3",
            h4: "4"
          }
        }
      });
      for (var section of ["peers", "amnezia"]) {
        w.section = section;
        wait(25);
        grabImage(scene).save("/tmp/node-design-" + section + ".png");
      }
      w.destroy();
    }
    function test_catalog_provider_delete_import_states() {
      var c = createTemporaryObject(catalogComponent, scene, {
        catalog: fullCatalog()
      });
      wait(40);
      grabImage(scene).save("/tmp/node-modal-catalog.png");
      c.search = "wireguard";
      wait(10);
      c.destroy();
      var provider = createTemporaryObject(providerComponent, scene);
      wait(30);
      grabImage(scene).save("/tmp/node-modal-provider.png");
      provider.destroy();
      var page = createTemporaryObject(pageComponent, scene);
      wait(20);
      page.group = false;
      page.deleting = {
        name: "FI",
        source: "profile",
        dependencies: [
          {
            owner: "proxy-groups[0].proxies[0]",
            field: "proxies"
          },
          {
            owner: "rules[0]",
            field: "rules"
          }
        ]
      };
      wait(30);
      grabImage(scene.parent).save("/tmp/node-modal-delete.png");
      page.deleting = null;
      page.destroy();
      var imp = createTemporaryObject(importComponent, scene);
      imp.inputText = "links";
      imp.parse();
      wait(20);
      grabImage(scene).save("/tmp/node-modal-preview.png");
      imp.step = 2;
      imp.createGroup = true;
      imp.newGroup = "Imported";
      wait(20);
      grabImage(scene).save("/tmp/node-modal-import-groups.png");
      imp.commit();
      var call = mockService.calls[mockService.calls.length - 1];
      compare(call.params.new_group, "Imported");
      imp.destroy();
    }
    function test_visual() {
      var e = createTemporaryObject(editorComponent, scene);
      wait(80);
      var img = grabImage(scene);
      verify(img.width > 0);
      img.save("/tmp/omarchy-proxy-editor.png");
    }
    function test_full_catalogue_forms() {
      var req = new XMLHttpRequest();
      req.open("GET", Qt.resolvedUrl("../../crates/omm-config/src/proxy_catalog.json"), false);
      req.send();
      if (!req.responseText) {
        skip("Run with QML_XHR_ALLOW_FILE_READ=1 to test the full catalogue");
        return;
      }
      var catalog = JSON.parse(req.responseText);
      for (var i = 0; i < catalog.nodes.length; i++) {
        var e = createTemporaryObject(editorComponent, scene, {
          catalog: catalog,
          draft: {
            name: "Test",
            type: catalog.nodes[i].type
          }
        });
        verify(e);
        wait(5);
        compare(e.schema.fields.length, catalog.nodes[i].fields.length);
        e.section = "advanced";
        wait(5);
        e.destroy();
      }
      var wg = catalog.nodes.filter(function (n) {
        return n.type === "wireguard";
      })[0];
      var awg = wg.fields.filter(function (f) {
        return f.key === "amnezia-wg-option";
      })[0];
      var f = createTemporaryObject(fieldComponent, scene, {
        field: awg,
        value: {
          version: 2,
          jc: 4,
          i1: "<b 0x0000><r 100>"
        },
        expanded: true
      });
      verify(f);
      wait(20);
    }
  }
}
