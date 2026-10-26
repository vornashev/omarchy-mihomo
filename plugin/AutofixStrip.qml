import QtQuick
import qs.Commons

// Autofix under a problem row: checking, a way found, a rule on trial,
// or "nothing works", with the same actions as the notification popup.
Rectangle {
  id: root
  property var fix: null
  property var svc
  property var panel
  property bool busy: false
  property real now: Date.now()

  readonly property string state_: fix ? fix.state : ""
  readonly property var best: {
    var f = fix
    if (!f || !f.results) return null
    return f.results.filter(function(o) { return o.target === f.target })[0] || null
  }
  readonly property var view: {
    var f = fix
    if (!f) return null
    switch (f.state) {
    case "checking": return { icon: Icons.refresh, color: Color.muted, title: I18n.t("afStripChecking"), sub: "" }
    case "found": return { icon: Icons.sparkles, color: Color.accent, title: I18n.t("afStripFound").arg(f.target).arg(best ? best.delay : 0),
      sub: (f.results || []).map(function(o) { return o.target + " " + (o.ok ? o.delay + " " + I18n.t("ms") : (o.error === "timeout" ? I18n.t("timeout") : (o.error || ""))) }).join(" · ") }
    case "trial": return { icon: Icons.timer, color: svc.green, title: I18n.t("afStripTrial").arg(f.target).arg(Math.max(1, Math.round((f.ends_at - now) / 60000))),
      sub: I18n.t("afStripTrialSub").arg(f.trial_ok).arg(f.trial_failed) }
    case "rolled_back":
      var failed = (f.tried || [])[(f.tried || []).length - 1] || ""
      return { icon: Icons.undo, color: svc.red, title: I18n.t("afStripRolled").arg(failed).arg(f.trial_failed), sub: I18n.t("afStripRolledSub") }
    case "protected":
      var pr = f.protect || {}
      return { icon: pr.reason === "strict" ? Icons.lock : Icons.shield, color: pr.reason === "strict" ? svc.yellow : Color.muted, title: I18n.t("afProtect_" + pr.reason).arg(pr.by),
        sub: pr.reason === "strict" ? I18n.t("afProtectSub_strict") : I18n.t("afProtectSub_explicit") }
    case "unreachable": return { icon: Icons.timer, color: svc.peach, title: I18n.t("afTitleUnreachable").arg(f.host), sub: I18n.t("afStripUnreachable").arg(svc.clock(f.until)) }
    }
    return null
  }

  visible: view !== null
  implicitHeight: visible ? Math.max(texts.implicitHeight, acts.implicitHeight) + Style.space(12) : 0
  radius: Style.cornerRadius
  color: view ? Util.alpha(view.color, 0.09) : "transparent"

  Timer { interval: 30000; running: root.state_ === "trial"; repeat: true; onTriggered: root.now = Date.now() }

  function call(method, params) {
    busy = true
    svc.request(method, params, function(r, err) {
      busy = false
      if (err) { panel.notify(err.message); return }
      if (r && r.host) svc.setFix(r)
    })
  }

  Txt {
    id: icon
    anchors.left: parent.left; anchors.leftMargin: Style.space(10)
    anchors.verticalCenter: parent.verticalCenter
    text: root.view ? root.view.icon : ""
    color: root.view ? root.view.color : Color.muted
    font.pixelSize: Style.font.icon
  }
  Column {
    id: texts
    anchors.left: icon.right; anchors.leftMargin: Style.space(8)
    anchors.right: acts.left; anchors.rightMargin: Style.space(8)
    anchors.verticalCenter: parent.verticalCenter
    spacing: Style.space(2)
    Txt { width: parent.width; text: root.view ? root.view.title : ""; bold: true; color: root.view ? root.view.color : Color.muted; font.pixelSize: Style.font.bodySmall }
    Txt { visible: text !== ""; width: parent.width; text: root.view ? root.view.sub : ""; dim: true; font.pixelSize: Style.font.caption }
  }
  Row {
    id: acts
    anchors.right: parent.right; anchors.rightMargin: Style.space(6)
    anchors.verticalCenter: parent.verticalCenter
    spacing: Style.space(4)
    Btn {
      visible: (root.state_ === "found" || root.state_ === "rolled_back") && !!root.fix.target
      primary: true; busy: root.busy; icon: Icons.timer
      text: root.fix && root.fix.target ? I18n.t("afApply").arg(root.fix.target) : ""
      onClicked: root.call("autofix.apply", { host: root.fix.host })
    }
    Btn {
      visible: root.state_ === "trial"
      busy: root.busy; icon: Icons.check; text: I18n.t("afPin")
      onClicked: root.call("autofix.pin", { host: root.fix.host })
    }
    Btn {
      visible: root.state_ === "trial"
      ghost: true; enabled: !root.busy; icon: Icons.undo; text: I18n.t("afUndo")
      onClicked: root.call("autofix.undo", { host: root.fix.host })
    }
    Btn {
      visible: root.state_ === "protected" && !!root.fix.protect && root.fix.protect.reason === "strict"
      ghost: true; icon: Icons.arrow; text: I18n.t("navProxies")
      onClicked: root.panel.showPage("proxies")
    }
    Btn {
      visible: root.state_ === "found" || root.state_ === "rolled_back"
      ghost: true; enabled: !root.busy; text: I18n.t("afNotNow")
      onClicked: root.call("autofix.dismiss", { host: root.fix.host })
    }
  }
}
