import QtQuick
import QtQml
import QtQuick.Controls
import qs.Commons
import "RuleModels.js" as RuleModels

// Create or edit one of the user's rules: from a connection (scopes it
// suggests, with what each would cover) or from scratch (every mihomo rule
// type, with suggestions while typing). Nothing is saved until Save.
//
// panel.editing: { from: "connections" | "rules", origin?, rule?, kind?, value?, full? }
Item {
  id: root
  property var svc
  property var panel

  readonly property var editing: panel.editing || ({})
  readonly property var origin: editing.origin || null
  readonly property var rule: editing.rule || null
  readonly property string from: editing.from || "rules"
  // Opened by SUPER+ALT+P: the focused window and the processes under it.
  readonly property var win: editing.window || null
  readonly property var candidates: editing.candidates || []

  // Connection scopes (`rules.scopes`) and the chosen one.
  property var scopes: []
  property int scopeIndex: -1
  // Full form: the user picks the type.
  property bool full: !origin || !!editing.full || !!rule
  property string tabKey: "domain"
  property string kind: "domain-suffix"
  property var parts: [{ kind: "process-name", value: "" }, { kind: "domain-suffix", value: "" }]
  property int partsRevision: 0

  property var targets: []
  property string target: ""
  property var results: ({})
  property bool testing: false
  property string lifetimeKey: "always"
  property bool reconnect: true
  property bool saving: false
  property var preview: null
  property string error: ""
  property int previewSerial: 0
  readonly property bool typing: valueInput.input.activeFocus || noteField.input.activeFocus || conditionFocus
  property bool conditionFocus: false

  readonly property var scope: scopes[scopeIndex] || null
  // The rule being edited (a duplicate is a new one).
  readonly property int selfId: rule && !editing.duplicate ? rule.id : -1
  readonly property string effectiveKind: full ? kind : (scope ? scope.kind : "")
  readonly property string effectiveValue: {
    partsRevision
    if (!full) return scope ? scope.value : ""
    if (RuleModels.isLogic(kind)) return RuleModels.logicValue(parts)
    return valueInput.text.trim()
  }
  readonly property string probeHost: RuleModels.probeHost(effectiveKind, preview ? preview.value : effectiveValue, origin)
  readonly property var chipModel: RuleModels.chips(targets, target, 4)
  readonly property bool canSave: effectiveKind !== "" && effectiveValue !== "" && target !== "" && !saving && error === ""

  function back() { if (from === "window") panel.close(); else panel.showPage(from) }

  function pickCandidate(c) {
    var next = Object.assign({}, editing)
    next.origin = Object.assign({}, origin, { process: c.name, process_path: c.path, pid: c.pid, connections: c.connections })
    panel.openRule(next)
  }
  function kindTitle(k) { return I18n.t("ruleTab_" + RuleModels.tabOf(k)) + " · " + I18n.t("variant_" + k) }

  function init() {
    if (rule) {
      kind = rule.kind
      tabKey = RuleModels.tabOf(rule.kind)
      if (RuleModels.isLogic(rule.kind)) parts = (rule.parts || []).map(function(p) { return { kind: p.kind, value: p.value } })
      else valueInput.text = rule.value
      target = rule.effective_target || rule.target
      noteField.text = rule.note || ""
      lifetimeKey = RuleModels.lifetimeKey(rule)
    } else if (editing.kind) {
      kind = editing.kind
      tabKey = RuleModels.tabOf(editing.kind)
      if (editing.value) valueInput.text = editing.value
    }
    if (origin) {
      svc.request("rules.scopes", origin, function(r, err) {
        if (err || !r) return
        scopes = r
        var want = editing.kind || "domain-suffix"
        var idx = 0
        for (var i = 0; i < r.length; i++) if (r[i].kind === want) { idx = i; break }
        scopeIndex = r.length ? idx : -1
        if (r.length === 0) full = true
      })
    }
    loadTargets()
  }

  function loadTargets() {
    svc.request("proxies.list", null, function(groups, err) {
      if (err || !groups) { error = err ? err.message : ""; return }
      var list = [{ name: "DIRECT", kind: "Direct", now: "" }]
      for (var i = 0; i < groups.length; i++) {
        if (groups[i].name === "GLOBAL") continue
        list.push({ name: groups[i].name, kind: groups[i].kind, now: groups[i].now || "" })
      }
      list.push({ name: "REJECT", kind: "Reject", now: "" })
      list.push({ name: "REJECT-DROP", kind: "Reject", now: "" })
      if (target && !list.some(function(t) { return t.name === target })) list.push({ name: target, kind: "Proxy", now: "" })
      targets = list
      if (target === "") {
        // The group the connection uses now, else the first group.
        var current = origin && origin.route ? origin.route.split(" → ")[0] : ""
        target = list.some(function(t) { return t.name === current }) ? current : (list.length > 1 ? list[1].name : "DIRECT")
      }
      testTargets()
    })
  }

  function testTargets() {
    var host = probeHost
    if (!host) return
    var names = chipModel.visible.concat(chipModel.more).map(function(t) { return t.name }).filter(function(n) { return n.indexOf("REJECT") !== 0 }).slice(0, 12)
    if (names.length === 0) return
    testing = true
    svc.request("rules.test", { host: host, port: origin && origin.port ? parseInt(origin.port) : 443, targets: names }, function(r, err) {
      testing = false
      if (err || !r) return
      var next = Object.assign({}, results)
      for (var i = 0; i < r.results.length; i++) next[r.results[i].target] = r.results[i]
      results = next
    })
  }

  function refreshPreview() { previewTimer.restart() }
  function loadPreview() {
    var mine = ++previewSerial
    if (effectiveKind === "" || effectiveValue === "" || effectiveValue === "()") { preview = null; error = ""; return }
    svc.request("rules.preview", { kind: effectiveKind, value: effectiveValue, target: target || "DIRECT" }, function(r, err) {
      if (mine !== previewSerial) return
      if (err) { preview = null; error = err.message; return }
      error = ""
      preview = r
    })
  }

  function pickTab(key) {
    tabKey = key
    kind = RuleModels.tab(key).variants[0]
  }
  function showFull() {
    if (scope) {
      kind = scope.kind
      tabKey = RuleModels.tabOf(scope.kind)
      valueInput.text = scope.value
    }
    full = true
  }
  function setPart(i, field, value) {
    if (field === "value") parts[i].value = value
    else { var next = parts.slice(); next[i] = { kind: value, value: next[i].value }; parts = next }
    partsRevision++
  }
  function addPart() { parts = parts.concat([{ kind: "domain-suffix", value: "" }]); partsRevision++ }
  function removePart(i) { var next = parts.slice(); next.splice(i, 1); parts = next; partsRevision++ }

  function previewText() {
    if (error !== "") return error
    if (!preview) return ""
    var c = preview.coverage || {}
    var bits = []
    if (preview.existing && !preview.existing.id === selfId) bits.push(I18n.t("ruleExists").arg(preview.existing.target))
    if (c.connections > 0) bits.push(I18n.t("coverConns").arg(c.connections))
    if (c.seen > 0) bits.push(I18n.t("coverSeen").arg(c.seen))
    if (c.unknown) bits.push(I18n.t("coverUnknown"))
    if (!c.connections && !c.seen && !c.unknown) bits.push(I18n.t("coverNone"))
    var routes = (c.routes || []).filter(function(r) { return r.target !== target || r.rule === "" })
    if (routes.length) bits.push(I18n.t("overrides").arg(routes.map(function(r) { return r.rule + (r.payload ? "(" + r.payload + ")" : "") + " → " + r.target }).join(", ")))
    return bits.join(" · ")
  }
  readonly property string previewTone: error !== "" ? "error" : (preview && ((preview.existing && !preview.existing.id === selfId) || !(preview.coverage && (preview.coverage.connections || preview.coverage.seen || preview.coverage.unknown))) ? "warn" : "ok")

  function save() {
    if (!canSave) return
    saving = true
    var res = results[target]
    var params = {
      kind: effectiveKind, value: effectiveValue, target: target, note: noteField.text.trim(),
      reconnect: reconnect,
      test: res ? { target: target, ok: !!res.ok, delay: res.delay || null, error: res.error || null, at: Date.now() } : null
    }
    if (lifetimeKey !== "until") params.lifetime = RuleModels.lifetime(lifetimeKey, Date.now())
    else params.lifetime = { expires: rule.expires, until_reboot: false }
    if (rule && !editing.duplicate && (rule.kind !== effectiveKind || (preview && rule.value !== preview.value))) params.replace = rule.id
    svc.request("rules.save", params, function(r, err) {
      saving = false
      if (err) { error = err.message; return }
      var msg = I18n.t("savedToast").arg(r.rule ? RuleModels.display(r.rule.kind, r.rule.value) : effectiveValue).arg(target)
      if (r.closed > 0) msg += " · " + I18n.t("closedConns").arg(r.closed)
      if (r.apply && !r.apply.reloaded) msg += " · " + I18n.t("notReloaded")
      if (from === "window" && r.rule) {
        // The panel closes; the notification keeps the way back.
        if (!svc.automation || !svc.automation.notify || svc.automation.notify.saved)
          svc.raise("saved", "saved", "normal", { rule_id: r.rule.id, line: RuleModels.display(r.rule.kind, r.rule.value) + " → " + r.rule.target })
        panel.close()
        return
      }
      panel.notify(msg)
      back()
    })
  }

  // Keyboard: ←→ route, ↑↓ scope, Enter save.
  function move(dx, dy) {
    if (dx !== 0 && targets.length) {
      var all = chipModel.visible
      var i = 0
      for (var k = 0; k < all.length; k++) if (all[k].name === target) i = k
      target = all[Math.max(0, Math.min(all.length - 1, i + dx))].name
    } else if (dy !== 0 && !full && scopes.length) {
      scopeIndex = Math.max(0, Math.min(scopes.length - 1, scopeIndex + dy))
    }
  }
  function activate() { save() }
  function textKey(t) {
    if (t === "t") testTargets()
    else if (t === "/" || t === "i") { if (!full) showFull(); valueInput.input.forceActiveFocus() }
  }

  onEffectiveKindChanged: refreshPreview()
  onEffectiveValueChanged: refreshPreview()
  onTargetChanged: refreshPreview()
  onProbeHostChanged: if (targets.length) testTimer.restart()
  Component.onCompleted: init()
  Timer { id: previewTimer; interval: 200; onTriggered: root.loadPreview() }
  Timer { id: testTimer; interval: 900; onTriggered: root.testTargets() }

  component SectionLabel: Row {
    property string label: ""
    property string hint: ""
    spacing: Style.space(8)
    Txt { text: parent.label; faint: true; bold: true; font.pixelSize: Style.font.caption; font.letterSpacing: 1 }
    Txt { text: parent.hint; faint: true; font.pixelSize: Style.font.caption }
  }

  Row {
    id: header
    width: parent.width
    spacing: Style.space(12)
    Btn { icon: Icons.back; tooltip: I18n.t("hintBack"); anchors.verticalCenter: parent.verticalCenter; onClicked: root.back() }
    Column {
      width: parent.width - Style.space(40)
      spacing: Style.space(3)
      Txt { text: root.rule && !root.editing.duplicate ? I18n.t("ruleEditTitle") : I18n.t("ruleNewTitle"); bold: true; font.pixelSize: Style.font.heading }
      Txt {
        width: parent.width
        text: root.win ? I18n.t("activeWindow") + " › " + (root.win.title || root.win.cls) : (root.from === "connections" ? I18n.t("navConnections") : I18n.t("navRules")) + " › "
          + (root.origin ? (root.origin.host || root.origin.ip) + (root.origin.port ? ":" + root.origin.port : "")
          : (root.rule ? RuleModels.display(root.rule.kind, root.rule.value) : I18n.t("ruleNewCrumb")))
        dim: true; font.pixelSize: Style.font.bodySmall
      }
    }
  }

  Flickable {
    id: body
    anchors.top: header.bottom; anchors.topMargin: Style.space(14)
    anchors.left: parent.left; anchors.right: parent.right
    anchors.bottom: bottom.top; anchors.bottomMargin: Style.space(10)
    contentHeight: form.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

    Column {
      id: form
      width: body.width - Style.space(6)
      spacing: Style.space(10)

      // What the connection is.
      Rectangle {
        visible: root.origin !== null
        width: parent.width
        height: facts.implicitHeight + Style.space(20)
        radius: Style.cornerRadius
        color: Util.alpha(Color.popups.text, 0.04)
        border.width: 1
        border.color: Util.alpha(Color.popups.text, 0.12)
        Row {
          id: facts
          anchors.fill: parent
          anchors.margins: Style.space(10)
          spacing: Style.space(12)
          Repeater {
            model: root.win ? [
              [I18n.t("factWindow"), root.win.title || root.win.cls, root.win.cls ? "class " + root.win.cls : ""],
              [I18n.t("factProcess"), root.origin.process, "pid " + root.origin.pid + " · " + root.origin.process_path],
              [I18n.t("factRoute"), root.origin.route || "—", I18n.t("connsShort").arg(root.origin.connections || 0)]
            ] : root.origin ? [
              [I18n.t("factHost"), root.origin.host || root.origin.ip, (root.origin.ip ? root.origin.ip + ":" + root.origin.port : "") + (root.origin.network ? " · " + root.origin.network : "")],
              [I18n.t("factProcess"), root.origin.process || I18n.t("unknownApp"), root.origin.process_path],
              [I18n.t("factRoute"), root.origin.route || "—", root.origin.rule]
            ] : []
            Column {
              required property var modelData
              width: (facts.width - facts.spacing * 2) / 3
              spacing: Style.space(3)
              Txt { text: modelData[0]; faint: true; font.pixelSize: Style.font.caption }
              Txt { width: parent.width; text: modelData[1]; bold: true }
              Txt { width: parent.width; text: modelData[2] || ""; dim: true; font.pixelSize: Style.font.caption }
            }
          }
        }
      }

      // A terminal: pick which of the programs in it the rule is for.
      SectionLabel { visible: root.candidates.length > 1; label: I18n.t("windowProcess"); hint: I18n.t("windowProcessHint") }
      Flow {
        visible: root.candidates.length > 1
        width: parent.width
        spacing: Style.space(6)
        Repeater {
          model: root.candidates
          Btn {
            required property var modelData
            text: modelData.name + "  " + I18n.t("connsShort").arg(modelData.connections)
            primary: root.origin && modelData.path === root.origin.process_path
            onClicked: root.pickCandidate(modelData)
          }
        }
      }

      SectionLabel { label: I18n.t("matchLabel"); hint: root.full ? "" : I18n.t(root.win ? "matchFromWindow" : "matchFromConn") }

      // Scopes a connection suggests.
      Grid {
        id: scopeGrid
        visible: !root.full
        width: parent.width
        columns: 2
        spacing: Style.space(8)
        Repeater {
          model: root.scopes
          Rectangle {
            required property var modelData
            required property int index
            readonly property bool on: index === root.scopeIndex
            objectName: "scope_" + modelData.kind
            width: (scopeGrid.width - scopeGrid.spacing) / 2
            height: Style.space(58)
            radius: Style.cornerRadius
            color: on ? Style.selectedFillFor(Color.popups.text, Color.accent) : Util.alpha(Color.popups.text, 0.04)
            border.width: 1
            border.color: on ? Color.accent : Util.alpha(Color.popups.text, 0.15)
            Rectangle {
              id: radio
              anchors.left: parent.left; anchors.leftMargin: Style.space(12)
              anchors.top: parent.top; anchors.topMargin: Style.space(11)
              width: Style.space(14); height: width; radius: width / 2
              color: "transparent"
              border.width: 1
              border.color: parent.on ? Color.accent : Util.alpha(Color.popups.text, 0.4)
              Rectangle { visible: parent.parent.on; anchors.centerIn: parent; width: Style.space(6); height: width; radius: width / 2; color: Color.accent }
            }
            Column {
              anchors.left: radio.right; anchors.leftMargin: Style.space(10)
              anchors.right: parent.right; anchors.rightMargin: Style.space(10)
              anchors.verticalCenter: parent.verticalCenter
              spacing: Style.space(2)
              Txt { width: parent.width; text: (modelData.kind === "process-name" ? I18n.t("processPrefix") + " " : "") + RuleModels.display(modelData.kind, modelData.value); bold: true }
              Txt { width: parent.width; text: I18n.matchKindLong(modelData.kind); dim: true; font.pixelSize: Style.font.caption }
              Row {
                spacing: Style.space(8)
                Txt { text: RuleModels.keyword(modelData.kind, modelData.value); color: root.svc.mauve; font.pixelSize: Style.font.caption }
                Txt {
                  text: [modelData.connections ? I18n.t("coverConns").arg(modelData.connections) : "", modelData.seen ? I18n.t("coverSeen").arg(modelData.seen) : ""].filter(function(s) { return s }).join(" · ")
                  faint: true; font.pixelSize: Style.font.caption
                }
              }
            }
            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.scopeIndex = index }
          }
        }
      }
      Txt {
        visible: !root.full
        text: Icons.add + " " + I18n.t("otherType")
        color: Color.accent
        font.pixelSize: Style.font.bodySmall
        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.showFull() }
      }

      // Every type.
      Rectangle {
        visible: root.full
        width: parent.width
        height: Style.space(32)
        radius: Style.cornerRadius
        color: Util.alpha(Color.popups.text, 0.04)
        Row {
          anchors.fill: parent
          anchors.margins: Style.space(3)
          spacing: Style.space(2)
          Repeater {
            model: RuleModels.tabs
            Rectangle {
              required property var modelData
              readonly property bool on: modelData.key === root.tabKey
              objectName: "tab_" + modelData.key
              width: (parent.width - Style.space(2) * (RuleModels.tabs.length - 1)) / RuleModels.tabs.length
              height: parent.height
              radius: Style.cornerRadius
              color: on ? Style.selectedFillFor(Color.popups.text, Color.accent) : (tm.containsMouse ? Style.hoverFillFor(Color.popups.text, Color.accent) : "transparent")
              Row {
                anchors.centerIn: parent
                spacing: Style.space(6)
                Txt { text: Icons[modelData.icon]; color: parent.parent.on ? Color.accent : Util.alpha(Color.popups.text, 0.42) }
                Txt { text: I18n.t("ruleTab_" + modelData.key); bold: parent.parent.on; dim: !parent.parent.on; font.pixelSize: Style.font.bodySmall }
              }
              MouseArea { id: tm; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.pickTab(modelData.key) }
            }
          }
        }
      }
      Flow {
        visible: root.full
        width: parent.width
        spacing: Style.space(6)
        Repeater {
          model: RuleModels.tab(root.tabKey).variants
          Rectangle {
            required property string modelData
            readonly property bool on: modelData === root.kind
            objectName: "variant_" + modelData
            width: variantText.implicitWidth + Style.space(20)
            height: variantText.implicitHeight + Style.space(12)
            radius: Style.cornerRadius
            color: on ? Style.selectedFillFor(Color.popups.text, Color.accent) : Util.alpha(Color.popups.text, 0.04)
            border.width: 1
            border.color: on ? Color.accent : Util.alpha(Color.popups.text, 0.15)
            Column {
              id: variantText
              anchors.centerIn: parent
              spacing: Style.space(1)
              Txt { text: I18n.t("variant_" + modelData); bold: parent.parent.on; dim: !parent.parent.on; font.pixelSize: Style.font.bodySmall }
              Txt { text: RuleModels.keyword(modelData, ""); color: root.svc.mauve; font.pixelSize: Style.font.caption - 1 }
            }
            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.kind = modelData }
          }
        }
      }

      SectionLabel {
        visible: root.full
        label: I18n.t("valueLabel_" + root.tabKey)
        hint: I18n.t("valueHint_" + root.kind)
      }
      RuleValueInput {
        id: valueInput
        visible: root.full && !RuleModels.isLogic(root.kind)
        width: parent.width
        svc: root.svc
        kind: root.kind
        placeholder: I18n.t("valuePlaceholder_" + root.kind)
        onTextChanged: root.refreshPreview()
        onAccepted: { root.panel.forceKeys() }
        onEscaped: { input.focus = false; root.panel.forceKeys() }
      }

      // AND / OR / NOT conditions.
      Column {
        visible: root.full && RuleModels.isLogic(root.kind)
        width: parent.width
        spacing: Style.space(6)
        Repeater {
          model: root.parts
          Row {
            id: cond
            required property var modelData
            required property int index
            width: parent.width
            spacing: Style.space(6)
            Txt {
              width: Style.space(40)
              anchors.verticalCenter: parent.verticalCenter
              horizontalAlignment: Text.AlignHCenter
              text: cond.index === 0 ? I18n.t(root.kind === "not" ? "logicNot" : "logicIf") : I18n.t(root.kind === "or" ? "logicOr" : "logicAnd")
              color: root.svc.mauve; bold: true; font.pixelSize: Style.font.caption
            }
            Btn {
              id: kindBtn
              width: Style.space(176)
              anchors.verticalCenter: parent.verticalCenter
              text: root.kindTitle(cond.modelData.kind) + " ▾"
              onClicked: { kindMenu.part = cond.index; kindMenu.popup(kindBtn, 0, kindBtn.height + Style.space(4)) }
            }
            RuleValueInput {
              width: parent.width - Style.space(40) - Style.space(176) - Style.space(30) - Style.space(18)
              svc: root.svc
              kind: cond.modelData.kind
              text: cond.modelData.value
              placeholder: I18n.t("valuePlaceholder_" + cond.modelData.kind)
              onTextChanged: if (text !== cond.modelData.value) root.setPart(cond.index, "value", text)
              input.onActiveFocusChanged: root.conditionFocus = input.activeFocus
              onEscaped: { input.focus = false; root.panel.forceKeys() }
            }
            Btn {
              ghost: true
              anchors.verticalCenter: parent.verticalCenter
              icon: Icons.close
              tooltip: I18n.t("delete")
              enabled: root.parts.length > (root.kind === "not" ? 1 : 2)
              onClicked: root.removePart(cond.index)
            }
          }
        }
        Txt {
          visible: root.kind !== "not"
          leftPadding: Style.space(46)
          text: Icons.add + " " + I18n.t("addCondition")
          color: Color.accent
          font.pixelSize: Style.font.bodySmall
          MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.addPart() }
        }
      }

      SectionLabel { label: I18n.t("targetLabel"); hint: I18n.t("ruleTargetHint") }
      Flow {
        width: parent.width
        spacing: Style.space(6)
        Repeater {
          model: root.chipModel.visible
          Rectangle {
            required property var modelData
            readonly property bool on: modelData.name === root.target
            readonly property var res: root.results[modelData.name]
            objectName: "target_" + modelData.name
            width: chipRow.implicitWidth + Style.space(20)
            height: Style.space(28)
            radius: Style.cornerRadius
            color: on ? Style.selectedFillFor(Color.popups.text, Color.accent) : Util.alpha(Color.popups.text, 0.04)
            border.width: 1
            border.color: on ? Color.accent : Util.alpha(Color.popups.text, 0.15)
            Row {
              id: chipRow
              anchors.centerIn: parent
              spacing: Style.space(6)
              Txt { text: modelData.name === "DIRECT" ? Icons.arrow : (modelData.kind === "Reject" ? Icons.ban : Icons.proxies); color: parent.parent.on ? Color.accent : Util.alpha(Color.popups.text, 0.6) }
              Txt { text: modelData.name; bold: parent.parent.on; dim: !parent.parent.on }
              Txt {
                visible: !!parent.parent.res
                text: !parent.parent.res ? "" : (parent.parent.res.ok ? parent.parent.res.delay + " " + I18n.t("ms") : (parent.parent.res.error === "timeout" ? I18n.t("timeout") : I18n.t("failed")))
                color: !parent.parent.res ? "transparent" : (parent.parent.res.ok ? (parent.parent.res.delay < 400 ? root.svc.green : root.svc.yellow) : root.svc.red)
                font.pixelSize: Style.font.caption
              }
            }
            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.target = modelData.name }
          }
        }
        Btn {
          id: moreBtn
          visible: root.chipModel.more.length > 0
          text: I18n.t("moreTargets").arg(root.chipModel.more.length) + " ▾"
          onClicked: moreMenu.popup(moreBtn, 0, moreBtn.height + Style.space(4))
        }
      }

      SectionLabel { label: I18n.t("paramsLabel") }
      Row {
        width: parent.width
        spacing: Style.space(8)
        Column {
          spacing: Style.space(4)
          Txt { text: I18n.t("lifetimeLabel"); dim: true; font.pixelSize: Style.font.caption }
          Btn {
            id: lifetimeBtn
            objectName: "lifetimeButton"
            width: Style.space(170)
            text: (root.lifetimeKey === "until" && root.rule ? I18n.t("lifetimeUntil").arg(root.svc.clock(root.rule.expires)) : I18n.t("lifetime_" + root.lifetimeKey)) + " ▾"
            onClicked: lifetimeMenu.popup(lifetimeBtn, 0, lifetimeBtn.height + Style.space(4))
          }
        }
        Column {
          width: parent.width - Style.space(178)
          spacing: Style.space(4)
          Txt { text: I18n.t("noteLabel"); dim: true; font.pixelSize: Style.font.caption }
          Field {
            id: noteField
            width: parent.width
            placeholder: I18n.t("notePlaceholder")
            onEscaped: { input.focus = false; root.panel.forceKeys() }
            onAccepted: root.panel.forceKeys()
          }
        }
      }
      Item {
        width: reconnectRow.implicitWidth
        height: reconnectRow.implicitHeight
      Row {
        id: reconnectRow
        spacing: Style.space(8)
        Rectangle {
          anchors.verticalCenter: parent.verticalCenter
          width: Style.space(14); height: width; radius: 3
          color: root.reconnect ? Color.accent : "transparent"
          border.width: root.reconnect ? 0 : 1
          border.color: Util.alpha(Color.popups.text, 0.4)
          Txt { anchors.centerIn: parent; visible: root.reconnect; text: Icons.check; color: Color.background; font.pixelSize: Style.font.caption }
        }
        Txt {
          text: I18n.t("reconnectAfter") + (root.preview && root.preview.coverage && root.preview.coverage.connections ? " (" + root.preview.coverage.connections + ")" : "")
          dim: true; font.pixelSize: Style.font.bodySmall
        }
      }
        MouseArea { objectName: "reconnectToggle"; anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.reconnect = !root.reconnect }
      }
    }
  }

  Column {
    id: bottom
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    spacing: Style.space(10)

    Rectangle {
      id: previewBox
      objectName: "rulePreview"
      visible: root.preview !== null || root.error !== ""
      width: parent.width
      height: previewCol.implicitHeight + Style.space(18)
      radius: Style.cornerRadius
      color: Qt.darker(Color.popups.background, 1.15)
      border.width: 1
      border.color: root.previewTone === "error" ? Util.alpha(root.svc.red, 0.5) : Util.alpha(Color.popups.text, 0.15)
      Column {
        id: previewCol
        anchors.left: parent.left; anchors.leftMargin: Style.space(12)
        anchors.right: parent.right; anchors.rightMargin: Style.space(12)
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(4)
        Txt {
          visible: root.preview !== null && root.error === ""
          width: parent.width
          text: root.preview ? root.preview.line : ""
          color: root.svc.green; bold: true; wrapMode: Text.WrapAnywhere; maximumLineCount: 2
        }
        Txt {
          width: parent.width
          text: root.previewText()
          color: root.previewTone === "error" ? root.svc.red : (root.previewTone === "warn" ? root.svc.yellow : Util.alpha(Color.popups.text, 0.68))
          wrapMode: Text.Wrap; maximumLineCount: 3
          font.pixelSize: Style.font.caption
        }
      }
    }

    Row {
      width: parent.width
      spacing: Style.space(8)
      layoutDirection: Qt.RightToLeft
      Btn { id: saveBtn; objectName: "saveRule"; primary: true; icon: Icons.check; text: I18n.t("saveApply"); busy: root.saving; enabled: root.canSave; onClicked: root.save() }
      Btn { id: checkBtn; icon: Icons.test; text: I18n.t("check"); busy: root.testing; enabled: root.probeHost !== ""; onClicked: root.testTargets() }
      Btn { id: cancelBtn; text: I18n.t("cancel"); onClicked: root.back() }
      Txt {
        width: parent.width - saveBtn.width - checkBtn.width - cancelBtn.width - parent.spacing * 3
        anchors.verticalCenter: parent.verticalCenter
        text: I18n.t("ruleKeysHint")
        faint: true; font.pixelSize: Style.font.caption; horizontalAlignment: Text.AlignLeft
      }
    }
  }

  ActionMenu {
    id: moreMenu
    Instantiator {
      model: root.chipModel.more
      delegate: ActionMenuItem {
        required property var modelData
        text: modelData.name
        detail: modelData.kind + (modelData.now ? " · " + modelData.now : "")
        radio: true
        marked: modelData.name === root.target
        onTriggered: root.target = modelData.name
      }
      onObjectAdded: function(index, object) { moreMenu.insertItem(index, object) }
      onObjectRemoved: function(index, object) { moreMenu.removeItem(object) }
    }
  }
  ActionMenu {
    id: lifetimeMenu
    Instantiator {
      model: ["always", "hour", "day", "boot"]
      delegate: ActionMenuItem {
        required property string modelData
        text: I18n.t("lifetime_" + modelData)
        radio: true
        marked: modelData === root.lifetimeKey
        onTriggered: root.lifetimeKey = modelData
      }
      onObjectAdded: function(index, object) { lifetimeMenu.insertItem(index, object) }
      onObjectRemoved: function(index, object) { lifetimeMenu.removeItem(object) }
    }
  }
  ActionMenu {
    id: kindMenu
    property int part: 0
    Instantiator {
      model: RuleModels.conditionKinds
      delegate: ActionMenuItem {
        required property string modelData
        text: root.kindTitle(modelData)
        detail: RuleModels.keyword(modelData, "")
        onTriggered: root.setPart(kindMenu.part, "kind", modelData)
      }
      onObjectAdded: function(index, object) { kindMenu.insertItem(index, object) }
      onObjectRemoved: function(index, object) { kindMenu.removeItem(object) }
    }
  }
}
