import QtQuick
import QtQuick.Controls
import qs.Commons
import qs.Commons as Commons

// Settings → DNS: the user's values on top of the profile's, per-site
// servers and a lookup through the core.
Item {
  id: root
  property var svc
  property var panel
  // {settings, effective: {dns, sniffer}, doh} from dns.get.
  property var info: null
  property var answer: null
  property bool querying: false
  readonly property var s: info ? info.settings : null
  readonly property var dns: info && info.effective && info.effective.dns ? info.effective.dns : ({})
  readonly property bool typing: [serversField, fallbackField, bootstrapField, filterField, policyDomain, policyServer, queryField].some(function(f) { return f.input.activeFocus })

  function reload() { svc.request("dns.get", null, function(r) { if (r) root.info = r }) }
  function save(key, value) {
    var next = JSON.parse(JSON.stringify(s || {}))
    next[key] = value
    svc.request("dns.set", next, function(r, err) {
      if (r) root.info = r
      root.panel.notify(err ? err.message : MihomoI18n.t("settingsSaved"))
    }, true)
  }
  // "a, b c" → ["a", "b", "c"]; empty → null, back to the profile.
  function listOf(text) {
    var items = text.split(/[\s,]+/).filter(function(x) { return x !== "" })
    return items.length ? items : null
  }
  function joined(list) { return (list || []).join(", ") }
  function query() {
    var name = queryField.text.trim()
    if (!name) return
    querying = true
    svc.request("dns.query", { name: name, type: queryType.current }, function(r, err) {
      root.querying = false
      root.answer = err ? { error: err.message } : r
    }, true)
  }
  function answerText(a) {
    if (!a) return ""
    if (a.error) return a.error
    var list = (a.Answer || []).map(function(x) { return x.data })
    if (list.length) return list.join("\n")
    return a.Status === 3 ? MihomoI18n.t("dnsNxDomain") : MihomoI18n.t("dnsNoAnswer").arg(a.Status !== undefined ? a.Status : "?")
  }

  Component.onCompleted: reload()
  onVisibleChanged: if (visible) reload()
  onSChanged: if (s) {
    serversField.text = joined(s.nameserver)
    fallbackField.text = joined(s.fallback)
    bootstrapField.text = joined(s["default-nameserver"])
    filterField.text = joined(s["fake-ip-filter"])
  }

  Flickable {
    anchors.fill: parent
    contentHeight: body.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar { policy: body.implicitHeight > height ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff }

    Column {
      id: body
      width: parent.width - Style.space(12)
      spacing: Style.space(14)
      visible: root.s !== null

      Txt { width: parent.width; text: MihomoI18n.t("dnsIntro"); dim: true; font.pixelSize: Style.font.bodySmall; wrapMode: Text.Wrap }

      Row {
        spacing: Style.space(16)
        Txt { text: MihomoI18n.t("dnsMode"); width: Style.space(170); anchors.verticalCenter: parent.verticalCenter }
        Seg {
          objectName: "dnsModeSeg"
          current: root.s && root.s["enhanced-mode"] ? root.s["enhanced-mode"] : ""
          model: [{ key: "", label: MihomoI18n.t("dnsFromProfile") }, { key: "fake-ip", label: "fake-ip" }, { key: "redir-host", label: "redir-host" }]
          onPicked: function(k) { root.save("enhanced-mode", k || null) }
        }
        Txt { text: MihomoI18n.t("dnsNow").arg(root.dns["enhanced-mode"] || "—"); faint: true; font.pixelSize: Style.font.bodySmall; anchors.verticalCenter: parent.verticalCenter }
      }

      Row {
        spacing: Style.space(16)
        Txt { text: MihomoI18n.t("dnsSniffer"); width: Style.space(170); anchors.verticalCenter: parent.verticalCenter }
        Seg {
          current: !root.s || root.s.sniffer === null || root.s.sniffer === undefined ? "" : (root.s.sniffer ? "on" : "off")
          model: [{ key: "", label: MihomoI18n.t("dnsFromProfile") }, { key: "on", label: MihomoI18n.t("on") }, { key: "off", label: MihomoI18n.t("off") }]
          onPicked: function(k) { root.save("sniffer", k === "" ? null : k === "on") }
        }
        Txt { text: MihomoI18n.t("dnsNow").arg(root.info && root.info.effective.sniffer ? MihomoI18n.t("on") : MihomoI18n.t("off")); faint: true; font.pixelSize: Style.font.bodySmall; anchors.verticalCenter: parent.verticalCenter }
      }

      // One row per list setting: empty means "as in the profile".
      Column {
        width: parent.width
        spacing: Style.space(10)
        DnsListRow { id: serversField; objectName: "dnsServers"; width: parent.width; label: MihomoI18n.t("dnsServers"); hint: MihomoI18n.t("dnsListHint"); profile: root.joined(root.dns.nameserver); onSubmit: function(v) { root.save("nameserver", root.listOf(v)) } }
        DnsListRow { id: fallbackField; width: parent.width; label: MihomoI18n.t("dnsFallback"); hint: MihomoI18n.t("dnsListHint"); profile: root.joined(root.dns.fallback); onSubmit: function(v) { root.save("fallback", root.listOf(v)) } }
        DnsListRow { id: bootstrapField; width: parent.width; label: MihomoI18n.t("dnsBootstrap"); hint: MihomoI18n.t("dnsBootstrapHint"); profile: root.joined(root.dns["default-nameserver"]); onSubmit: function(v) { root.save("default-nameserver", root.listOf(v)) } }
        DnsListRow { id: filterField; width: parent.width; label: MihomoI18n.t("dnsFakeIpFilter"); hint: MihomoI18n.t("dnsFilterHint").arg((root.dns["fake-ip-filter"] || []).length); profile: ""; onSubmit: function(v) { root.save("fake-ip-filter", root.listOf(v) || []) } }
      }

      Rectangle { width: parent.width; height: 1; color: Util.alpha(Commons.Color.popups.text, 0.12) }

      Column {
        width: parent.width
        spacing: Style.space(8)
        Txt { text: MihomoI18n.t("dnsPolicy"); bold: true }
        Txt { width: parent.width; text: MihomoI18n.t("dnsPolicyHint"); dim: true; font.pixelSize: Style.font.bodySmall; wrapMode: Text.Wrap }
        Repeater {
          model: root.s ? root.s.policy : []
          Row {
            required property var modelData
            required property int index
            width: parent.width
            spacing: Style.space(10)
            Txt { width: Style.space(220); text: modelData.domain; anchors.verticalCenter: parent.verticalCenter }
            Txt { width: parent.width - Style.space(290); text: "→ " + modelData.server; dim: true; anchors.verticalCenter: parent.verticalCenter }
            Btn {
              anchors.verticalCenter: parent.verticalCenter
              objectName: "dnsPolicyRemove" + index
              ghost: true; icon: Icons.close; tooltip: MihomoI18n.t("dnsRemove")
              onClicked: { var list = root.s.policy.slice(); list.splice(index, 1); root.save("policy", list) }
            }
          }
        }
        Row {
          spacing: Style.space(8)
          Field { id: policyDomain; objectName: "dnsPolicyDomain"; width: Style.space(220); placeholder: "+.example.org" }
          Field { id: policyServer; width: Style.space(300); placeholder: root.info ? root.info.doh : "" }
          Btn {
            objectName: "dnsPolicyAdd"
            icon: Icons.add
            text: MihomoI18n.t("add")
            enabled: policyDomain.text.trim() !== ""
            onClicked: root.svc.request("dns.policy_add", { domain: policyDomain.text, server: policyServer.text }, function(r, err) {
              if (r) { root.info = r; policyDomain.text = ""; policyServer.text = "" }
              root.panel.notify(err ? err.message : MihomoI18n.t("settingsSaved"))
            }, true)
          }
        }
      }

      Rectangle { width: parent.width; height: 1; color: Util.alpha(Commons.Color.popups.text, 0.12) }

      Column {
        width: parent.width
        spacing: Style.space(8)
        Txt { text: MihomoI18n.t("dnsLookup"); bold: true }
        Row {
          spacing: Style.space(8)
          Field { id: queryField; objectName: "dnsQueryField"; width: Style.space(260); placeholder: "chatgpt.com"; onAccepted: root.query() }
          Seg { id: queryType; current: "A"; model: [{ key: "A", label: "A" }, { key: "AAAA", label: "AAAA" }]; onPicked: function(k) { current = k } }
          Btn { objectName: "dnsQueryBtn"; text: MihomoI18n.t("dnsLookupRun"); icon: Icons.search; busy: root.querying; enabled: queryField.text.trim() !== ""; onClicked: root.query() }
        }
        Txt {
          objectName: "dnsAnswer"
          visible: root.answer !== null
          width: parent.width
          text: root.answerText(root.answer)
          color: root.answer && root.answer.error ? root.svc.red : Commons.Color.popups.text
          font.pixelSize: Style.font.body
          wrapMode: Text.Wrap
        }
      }
    }
  }
}
