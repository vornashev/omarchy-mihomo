import QtQuick
import qs.Commons
import "Notices.js" as Notices

// One event, opened by clicking its desktop notification: what happened,
// the details behind it and what can be done. Notifications in Omarchy have
// no buttons, so every action lives here.
Item {
  id: root
  property var svc
  property var panel

  readonly property bool typing: false
  readonly property string key: panel.noticeKey
  readonly property var notice: svc.notices[key] || null
  readonly property string kind: notice ? notice.kind : ""
  readonly property var info: notice ? (notice.data || {}) : ({})
  // An autofix follows the live state, not the notice that announced it.
  readonly property var fix: kind === "autofix" ? (svc.fixes[info.host] || info) : null
  property bool busy: false
  property real now: Date.now()

  implicitHeight: col.implicitHeight

  Timer { interval: 30000; running: true; repeat: true; onTriggered: root.now = Date.now() }

  function call(method, params, done) {
    busy = true
    svc.request(method, params, function(r, err) {
      busy = false
      if (err) { panel.notify(err.message); return }
      if (r && r.host) svc.setFix(r)
      if (done) done(r)
    })
  }

  function textKey(t) { if (t === "q") panel.close() }
  function activate() { if (primaryBtn.visible && primaryBtn.enabled) primaryBtn.clicked() }

  readonly property var head: {
    var d = info
    switch (kind) {
    case "autofix":
      var f = fix || {}
      if (!f.state) return { icon: Icons.sparkles, color: Color.muted, title: f.host || "", sub: "" }
      if (f.state === "protected") return { icon: Icons.shield, color: Color.muted, title: f.host, sub: I18n.t("afSub_protected") }
      var at = f.state === "trial" ? f.applied_at : (f.state === "pinned" ? f.ends_at : (f.checked_at || f.updated))
      return { icon: Icons.sparkles, color: f.state === "rolled_back" ? svc.red : (f.state === "unreachable" ? svc.peach : (f.state === "found" ? Color.accent : svc.green)),
        title: f.host, sub: I18n.t("afSub_" + f.state).arg(svc.clock(at)) }
    case "core_down": return { icon: Icons.shieldAlert, color: svc.red, title: Notices.text(notice, I18n.t, svc.clock).title, sub: svc.clock(notice.at) }
    case "core_up": return { icon: Icons.statusCheck, color: svc.green, title: I18n.t("nCoreUp"), sub: svc.clock(notice.at) }
    case "dead_group": case "group_alive":
      return { icon: Icons.unplug, color: kind === "dead_group" ? svc.peach : svc.green, title: Notices.text(notice, I18n.t, svc.clock).title, sub: svc.clock(notice.at) }
    case "strict_down": return { icon: Icons.lock, color: svc.red, title: I18n.t("nStrictTitle").arg(d.group), sub: svc.clock(notice.at) }
    case "strict_up": return { icon: Icons.lockOpen, color: svc.green, title: I18n.t("nStrictTitleUp").arg(d.group), sub: svc.clock(notice.at) }
    case "subscription": return { icon: Icons.profiles, color: svc.yellow, title: Notices.text(notice, I18n.t, svc.clock).title, sub: svc.clock(notice.at) }
    case "orphans": return { icon: Icons.orphan, color: svc.yellow, title: Notices.text(notice, I18n.t, svc.clock).title, sub: d.profile || svc.clock(notice.at) }
    case "saved": return { icon: Icons.check, color: svc.green, title: I18n.t("nSaved"), sub: svc.clock(notice.at) }
    case "core_update": return { icon: Icons.down, color: Color.accent, title: Notices.text(notice, I18n.t, svc.clock).title, sub: svc.clock(notice.at) }
    case "traffic_spike": case "traffic_route": return { icon: Icons.traffic, color: svc.peach, title: Notices.text(notice, I18n.t, svc.clock).title, sub: svc.clock(notice.at) }
    }
    return { icon: Icons.info, color: Color.muted, title: I18n.t("nNoticeGone"), sub: "" }
  }

  // Colored strip: what the fix is doing now.
  readonly property var strip: {
    var f = fix
    if (kind === "strict_down") return { icon: Icons.shield, color: svc.yellow, title: I18n.t("nStrictStrip"), sub: I18n.t("nStrictStripSub").arg(info.group) }
    if (kind === "strict_up") return { icon: Icons.check, color: svc.green, title: info.node ? info.node + (info.delay ? " · " + info.delay + " " + I18n.t("ms") : "") : I18n.t("nStrictTitleUp").arg(info.group), sub: Notices.text(notice, I18n.t, svc.clock).body }
    if (!f) return null
    switch (f.state) {
    case "trial": return { icon: Icons.timer, color: svc.green, title: I18n.t("afStripTrial").arg(f.target).arg(Math.max(1, Math.round((f.ends_at - now) / 60000))), sub: I18n.t("afStripTrialSub").arg(f.trial_ok).arg(f.trial_failed) }
    case "pinned": return { icon: Icons.check, color: svc.green, title: f.rule || "", sub: I18n.t("afStripPinnedSub") }
    case "rolled_back":
      var failed = (f.tried || [])[(f.tried || []).length - 1] || ""
      return { icon: Icons.undo, color: svc.red, title: I18n.t("afStripRolled").arg(failed).arg(f.trial_failed), sub: I18n.t("afStripRolledSub") }
    case "unreachable": return { icon: Icons.timer, color: svc.peach, title: I18n.t("afStripUnreachable").arg(svc.clock(f.until)), sub: I18n.t("afStripUnreachableSub") }
    case "checking": return { icon: Icons.refresh, color: Color.muted, title: I18n.t("afStripChecking"), sub: "" }
    case "reverted": return { icon: Icons.check, color: svc.green, title: I18n.t("afStripReverted").arg(f.was_rule || "Match").arg(f.was), sub: I18n.t("afStripRevertedSub").arg(f.target) }
    case "protected":
      var pr = f.protect || {}
      return { icon: pr.reason === "strict" ? Icons.lock : Icons.shield, color: pr.reason === "strict" ? svc.yellow : Color.muted, title: I18n.t("afProtect_" + pr.reason).arg(pr.by), sub: pr.reason === "strict" ? I18n.t("afProtectSub_strict") : I18n.t("afProtectSub_explicit") }
    case "dismissed": return { icon: Icons.timer, color: Color.muted, title: I18n.t("afStripDismissed").arg(svc.clock(f.until)), sub: "" }
    }
    return null
  }

  // Working routes other than the one in use or proposed.
  readonly property var alternatives: {
    var f = fix
    if (!f || !f.results) return []
    return f.results.filter(function(o) { return o.ok && o.target !== f.target && o.target !== f.was })
  }

  Column {
    id: col
    width: parent.width
    spacing: Style.space(12)

    Row {
      width: parent.width
      spacing: Style.space(10)
      Btn { id: backBtn; icon: Icons.back; tooltip: I18n.t("afAllProblems"); anchors.verticalCenter: parent.verticalCenter; onClicked: root.panel.showPage("problems") }
      Txt { text: root.head.icon; color: root.head.color; font.pixelSize: Style.font.heading; anchors.verticalCenter: parent.verticalCenter }
      Column {
        width: parent.width - backBtn.width - closeBtn.width - Style.space(60)
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(2)
        Txt { width: parent.width; text: root.head.title || ""; bold: true; wrapMode: Text.Wrap; maximumLineCount: 2 }
        Txt { width: parent.width; text: root.head.sub || ""; dim: true; font.pixelSize: Style.font.caption }
      }
      Btn { id: closeBtn; ghost: true; icon: Icons.close; tooltip: "Esc"; anchors.verticalCenter: parent.verticalCenter; onClicked: root.panel.close() }
    }

    // ---------------------------------------------------------- autofix
    Txt {
      visible: !!root.fix && !!root.fix.was
      width: parent.width
      wrapMode: Text.Wrap
      dim: true
      font.pixelSize: Style.font.caption
      text: root.fix ? I18n.t("afFacts").arg(root.fix.failures).arg(root.svc.automation ? root.svc.automation.window_min : 5)
        .arg(I18n.kind(root.fix.kind)).arg((root.fix.apps || []).join(", ") || "—").arg(root.fix.was_rule || "Match").arg(root.fix.was) : ""
    }

    Rectangle {
      visible: root.strip !== null
      width: parent.width
      height: stripCol.implicitHeight + Style.space(14)
      radius: Style.cornerRadius
      color: root.strip ? Util.alpha(root.strip.color, 0.09) : "transparent"
      Row {
        anchors.fill: parent
        anchors.margins: Style.space(7)
        anchors.leftMargin: Style.space(10)
        spacing: Style.space(8)
        Txt { id: stripIcon; text: root.strip ? root.strip.icon : ""; color: root.strip ? root.strip.color : Color.muted; font.pixelSize: Style.font.icon }
        Column {
          id: stripCol
          width: parent.width - stripIcon.width - Style.space(8)
          spacing: Style.space(2)
          Txt { width: parent.width; text: root.strip ? root.strip.title : ""; bold: true; color: root.strip ? root.strip.color : Color.muted; wrapMode: Text.Wrap }
          Txt { visible: text !== ""; width: parent.width; text: root.strip ? root.strip.sub : ""; dim: true; font.pixelSize: Style.font.caption; wrapMode: Text.Wrap }
        }
      }
    }

    Column {
      visible: !!root.fix && (root.fix.results || []).length > 0
      width: parent.width
      spacing: Style.space(6)
      Txt { text: I18n.t("afCheck") + (root.fix && root.fix.checked_at ? "  " + root.svc.clock(root.fix.checked_at) : ""); faint: true; bold: true; font.pixelSize: Style.font.caption }
      Rectangle {
        width: parent.width
        height: resCol.implicitHeight + Style.space(14)
        radius: Style.cornerRadius
        color: Util.alpha(Color.popups.text, 0.04)
        Column {
          id: resCol
          anchors.fill: parent
          anchors.margins: Style.space(7)
          anchors.leftMargin: Style.space(10)
          anchors.rightMargin: Style.space(10)
          spacing: Style.space(4)
          Repeater {
            model: root.fix ? root.fix.results : []
            Row {
              required property var modelData
              readonly property bool best: root.fix && modelData.target === root.fix.target
              width: resCol.width
              spacing: Style.space(8)
              Txt { text: modelData.ok ? Icons.check : Icons.close; color: modelData.ok ? root.svc.green : root.svc.red; font.pixelSize: Style.font.caption }
              Txt {
                width: parent.width - Style.space(100)
                text: modelData.target + (modelData.target === root.fix.was ? " · " + I18n.t("afWas") : "")
                bold: parent.best; dim: !parent.best
              }
              Txt {
                width: Style.space(70); horizontalAlignment: Text.AlignRight
                text: modelData.ok ? modelData.delay + " " + I18n.t("ms") : (modelData.error === "timeout" ? I18n.t("timeout") : (modelData.error || ""))
                color: modelData.ok ? root.svc.green : root.svc.red; font.pixelSize: Style.font.caption
              }
            }
          }
        }
      }
    }

    Flow {
      visible: root.alternatives.length > 0 && !!root.fix && ["trial", "found", "rolled_back"].indexOf(root.fix.state) !== -1
      width: parent.width
      spacing: Style.space(6)
      Txt { text: I18n.t("afOrVia"); faint: true; font.pixelSize: Style.font.caption; height: Style.space(24) }
      Repeater {
        model: root.alternatives
        Btn {
          required property var modelData
          text: modelData.target + "  " + modelData.delay + " " + I18n.t("ms")
          enabled: !root.busy
          onClicked: root.call("autofix.apply", { host: root.fix.host, target: modelData.target })
        }
      }
    }

    // ---------------------------------------------------------- other kinds
    Column {
      visible: root.kind === "core_down" && (root.info.log || []).length > 0
      width: parent.width
      spacing: Style.space(6)
      Txt { text: I18n.t("nCoreLog"); faint: true; bold: true; font.pixelSize: Style.font.caption }
      Repeater {
        model: root.kind === "core_down" ? root.info.log : []
        Txt { required property string modelData; width: col.width; text: modelData; color: root.svc.red; font.pixelSize: Style.font.caption; wrapMode: Text.Wrap; maximumLineCount: 3 }
      }
    }

    Txt {
      visible: ["core_down", "core_up", "dead_group", "subscription", "orphans", "group_alive", "traffic_spike", "traffic_route", "core_update"].indexOf(root.kind) !== -1
      width: parent.width
      wrapMode: Text.Wrap
      dim: true
      font.pixelSize: Style.font.bodySmall
      text: root.notice ? (Notices.text(root.notice, I18n.t, root.svc.clock) || {}).body || "" : ""
    }

    Column {
      visible: root.kind === "strict_down" && (root.info.hosts || []).length > 0
      width: parent.width
      spacing: Style.space(6)
      Txt { text: I18n.t("nStrictHosts"); faint: true; bold: true; font.pixelSize: Style.font.caption }
      Rectangle {
        width: parent.width
        height: hostCol.implicitHeight + Style.space(14)
        radius: Style.cornerRadius
        color: Util.alpha(Color.popups.text, 0.04)
        Column {
          id: hostCol
          anchors.fill: parent
          anchors.margins: Style.space(7)
          anchors.leftMargin: Style.space(10)
          anchors.rightMargin: Style.space(10)
          spacing: Style.space(4)
          Repeater {
            model: root.kind === "strict_down" ? root.info.hosts : []
            Row {
              required property var modelData
              width: parent.width
              spacing: Style.space(8)
              Txt { text: Icons.close; color: root.svc.red; font.pixelSize: Style.font.caption }
              Txt { width: parent.width - Style.space(70); text: modelData.host + (modelData.app ? " · " + modelData.app : ""); dim: true }
              Txt { width: Style.space(40); horizontalAlignment: Text.AlignRight; text: "×" + modelData.count; color: root.svc.red; font.pixelSize: Style.font.caption }
            }
          }
        }
      }
    }

    Column {
      visible: root.kind === "orphans" || root.kind === "saved"
      width: parent.width
      spacing: Style.space(4)
      Repeater {
        model: root.kind === "orphans" ? root.info.rules : (root.kind === "saved" ? [{ line: root.info.line }] : [])
        Txt { required property var modelData; width: col.width; text: modelData.line; color: root.kind === "saved" ? root.svc.green : Color.popups.text; font.pixelSize: Style.font.bodySmall }
      }
    }

    // ---------------------------------------------------------- actions
    Row {
      spacing: Style.space(6)
      Btn {
        id: primaryBtn
        primary: true
        busy: root.busy
        visible: text !== ""
        readonly property var act: {
          var f = root.fix
          if (f) {
            if (f.state === "trial") return { text: I18n.t("afPin"), icon: Icons.check, run: function() { root.call("autofix.pin", { host: f.host }) } }
            if ((f.state === "found" || f.state === "rolled_back") && f.target) return { text: I18n.t("afApply").arg(f.target), icon: Icons.timer, run: function() { root.call("autofix.apply", { host: f.host }) } }
            if (f.state === "unreachable") return { text: I18n.t("afRecheck"), icon: Icons.refresh, run: function() { root.call("autofix.recheck", { host: f.host }) } }
            if (f.state === "reverted" && f.target) return { text: I18n.t("afKeep").arg(f.target), icon: Icons.check, run: function() { root.call("autofix.keep", { host: f.host, target: f.target }) } }
            return { text: "", run: null }
          }
          switch (root.kind) {
          case "core_down": return { text: I18n.t("nRestart"), icon: Icons.refresh, run: function() { root.call("core.restart", null) } }
          case "core_update":
            var update = (root.svc.issues || []).filter(function(i) { return i.id === "core_update" && i.fix })[0]
            if (!update) return { text: "", run: null }
            return { text: I18n.t("coreUpdateTo").arg(update.detail), icon: Icons.down, run: function() { root.svc.runFix(update.fix); root.panel.close() } }
          case "strict_down": return { text: I18n.t("nCheckNodes"), icon: Icons.refresh, run: function() { root.call("group.delay", { name: root.info.group }, function() { root.panel.notify(I18n.t("nCheckNodes") + " ✓") }) } }
          case "dead_group": return { text: I18n.t("afRecheck"), icon: Icons.refresh, run: function() { root.call("group.delay", { name: root.info.group }, function() { root.panel.notify(I18n.t("afRecheck") + " ✓") }) } }
          case "subscription": return { text: I18n.t("nRetry"), icon: Icons.refresh, run: function() { root.call("profiles.update", { id: root.info.id }, function(r) { if (r && r[0]) root.panel.notify(r[0].error || I18n.t("profileUpdated")) }) } }
          case "orphans":
            var s = (root.info.rules || []).filter(function(r) { return r.suggest })
            if (!s.length) return { text: "", run: null }
            return { text: I18n.t("nMoveTo").arg(s[0].suggest), icon: Icons.arrow, run: function() {
              s.forEach(function(r) { root.call("rules.update", { id: r.id, target: r.suggest }) })
              root.panel.showPage("rules")
            } }
          case "saved": return { text: I18n.t("afUndo"), icon: Icons.undo, run: function() { root.undoSaved() } }
          case "traffic_spike": case "traffic_route": return { text: I18n.t("nOpenTraffic"), icon: Icons.traffic, run: function() { root.panel.openTraffic(root.info.app) } }
          }
          return { text: "", run: null }
        }
        text: act.text
        icon: act.icon || ""
        onClicked: if (act.run) act.run()
      }
      Btn {
        readonly property var act: {
          var f = root.fix
          if (f) {
            if (f.state === "trial") return { text: I18n.t("afUndo"), icon: Icons.undo, run: function() { root.call("autofix.undo", { host: f.host }, function() { root.panel.notify(I18n.t("afUndone")) }) } }
            if (f.state === "found" || f.state === "rolled_back") return { text: I18n.t("afNotNow"), run: function() { root.call("autofix.dismiss", { host: f.host }, function() { root.panel.close() }) } }
            if (f.state === "unreachable") return { text: I18n.t("afQuiet"), run: function() { root.call("autofix.dismiss", { host: f.host }, function() { root.panel.close() }) } }
            if (f.state === "reverted") return { text: I18n.t("afOk"), run: function() { root.panel.close() } }
            if (f.state === "pinned") return { text: I18n.t("afDeleteRule"), icon: Icons.remove, run: function() { root.call("rules.delete", { id: f.rule_id }, function() { root.call("autofix.dismiss", { host: f.host }) ; root.panel.notify(I18n.t("afUndone")) }) } }
            return { text: "", run: null }
          }
          switch (root.kind) {
          case "core_down": case "core_up": return { text: I18n.t("nOpenLogs"), icon: Icons.logs, run: function() { root.panel.showPage("logs") } }
          case "dead_group": case "group_alive": case "strict_down": case "strict_up": return { text: I18n.t("nOpenProxies"), icon: Icons.proxies, run: function() { root.panel.showPage("proxies") } }
          case "core_update": return { text: I18n.t("nOpenCoreSettings"), icon: Icons.settings, run: function() { root.panel.settingsTab = "core"; root.panel.showPage("settings") } }
          case "subscription": return { text: I18n.t("nOpenProfiles"), icon: Icons.profiles, run: function() { root.panel.showPage("profiles") } }
          case "orphans": case "saved": return { text: I18n.t("afToRules"), icon: Icons.rules, run: function() { root.panel.showPage("rules") } }
          case "traffic_route": return { text: I18n.t("nRuleForApp").arg(root.info.app), icon: Icons.ruleAdd, run: function() {
            root.panel.openRule({ from: "traffic", kind: "process-name", value: root.info.app, origin: { host: "", ip: "", process: root.info.app, process_path: "", port: "", network: "", route: "", rule: "" } })
          } }
          }
          return { text: "", run: null }
        }
        visible: act.text !== ""
        ghost: (!!root.fix && root.fix.state !== "trial" && root.fix.state !== "pinned") || root.kind === "strict_down"
        enabled: !root.busy
        text: act.text
        icon: act.icon || ""
        onClicked: if (act.run) act.run()
      }
    }

    Txt {
      visible: root.kind === "strict_down" || root.kind === "strict_up"
      text: Icons.settings + " " + I18n.t("nStrictSettings"); dim: true; font.pixelSize: Style.font.caption
      MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: { root.panel.settingsTab = "auto"; root.panel.showPage("settings") } }
    }

    // Links: the full list, and never touching this host again.
    Rectangle {
      visible: !!root.fix
      width: parent.width
      height: 1
      color: Util.alpha(Color.popups.text, 0.12)
    }
    Row {
      visible: !!root.fix
      spacing: Style.space(14)
      Txt {
        text: Icons.arrow + " " + I18n.t("afInProblems"); dim: true; font.pixelSize: Style.font.caption
        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.panel.showPage("problems") }
      }
      Txt {
        visible: !!root.fix && root.fix.state !== "pinned"
        text: Icons.hide + " " + I18n.t("afExclude"); dim: true; font.pixelSize: Style.font.caption
        MouseArea {
          anchors.fill: parent; cursorShape: Qt.PointingHandCursor
          onClicked: root.call("autofix.exclude", { host: root.fix.host }, function() {
            var f = Object.assign({}, root.fix); f.state = "dismissed"; root.svc.setFix(f)
            root.panel.notify(I18n.t("afExcluded").arg(f.host)); root.panel.close()
          })
        }
      }
    }
  }

  // Undo the saved rule if nothing changed after it; else open the rules.
  function undoSaved() {
    var id = info.rule_id
    svc.request("rules.history", { limit: 1 }, function(h, err) {
      if (err || !h || !h.length || h[0].rule_id !== id) { panel.showPage("rules"); return }
      call("rules.rollback", { to: h[0].id - 1 }, function() { panel.notify(I18n.t("nUndoneSaved")); panel.close() })
    })
  }
}
