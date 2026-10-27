import QtQuick
import QtQuick.Controls
import qs.Commons

// Rule sets for RULE-SET rules: the user's own (added from the MetaCubeX
// catalog or by URL) and the profile's, with their size and freshness.
Item {
  id: root
  property var svc
  property var panel
  property var sets: null
  property string busy: ""
  readonly property var mine: sets && sets.mine ? sets.mine : []
  readonly property var profile: sets && sets.profile ? sets.profile : []

  function reload() { svc.request("rule_sets.list", null, function(r) { if (r) root.sets = r }) }
  function act(method, params, key, done) {
    busy = key
    svc.request(method, params, function(r, err) {
      root.busy = ""
      if (err) { root.panel.notify(err.message); return }
      root.sets = r.list || r
      if (done) root.panel.notify(done(r))
    }, true)
  }
  // "geosite:openai" goes to the catalog, anything else must be a URL.
  function add() {
    var text = input.text.trim()
    if (!text) return
    if (/^https?:\/\//.test(text)) {
      var name = text.split("/").pop().replace(/\.[a-z]+$/, "").replace(/[^A-Za-z0-9._-]/g, "-")
      var format = /\.mrs$/.test(text) ? "mrs" : (/\.ya?ml$/.test(text) ? "yaml" : "text")
      act("rule_sets.add", { name: name, url: text, behavior: behavior.current, format: format }, "add", function(r) { input.text = ""; return I18n.t("ruleSetsAdded").arg(r.name) })
    } else {
      act("rule_sets.add", { catalog: text }, "add", function(r) { input.text = ""; return I18n.t("ruleSetsAdded").arg(r.name) })
    }
  }
  function describe(rt) {
    if (!rt) return I18n.t("ruleSetsNotLoaded")
    var parts = [I18n.t("ruleSetsRules").arg(rt.rule_count)]
    if (rt.updated_at && rt.updated_at.indexOf("0001") !== 0) parts.push(I18n.t("ruleSetsUpdated").arg(Qt.formatDateTime(new Date(rt.updated_at), "dd.MM HH:mm")))
    return parts.join(" · ")
  }

  Component.onCompleted: reload()
  onVisibleChanged: if (visible) reload()

  Flickable {
    anchors.fill: parent
    contentHeight: body.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

    Column {
      id: body
      width: parent.width - Style.space(8)
      spacing: Style.space(10)

      Txt { width: parent.width; text: I18n.t("ruleSetsHint"); dim: true; font.pixelSize: Style.font.bodySmall; wrapMode: Text.Wrap }

      Row {
        width: parent.width
        spacing: Style.space(8)
        Field {
          id: input
          objectName: "ruleSetsInput"
          width: parent.width - behavior.width - addBtn.width - Style.space(16)
          placeholder: I18n.t("ruleSetsCatalog")
          onAccepted: root.add()
        }
        Seg {
          id: behavior
          visible: /^https?:\/\//.test(input.text.trim())
          width: visible ? implicitWidth : 0
          current: "domain"
          model: [{ key: "domain", label: "domain" }, { key: "ipcidr", label: "ipcidr" }, { key: "classical", label: "classical" }]
          onPicked: function(k) { current = k }
        }
        Btn { id: addBtn; objectName: "ruleSetsAdd"; icon: Icons.add; text: I18n.t("ruleSetsAdd"); busy: root.busy === "add"; enabled: input.text.trim() !== "" && root.busy === ""; onClicked: root.add() }
      }

      Txt { text: I18n.t("ruleSetsMine"); faint: true; bold: true; font.pixelSize: Style.font.caption }
      Txt { visible: root.sets !== null && root.mine.length === 0; width: parent.width; text: I18n.t("ruleSetsEmpty"); dim: true; font.pixelSize: Style.font.bodySmall; wrapMode: Text.Wrap }
      Repeater {
        model: root.mine
        Rectangle {
          required property var modelData
          required property int index
          width: body.width
          height: Style.space(44)
          radius: Style.cornerRadius
          color: Util.alpha(Color.popups.text, 0.04)
          Column {
            anchors.left: parent.left; anchors.leftMargin: Style.space(10)
            anchors.right: actions.left; anchors.rightMargin: Style.space(8)
            anchors.verticalCenter: parent.verticalCenter
            spacing: Style.space(2)
            Txt { width: parent.width; text: modelData.name + "  ·  " + modelData.behavior + (modelData.proxy ? "  ·  " + modelData.proxy : "") }
            Txt { width: parent.width; text: root.describe(modelData.runtime) + "  ·  " + modelData.url; faint: true; font.pixelSize: Style.font.caption }
          }
          Row {
            id: actions
            anchors.right: parent.right; anchors.rightMargin: Style.space(6)
            anchors.verticalCenter: parent.verticalCenter
            spacing: Style.space(4)
            Btn { ghost: true; icon: Icons.refresh; tooltip: I18n.t("ruleSetsRefresh"); busy: root.busy === "refresh" + index; onClicked: root.act("rule_sets.refresh", { name: modelData.name }, "refresh" + index) }
            Btn {
              objectName: "ruleSetRemove" + index
              ghost: true; icon: Icons.close; tooltip: I18n.t("ruleSetsRemove")
              onClicked: { var n = modelData.name; root.act("rule_sets.remove", { name: n }, "remove" + index, function() { return I18n.t("ruleSetsRemoved").arg(n) }) }
            }
          }
        }
      }

      Txt { visible: root.profile.length > 0; text: I18n.t("ruleSetsProfile"); faint: true; bold: true; font.pixelSize: Style.font.caption }
      Repeater {
        model: root.profile
        Row {
          required property var modelData
          width: body.width
          spacing: Style.space(10)
          Txt { width: Style.space(200); text: modelData.name; dim: true }
          Txt { width: Style.space(80); text: modelData.behavior; faint: true; font.pixelSize: Style.font.caption }
          Txt { width: parent.width - Style.space(300); text: root.describe(modelData); faint: true; font.pixelSize: Style.font.caption }
        }
      }
    }
  }
}
