pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import qs.Commons
import qs.Commons as Commons
import "ProxyModels.js" as Models

Item {
  id: root
  property var svc
  property var metadata: ({})
  property string inputText: ""
  property string filePath: ""
  property var rows: []
  property var memberOf: []
  property string error: ""
  property bool busy: false
  property bool createGroup: false
  property string newGroup: ""
  property var groupItems: []
  readonly property var selectedRows: rows.filter(function (r) {
    return r.selected && r.definition && r.action !== "skip";
  })
  onStepChanged: if (step === 2 && svc)
    svc.request("proxy_groups.list", null, function (r, e) {
      if (!e)
        groupItems = r.items || [];
    })
  property int step: 0
  property int parseGeneration: 0
  property bool parsed: false
  readonly property var validRows: rows.filter(function (r) {
    return !!r.definition;
  })
  readonly property var detectedTypes: validRows.map(function (r) {
    return r.definition.type;
  }).filter(function (t, i, a) {
    return a.indexOf(t) === i;
  })
  onInputTextChanged: {
    parsed = false;
    parseGeneration++;
    if (step === 0 && !filePath)
      previewTimer.restart();
  }
  onFilePathChanged: {
    parsed = false;
    parseGeneration++;
    if (step === 0 && filePath)
      previewTimer.restart();
  }
  Timer {
    id: previewTimer
    interval: 450
    onTriggered: if (root.inputText.trim() || root.filePath)
      root.parse(false)
  }
  FileDialog {
    id: fileDialog
    title: MihomoI18n.t("proxyOpenFile")
    nameFilters: ["Proxy files (*.yaml *.yml *.conf *.txt)", "All files (*)"]
    onAccepted: root.filePath = decodeURIComponent(String(selectedFile).replace(/^file:\/\//, ""))
  }
  readonly property bool typing: true
  signal cancelled
  signal saved(bool reloaded)
  function changeRow(index, key, value) {
    var a = Models.clone(rows);
    a[index][key] = value;
    rows = a;
  }
  function parse(advance) {
    if (advance === undefined)
      advance = true;
    previewTimer.stop();
    var generation = ++parseGeneration;
    busy = true;
    error = "";
    svc.request("proxy_import.preview", filePath ? {
      path: filePath
    } : {
      content: inputText
    }, function (r, e) {
      if (generation !== parseGeneration) {
        busy = false;
        return;
      }
      busy = false;
      if (e) {
        error = e.message;
        return;
      }
      rows = Models.imported(r, metadata.targets || []);
      parsed = true;
      if (advance)
        step = 1;
    });
  }
  function commit() {
    var selected = rows.filter(function (r) {
      return r.selected && r.definition && r.action !== "skip";
    });
    if (!selected.length) {
      error = MihomoI18n.t("proxySelectNodes");
      return;
    }
    var items = selected.map(function (r) {
      var d = Models.clone(r.definition);
      if (r.action === "replace")
        d.name = r.original;
      return d;
    });
    if (createGroup && (!newGroup.trim() || (metadata.targets || []).indexOf(newGroup.trim()) >= 0 || items.some(function (n) {
        return n.name === newGroup.trim();
      }))) {
      error = MihomoI18n.t("nodeCollision");
      return;
    }
    busy = true;
    error = "";
    svc.request("proxy_import.commit", {
      profile_id: metadata.profile_id,
      revision: metadata.revision,
      items: items,
      replace: selected.filter(function (r) {
        return r.action === "replace";
      }).map(function (r) {
        return r.original;
      }),
      member_of: memberOf,
      new_group: createGroup ? newGroup.trim() : null
    }, function (r, e) {
      busy = false;
      if (e) {
        error = e.message;
        return;
      }
      saved(r.reloaded);
    });
  }
  Column {
    id: head
    width: parent.width
    spacing: Style.space(8)
    Row {
      spacing: Style.space(6)
      Txt {
        text: MihomoI18n.t("navProxies")
        color: Commons.Color.accent
        font.pixelSize: Style.font.bodySmall
        MouseArea {
          anchors.fill: parent
          cursorShape: Qt.PointingHandCursor
          onClicked: root.cancelled()
        }
      }
      Txt {
        text: "› " + MihomoI18n.t("proxyImport")
        dim: true
        font.pixelSize: Style.font.bodySmall
      }
    }
    Txt {
      text: MihomoI18n.t("proxyImportTitle")
      bold: true
      font.pixelSize: Style.font.heading
    }
    Txt {
      text: root.step === 0 ? MihomoI18n.t("proxyImportDescription") : root.step === 1 ? MihomoI18n.t("nodeImportReview") : MihomoI18n.t("nodeImportTarget").arg(root.selectedRows.length)
      dim: true
      width: parent.width
      wrapMode: Text.Wrap
    }
    Row {
      width: parent.width
      spacing: Style.space(16)
      Repeater {
        model: ["proxyInputStep", "proxyPreviewStep", "proxyGroups"]
        Row {
          required property string modelData
          required property int index
          spacing: Style.space(6)
          Rectangle {
            width: Style.space(20)
            height: width
            radius: width / 2
            color: parent.index === root.step ? Commons.Color.accent : Util.alpha(Commons.Color.popups.text, 0.1)
            Txt {
              anchors.centerIn: parent
              text: parent.parent.index + 1
              color: parent.parent.index === root.step ? Commons.Color.background : Commons.Color.popups.text
              font.pixelSize: Style.font.bodySmall
            }
          }
          Txt {
            text: MihomoI18n.t(parent.modelData)
            dim: parent.index !== root.step
            anchors.verticalCenter: parent.verticalCenter
            font.pixelSize: Style.font.body
          }
        }
      }
    }
    Txt {
      text: root.error
      visible: text !== ""
      color: Commons.Color.urgent
      width: parent.width
      wrapMode: Text.Wrap
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
      Column {
        visible: root.step === 0
        width: parent.width
        spacing: Style.space(10)
        Row {
          width: parent.width
          spacing: Style.space(8)
          Txt {
            width: Math.max(0, parent.width - pasteButton.width - formatLabel.width - Style.space(16))
            text: MihomoI18n.t("proxyPasteText")
            anchors.verticalCenter: parent.verticalCenter
          }
          Btn {
            id: pasteButton
            text: MihomoI18n.t("proxyPaste")
            onClicked: {
              root.filePath = "";
              inputArea.paste();
              inputArea.forceActiveFocus();
            }
          }
          Txt {
            id: formatLabel
            text: MihomoI18n.t("proxyFormatAuto")
            dim: true
            font.pixelSize: Style.font.bodySmall
            anchors.verticalCenter: parent.verticalCenter
          }
        }
        Rectangle {
          width: parent.width
          height: Style.space(270)
          radius: Style.cornerRadius
          color: Util.alpha(Commons.Color.popups.text, 0.025)
          border.width: 1
          border.color: inputArea.activeFocus ? Commons.Color.accent : Util.alpha(Commons.Color.popups.text, 0.15)
          ScrollView {
            id: inputScroll
            anchors.fill: parent
            anchors.margins: Style.space(6)
            clip: true
            TextArea {
              id: inputArea
              text: root.inputText
              placeholderText: "vless://…\ntrojan://…\n\n[Interface]\nPrivateKey = …"
              wrapMode: TextEdit.NoWrap
              leftPadding: Style.space(32)
              Text {
                x: Style.space(4)
                y: inputArea.topPadding
                text: Array.from({
                  length: root.inputText.split("\n").length
                }, function (_, i) {
                  return i + 1;
                }).join("\n")
                color: Commons.Color.muted
                font: inputArea.font
              }
              selectByMouse: true
              color: Commons.Color.popups.text
              font.family: Style.font.family
              font.pixelSize: Style.font.body
              background: null
              onTextChanged: if (activeFocus) {
                root.filePath = "";
                root.inputText = text;
              }
            }
          }
          DropArea {
            anchors.fill: parent
            onDropped: function (drop) {
              if (drop.hasUrls && String(drop.urls[0]).indexOf("file://") === 0) {
                root.filePath = decodeURIComponent(String(drop.urls[0]).replace(/^file:\/\//, ""));
                drop.acceptProposedAction();
              } else if (drop.hasText) {
                root.filePath = "";
                root.inputText = drop.text;
                drop.acceptProposedAction();
              }
            }
          }
        }
        Flow {
          width: parent.width
          spacing: Style.space(6)
          Txt {
            text: MihomoI18n.t("proxyRecognized")
            dim: true
            font.pixelSize: Style.font.bodySmall
          }
          Repeater {
            model: root.detectedTypes
            Chip {
              required property string modelData
              text: root.validRows.filter(function (r) {
                return r.definition.type === modelData;
              }).length + " " + modelData
            }
          }
        }
        Repeater {
          model: root.rows.filter(function (r) {
            return !!r.error;
          })
          Txt {
            required property var modelData
            width: parent.width
            text: MihomoI18n.t("proxyLine") + " " + modelData.index + ": " + modelData.error
            color: Commons.Color.urgent
            wrapMode: Text.Wrap
            font.pixelSize: Style.font.bodySmall
          }
        }
        Row {
          width: parent.width
          spacing: Style.space(8)
          Btn {
            id: openFileButton
            text: MihomoI18n.t("proxyOpenFile")
            icon: Icons.file
            onClicked: fileDialog.open()
          }
          Txt {
            width: parent.width - openFileButton.width - Style.space(8)
            text: root.filePath || MihomoI18n.t("proxyDropHint")
            elide: Text.ElideMiddle
            dim: true
            font.pixelSize: Style.font.bodySmall
            anchors.verticalCenter: parent.verticalCenter
          }
        }
      }

      Column {
        visible: root.step === 1
        width: parent.width
        spacing: Style.space(10)
        Txt {
          text: MihomoI18n.t("proxyReviewWarnings")
          width: parent.width
          wrapMode: Text.Wrap
          dim: true
        }
        Repeater {
          model: root.rows.length
          Rectangle {
            id: entry
            required property int index
            readonly property var row: root.rows[index]
            width: parent.width
            height: entryBody.implicitHeight + Style.space(16)
            radius: Style.cornerRadius
            color: Util.alpha(Commons.Color.popups.text, 0.04)
            Column {
              id: entryBody
              x: Style.space(8)
              y: Style.space(8)
              width: parent.width - Style.space(16)
              spacing: Style.space(6)
              ProxyCheck {
                text: entry.row.definition ? entry.row.definition.type + " · " + entry.row.original : MihomoI18n.t("proxyLine") + " " + entry.row.index
                enabled: !!entry.row.definition
                checked: entry.row.selected
                onToggled: root.changeRow(entry.index, "selected", checked)
              }
              Txt {
                visible: !!entry.row.definition
                width: parent.width
                text: entry.row.definition ? (entry.row.definition.server || "") + (entry.row.definition.port ? ":" + entry.row.definition.port : "") : ""
                elide: Text.ElideRight
                dim: true
                font.pixelSize: Style.font.bodySmall
              }
              Row {
                visible: !!entry.row.definition && (root.metadata.targets || []).indexOf(entry.row.original) >= 0
                width: parent.width
                spacing: Style.space(8)
                Seg {
                  model: [
                    {
                      key: "rename",
                      label: MihomoI18n.t("proxyRename")
                    },
                    {
                      key: "replace",
                      label: MihomoI18n.t("proxyReplace")
                    },
                    {
                      key: "skip",
                      label: MihomoI18n.t("proxySkip")
                    }
                  ]
                  current: entry.row.action
                  onPicked: function (k) {
                    root.changeRow(entry.index, "action", k);
                  }
                }
              }
              Field {
                visible: !!entry.row.definition && (root.metadata.targets || []).indexOf(entry.row.original) >= 0 && entry.row.action === "rename"
                width: parent.width
                text: entry.row.definition ? entry.row.definition.name : ""
                input.onTextEdited: {
                  var d = Models.clone(entry.row.definition);
                  d.name = text;
                  root.changeRow(entry.index, "definition", d);
                }
              }
              Txt {
                visible: entry.row.error !== "" || entry.row.warnings.length
                text: entry.row.error || entry.row.warnings.join("\n")
                width: parent.width
                wrapMode: Text.Wrap
                color: Commons.Color.urgent
                font.pixelSize: Style.font.body
              }
            }
          }
        }
      }
      Column {
        visible: root.step === 2
        width: parent.width
        spacing: Style.space(8)
        Txt {
          text: MihomoI18n.t("proxyMemberOf")
          bold: true
        }
        Repeater {
          model:root.metadata.groups || []
          Column {
            id:importGroup
            required property string modelData
            readonly property var details:root.groupItems.filter(function(g) { return g.name===modelData })[0] || ({})
            width:parent.width; spacing:Style.space(4)
            ProxyCheck { text:importGroup.modelData; enabled:!importGroup.details.automatic; checked:root.memberOf.indexOf(importGroup.modelData)>=0; onToggled:root.memberOf=Models.toggleMember(root.memberOf,importGroup.modelData,checked) }
            Txt { width:parent.width; text:importGroup.details.automatic ? MihomoI18n.t("nodeAutoGroup") : (MihomoI18n.t("proxyType_"+(importGroup.details.type || "select")) + " · " + (importGroup.details.members || []).length + (root.memberOf.indexOf(importGroup.modelData)>=0 ? " → " + ((importGroup.details.members || []).length+root.selectedRows.length) : "")); dim:true; font.pixelSize:Style.font.bodySmall; wrapMode:Text.Wrap }
          }
        }
        ProxyCheck {
          text: MihomoI18n.t("nodeImportGroup")
          checked: root.createGroup
          onToggled: root.createGroup = checked
        }
        Field {
          visible: root.createGroup
          width: parent.width
          placeholder: MihomoI18n.t("proxyGroup")
          text: root.newGroup
          input.onTextEdited: root.newGroup = text
        }
        Rectangle {
          width: parent.width
          height: summary.implicitHeight + Style.space(24)
          radius: Style.cornerRadius
          color: Util.alpha(Commons.Color.popups.text, 0.04)
          Column {
            id: summary
            x: Style.space(12)
            y: Style.space(12)
            width: parent.width - Style.space(24)
            spacing: Style.space(8)
            Txt {
              text: MihomoI18n.t("nodeImportSummary")
              bold: true
            }
            Txt {
              width: parent.width
              text: MihomoI18n.t("nodeImportCount").arg(root.metadata.profile_name || "").arg(root.selectedRows.length)
              wrapMode: Text.Wrap
              dim: true
              font.pixelSize: Style.font.body
            }
            Repeater {
              model: root.memberOf
              Txt {
                required property string modelData
                text: modelData + ": +" + root.selectedRows.length
                dim: true
                font.pixelSize: Style.font.body
              }
            }
            Txt {
              visible: root.createGroup
              text: root.newGroup + ": +" + root.selectedRows.length
              dim: true
              font.pixelSize: Style.font.body
            }
            Txt {
              width: parent.width
              text: MihomoI18n.t("nodeImportCheck")
              wrapMode: Text.Wrap
              dim: true
              font.pixelSize: Style.font.body
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
      width: Math.max(0, parent.width - backButton.width - nextButton.width - Style.space(16))
      text: root.step === 0 ? MihomoI18n.t("proxyLocalParsing") : MihomoI18n.t("nodeImportedCount").arg(root.selectedRows.length)
      elide: Text.ElideRight
      font.pixelSize: Style.font.bodySmall
      dim: true
      anchors.verticalCenter: parent.verticalCenter
    }
    Btn {
      id: backButton
      text: root.step ? MihomoI18n.t("proxyBack") : MihomoI18n.t("cancel")
      enabled: !root.busy
      onClicked: {
        if (root.step)
          root.step--;
        else
          root.cancelled();
      }
    }
    Btn {
      id: nextButton
      text: root.step === 0 ? MihomoI18n.t("proxyImportNext").arg(root.validRows.length) : root.step === 1 ? MihomoI18n.t("proxyNext") : MihomoI18n.t("proxyImportApply")
      primary: true
      busy: root.busy
      enabled: !root.busy && (root.step === 0 ? !!root.inputText.trim() || !!root.filePath : root.selectedRows.length > 0)
      onClicked: {
        if (root.step === 0)
          root.parse();
        else if (root.step === 1)
          root.step = 2;
        else
          root.commit();
      }
    }
  }
}
