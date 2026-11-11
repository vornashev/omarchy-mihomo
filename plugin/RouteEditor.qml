import QtQuick
import QtQuick.Controls
import qs.Commons
import qs.Commons as Commons
import "ProblemModels.js" as ProblemModels

// Choose how a resource is routed: scope (host / domain with subdomains /
// IP) and target (DIRECT or a group of the current profile). Nothing is
// saved until the user presses Save.
Item {
  id: root
  property var svc
  property var panel

  readonly property var problem: panel.editing && panel.editing.problem ? panel.editing.problem : null
  readonly property var rule: panel.editing && panel.editing.rule ? panel.editing.rule : null
  readonly property var group: panel.editing ? panel.editing.group || null : null
  readonly property var groupRules: group ? ProblemModels.groupScopes(group) : []

  property var scopes: []
  property int scopeIndex: 0
  property var targets: []
  property string target: ""
  property var results: ({})
  property var probe: null
  property bool testing: false
  property bool saving: false
  property string preview: ""
  property var existing: null
  property string error: ""
  property int cursor: 0
  readonly property bool typing: false
  // A sinkhole or a silent DNS server: a route cannot cure it, a server can.
  readonly property string dnsKind: {
    if (!problem || group || isIp(problem.host)) return ""
    var k = problem.kinds || {}
    return k.dns_stub ? "stub" : (k.dns_fail ? "fail" : "")
  }
  readonly property string dnsDomain: problem ? (problem.domain_hint || problem.name || problem.host) : ""
  function addDnsPolicy() {
    svc.request("dns.policy_add", { domain: dnsDomain }, function(r, err) {
      if (err) { panel.notify(err.message); return }
      var p = (r.settings.policy || []).filter(function(x) { return x.domain === "+." + root.dnsDomain })[0]
      panel.notify(MihomoI18n.t("dnsPolicyAdded").arg("+." + root.dnsDomain).arg(p ? p.server : r.doh))
    }, true)
  }

  readonly property var scope: scopes[scopeIndex] || null
  readonly property string currentTarget: problem && problem.route ? problem.route.target : (rule ? (rule.effective_target || rule.target) : "")

  function isIp(s) { return /^[0-9.]+$/.test(s) || s.indexOf(":") !== -1 }
  function parentDomain(h) {
    var parts = h.split(".")
    return parts.length > 2 ? parts.slice(1).join(".") : h
  }

  function buildScopes() {
    var out = []
    if (group) {
      var single = groupRules.length === 1 ? groupRules[0] : null
      out.push({ kind: "group", value: "", title: single ? single.value : MihomoI18n.t("problemAddresses").arg(group.items.length), sub: single ? MihomoI18n.matchKindLong(single.kind) : MihomoI18n.t("groupScopeHint") })
    } else if (rule) {
      out.push({ kind: rule.kind, value: rule.value, title: rule.value, sub: MihomoI18n.matchKindLong(rule.kind) })
    } else if (problem) {
      var h = problem.host
      if (isIp(h)) {
        if (problem.domain_hint) {
          out.push({ kind: "domain-suffix", value: problem.domain_hint, title: "*." + problem.domain_hint, sub: MihomoI18n.t("scopeSuffixHint") })
          out.push({ kind: "domain", value: problem.domain_hint, title: problem.domain_hint, sub: MihomoI18n.t("scopeExact") })
        }
        if (problem.name && problem.name !== problem.domain_hint)
          out.push({ kind: "domain", value: problem.name, title: problem.name, sub: MihomoI18n.t("scopeExact") })
        out.push({ kind: "ip-cidr", value: h, title: h, sub: MihomoI18n.t("scopeIp") })
      } else {
        out.push({ kind: "domain", value: h, title: h, sub: MihomoI18n.t("scopeExact") })
        out.push({ kind: "domain-suffix", value: parentDomain(h), title: "*." + parentDomain(h), sub: MihomoI18n.t("scopeSuffix") })
        scopeIndex = 1
      }
    }
    scopes = out
  }

  // Host to probe: the concrete resource, not the suffix.
  function testHost() {
    if (!scope) return ""
    if (group) return problem.name || problem.domain_hint || problem.host
    if (scope.kind === "ip-cidr") return scope.value.split("/")[0]
    if (problem && !isIp(problem.host)) return problem.host
    if (scope.kind === "domain-suffix" && problem && problem.name) return problem.name
    return scope.value
  }
  function testPort() {
    if (problem && problem.ports && problem.ports.length) return problem.ports[0]
    return 443
  }

  function loadTargets() {
    svc.request("proxies.list", null, function(groups, err) {
      if (err || !groups) { error = err ? err.message : ""; return }
      var list = [{ name: "DIRECT", kind: "Direct", now: "" }]
      for (var i = 0; i < groups.length; i++) {
        if (groups[i].name === "GLOBAL") continue
        list.push({ name: groups[i].name, kind: groups[i].kind, now: groups[i].now || "" })
      }
      if (rule && currentTarget && !list.some(function(t) { return t.name === currentTarget })) list.push({ name: currentTarget, kind: "Proxy", now: "" })
      targets = list
      if (target === "") {
        // Default to a different route than the failing one.
        target = rule ? currentTarget : (currentTarget === "DIRECT" && list.length > 1 ? list[1].name : "DIRECT")
      }
      cursor = Math.max(0, indexOfTarget(target))
      testAll()
    })
  }

  function indexOfTarget(name) {
    for (var i = 0; i < targets.length; i++) if (targets[i].name === name) return i
    return -1
  }

  function testAll() { runTest(targets.slice(0, 32).map(function(t) { return t.name })) }
  function testSelected() { if (target) runTest([target]) }

  function runTest(names) {
    var host = testHost()
    if (!host || names.length === 0) return
    testing = true
    svc.request("rules.test", { host: host, port: testPort(), targets: names }, function(r, err) {
      testing = false
      if (err || !r) { error = err ? err.message : ""; return }
      probe = r.probe
      var next = Object.assign({}, results)
      for (var i = 0; i < r.results.length; i++) next[r.results[i].target] = r.results[i]
      results = next
    })
  }

  function refreshPreview() {
    if (!scope || !target) return
    if (group) {
      preview = groupRules.map(function(r) { return MihomoI18n.matchKind(r.kind) + " " + r.value + " → " + target }).join("\n")
      existing = null
      return
    }
    svc.request("rules.preview", { kind: scope.kind, value: scope.value, target: target }, function(r, err) {
      if (err) { preview = ""; error = err.message; return }
      error = ""
      preview = r.line
      existing = r.existing
    })
  }

  function save() {
    if (!scope || !target || saving) return
    saving = true
    var res = results[target]
    var test = res ? { target: target, ok: !!res.ok, delay: res.delay || null, error: res.error || null, at: Date.now() } : null
    var params = { kind: scope.kind, value: scope.value, target: target, test: test }
    var done = function(r, err) {
      saving = false
      if (err) { error = err.message; return }
      var msg = group ? MihomoI18n.t("groupSaved").arg(group.items.length).arg(target) : MihomoI18n.t("savedToast").arg(r.rule ? r.rule.value : scope.value).arg(target)
      if (r.closed > 0) msg += " · " + MihomoI18n.t("closedConns").arg(r.closed)
      if (r.apply && !r.apply.reloaded) msg += " · " + MihomoI18n.t("notReloaded")
      panel.notify(msg)
      panel.showPage(rule ? "rules" : "problems")
    }
    if (group) svc.request("rules.save_many", { rules: groupRules, target: target }, done)
    else if (rule && rule.kind === scope.kind && rule.value === scope.value) svc.request("rules.update", { id: rule.id, target: target === rule.effective_target ? rule.target : target, enabled: true, test: test }, done)
    else svc.request("rules.save", params, done)
  }

  function move(dx, dy) {
    if (dx !== 0) { scopeIndex = Math.max(0, Math.min(scopes.length - 1, scopeIndex + dx)); return }
    cursor = Math.max(0, Math.min(targets.length - 1, cursor + dy))
    target = targets[cursor].name
    targetList.positionViewAtIndex(cursor, ListView.Contain)
  }
  function activate() { save() }
  function textKey(t) { if (t === "t") testSelected(); else if (t === "T") testAll() }

  onScopeChanged: refreshPreview()
  onTargetChanged: refreshPreview()
  Component.onCompleted: { buildScopes(); loadTargets() }

  Column {
    id: top
    anchors.left: parent.left
    anchors.right: parent.right
    spacing: Style.space(12)

    Row {
      spacing: Style.space(12)
      Btn { icon: Icons.back; tooltip: MihomoI18n.t("hintBack"); anchors.verticalCenter: parent.verticalCenter; onClicked: root.panel.showPage(root.rule ? "rules" : "problems") }
      Column {
        spacing: Style.space(3)
        Txt { text: root.group ? MihomoI18n.t("groupEditorTitle") : root.rule ? MihomoI18n.t("editRuleTitle") : MihomoI18n.t("editorTitle"); bold: true; font.pixelSize: Style.font.heading }
        Txt {
          text: (root.rule ? MihomoI18n.t("navRules") : MihomoI18n.t("navProblems")) + " › " + (root.group ? MihomoI18n.t("problemAddresses").arg(root.group.items.length) : root.problem ? root.problem.host : (root.rule ? root.rule.value : ""))
          dim: true; font.pixelSize: Style.font.body
        }
      }
    }

    // What we know about the failures.
    Rectangle {
      visible: root.problem !== null && root.group === null
      width: parent.width
      height: facts.implicitHeight + Style.space(20)
      radius: Style.cornerRadius
      color: Util.alpha(Commons.Color.popups.text, 0.04)
      border.width: 1
      border.color: Util.alpha(Commons.Color.popups.text, 0.12)
      Row {
        id: facts
        anchors.fill: parent
        anchors.margins: Style.space(10)
        spacing: Style.space(10)
        Repeater {
          model: root.problem ? [
            [MihomoI18n.t("factErrors"), Object.keys(root.problem.kinds).map(function(k) { return MihomoI18n.kind(k) + " ×" + root.problem.kinds[k] }).join("\n")],
            [MihomoI18n.t("factApps"), Object.keys(root.problem.apps).join("\n") || MihomoI18n.t("unknownApp")],
            [MihomoI18n.t("factRoute"), root.problem.route.target + (root.problem.route.rule ? "\n" + root.problem.route.rule + (root.problem.route.payload ? " · " + root.problem.route.payload : "") : "")],
            [MihomoI18n.t("factPeriod"), root.svc.clock(root.problem.first_seen) + " — " + root.svc.clock(root.problem.last_seen) + "\n" + MihomoI18n.t("repeats").arg(root.problem.count)]
          ] : []
          Column {
            required property var modelData
            width: (facts.width - facts.spacing * 3) / 4
            spacing: Style.space(4)
            Txt { text: modelData[0]; faint: true; font.pixelSize: Style.font.bodySmall }
            Txt { width: parent.width; text: modelData[1]; wrapMode: Text.Wrap; maximumLineCount: 3; font.pixelSize: Style.font.body }
          }
        }
      }
    }

    Banner {
      objectName: "dnsBanner"
      visible: root.dnsKind !== ""
      width: parent.width
      tone: "warn"
      toneColor: root.svc.yellow
      text: root.dnsKind === "stub" ? MihomoI18n.t("dnsStubTitle") + ". " + MihomoI18n.t("dnsStubBody") : MihomoI18n.t("dnsFailTitle") + ". " + MihomoI18n.t("dnsFailBody")
      action: MihomoI18n.t("dnsUseDoh").arg(root.dnsDomain)
      onActionClicked: root.addDnsPolicy()
    }

    Banner {
      visible: root.group !== null
      width: parent.width
      text: MihomoI18n.t("groupProbeHint").arg(root.testHost())
    }

    Flickable {
      visible: root.group !== null
      width: parent.width
      height: visible ? Math.min(Style.space(72), groupHosts.implicitHeight) : 0
      contentHeight: groupHosts.implicitHeight
      clip: true
      Txt { id: groupHosts; width: parent.width; text: root.group ? root.group.items.map(function(p) { return p.host + (p.name || p.domain_hint ? " (" + (p.name || p.domain_hint) + ")" : "") }).join(", ") : ""; wrapMode: Text.Wrap; font.pixelSize: Style.font.body; dim: true }
      ScrollBar.vertical: ScrollBar {}
    }

    Txt { text: MihomoI18n.t("scopeLabel"); faint: true; bold: true; font.pixelSize: Style.font.bodySmall; font.letterSpacing: 1 }
    Row {
      width: parent.width
      spacing: Style.space(8)
      Repeater {
        model: root.scopes
        Rectangle {
          required property var modelData
          required property int index
          readonly property bool on: index === root.scopeIndex
          width: (parent.width - Style.space(8) * (root.scopes.length - 1)) / Math.max(1, root.scopes.length)
          height: Style.space(58)
          radius: Style.cornerRadius
          color: on ? Style.selectedFillFor(Commons.Color.popups.text, Commons.Color.accent) : Util.alpha(Commons.Color.popups.text, 0.04)
          border.width: 1
          border.color: on ? Commons.Color.accent : Util.alpha(Commons.Color.popups.text, 0.15)
          Rectangle {
            id: radio
            anchors.left: parent.left; anchors.leftMargin: Style.space(12)
            anchors.verticalCenter: parent.verticalCenter
            width: Style.space(14); height: width; radius: width / 2
            color: "transparent"
            border.width: 1
            border.color: parent.on ? Commons.Color.accent : Util.alpha(Commons.Color.popups.text, 0.4)
            Rectangle { visible: parent.parent.on; anchors.centerIn: parent; width: Style.space(6); height: width; radius: width / 2; color: Commons.Color.accent }
          }
          Column {
            anchors.left: radio.right; anchors.leftMargin: Style.space(10)
            anchors.right: parent.right; anchors.rightMargin: Style.space(10)
            anchors.verticalCenter: parent.verticalCenter
            spacing: Style.space(2)
            Txt { width: parent.width; text: modelData.title; bold: true }
            Txt { width: parent.width; text: modelData.sub; dim: true; font.pixelSize: Style.font.bodySmall }
          }
          MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.scopeIndex = index }
        }
      }
    }

    Row {
      spacing: Style.space(8)
      Txt { text: MihomoI18n.t("targetLabel"); faint: true; bold: true; font.pixelSize: Style.font.bodySmall; font.letterSpacing: 1 }
      Txt { text: MihomoI18n.t("targetHint"); faint: true; font.pixelSize: Style.font.bodySmall }
    }
  }

  ListView {
    id: targetList
    anchors.top: top.bottom
    anchors.topMargin: Style.space(6)
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: bottom.top
    anchors.bottomMargin: Style.space(10)
    clip: true
    spacing: Style.space(2)
    model: root.targets
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
    delegate: Rectangle {
      required property var modelData
      required property int index
      readonly property bool on: modelData.name === root.target
      readonly property var res: root.results[modelData.name]
      width: targetList.width - Style.space(6)
      height: Style.space(32)
      radius: Style.cornerRadius
      color: on ? Style.selectedFillFor(Commons.Color.popups.text, Commons.Color.accent) : (ma.containsMouse ? Util.alpha(Commons.Color.popups.text, 0.04) : "transparent")
      Row {
        anchors.left: parent.left; anchors.leftMargin: Style.space(12)
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(10)
        Txt { text: modelData.name === "DIRECT" ? Icons.arrow : Icons.proxies; color: parent.parent.on ? Commons.Color.accent : Util.alpha(Commons.Color.popups.text, 0.6) }
        Txt { text: modelData.name; bold: parent.parent.on }
        Txt { text: modelData.kind + (modelData.now ? " · " + modelData.now : ""); faint: true; font.pixelSize: Style.font.bodySmall }
        Chip { visible: modelData.name === root.currentTarget; showDot: false; text: MihomoI18n.t("current") }
      }
      Row {
        anchors.right: parent.right; anchors.rightMargin: Style.space(12)
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(8)
        Txt {
          text: !res ? (root.testing ? "…" : "—") : (res.ok ? res.delay + " " + MihomoI18n.t("ms") : (res.error === "timeout" ? MihomoI18n.t("timeout") : MihomoI18n.t("failed")))
          color: !res ? Util.alpha(Commons.Color.popups.text, 0.42) : (res.ok ? (res.delay < 400 ? root.svc.green : root.svc.yellow) : root.svc.red)
          font.pixelSize: Style.font.body
        }
        Txt { visible: parent.parent.on; text: Icons.check; color: Commons.Color.accent }
      }
      MouseArea { id: ma; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: { root.target = modelData.name; root.cursor = index } }
    }
  }

  Column {
    id: bottom
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    spacing: Style.space(10)

    Banner {
      readonly property var res: root.results[root.target]
      visible: root.error !== "" || res !== undefined
      width: parent.width
      tone: root.error !== "" ? "error" : (res && res.ok ? "ok" : "warn")
      toneColor: tone === "ok" ? root.svc.green : (tone === "warn" ? root.svc.yellow : root.svc.red)
      text: {
        if (root.error !== "") return root.error
        if (!res) return ""
        var url = root.probe ? root.probe.url : root.testHost()
        var line = res.ok ? MihomoI18n.t("checkOk").arg(url).arg(root.target).arg(res.delay) : MihomoI18n.t("checkFail").arg(url).arg(root.target).arg(res.error || "")
        if (!res.ok && root.probe && root.probe.inconclusive) line += " " + MihomoI18n.t("inconclusive")
        return line
      }
    }

    Flickable {
      visible: root.preview !== ""
      width: parent.width
      height: Math.min(Style.space(60), previewText.implicitHeight)
      contentHeight: previewText.implicitHeight
      clip: true
      Txt {
        id: previewText
        width: parent.width
        text: root.preview + "   " + (root.existing && !root.rule ? MihomoI18n.t("replacesExisting").arg(root.existing.target) : MihomoI18n.t("abovePreview"))
        dim: true
        font.pixelSize: Style.font.bodySmall
      }
      ScrollBar.vertical: ScrollBar {}
    }

    Row {
      width: parent.width
      spacing: Style.space(8)
      layoutDirection: Qt.RightToLeft
      Btn { primary: true; icon: Icons.check; text: MihomoI18n.t("saveApply"); busy: root.saving; enabled: root.scope !== null && root.target !== ""; onClicked: root.save() }
      Btn { icon: Icons.test; text: MihomoI18n.t("check"); busy: root.testing; onClicked: root.testSelected() }
      Btn { text: MihomoI18n.t("cancel"); onClicked: root.panel.showPage(root.rule ? "rules" : "problems") }
      Txt {
        width: parent.width - Style.space(400)
        anchors.verticalCenter: parent.verticalCenter
        text: MihomoI18n.t("manualOnly")
        faint: true; font.pixelSize: Style.font.bodySmall; horizontalAlignment: Text.AlignLeft
      }
    }
  }
}
