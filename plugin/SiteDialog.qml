import QtQuick
import qs.Commons
import "SiteModels.js" as SiteModels

// New site / edit site: address, name (the host when empty) and the icon:
// the site's favicon, an image link or a letter. Shows the card it makes.
ProxyDialog {
  id: root
  property var svc
  // The site being edited; null for a new one.
  property var site: null
  property string iconMode: "auto"
  readonly property string url: urlField.text.trim()
  readonly property bool valid: SiteModels.host(url) !== "" && url.indexOf(" ") < 0
  signal saveRequested(var form)

  function edit(s) {
    site = s
    urlField.text = s ? s.url : ""
    nameField.text = s ? s.name : ""
    iconMode = SiteModels.iconMode(s ? s.icon : null)
    iconField.text = iconMode === "url" ? s.icon : ""
    open()
    urlField.input.forceActiveFocus()
  }
  function submit() {
    if (!valid) return
    saveRequested({ id: site ? site.id : null, url: url, name: nameField.text.trim(), icon: SiteModels.iconValue(iconMode, iconField.text) })
  }

  title: site ? MihomoI18n.t("siteEditTitle") : MihomoI18n.t("siteNewTitle")
  onConfirmRequested: submit()

  Column {
    width: parent.width
    spacing: Style.space(6)
    Txt { text: MihomoI18n.t("siteUrl"); dim: true; bold: true; font.pixelSize: Style.font.body }
    Field {
      id: urlField
      objectName: "siteUrlField"
      width: parent.width
      icon: Icons.globe
      placeholder: "youtube.com"
      onAccepted: root.submit()
      onEscaped: root.close()
    }
    Txt { width: parent.width; text: MihomoI18n.t("siteUrlHint"); faint: true; font.pixelSize: Style.font.bodySmall; wrapMode: Text.Wrap }
  }
  Column {
    width: parent.width
    spacing: Style.space(6)
    Txt { text: MihomoI18n.t("siteName"); dim: true; bold: true; font.pixelSize: Style.font.body }
    Field {
      id: nameField
      objectName: "siteNameField"
      width: parent.width
      placeholder: SiteModels.defaultName(root.url) || MihomoI18n.t("siteNameHint")
      onAccepted: root.submit()
      onEscaped: root.close()
    }
  }
  Column {
    width: parent.width
    spacing: Style.space(6)
    Txt { text: MihomoI18n.t("siteIcon"); dim: true; bold: true; font.pixelSize: Style.font.body }
    Seg {
      objectName: "siteIconMode"
      model: [{ key: "auto", label: MihomoI18n.t("siteIconAuto") }, { key: "url", label: MihomoI18n.t("siteIconUrl") }, { key: "letter", label: MihomoI18n.t("siteIconLetter") }]
      current: root.iconMode
      onPicked: function(key) { root.iconMode = key; if (key === "url") iconField.input.forceActiveFocus() }
    }
    Field {
      id: iconField
      objectName: "siteIconField"
      visible: root.iconMode === "url"
      width: parent.width
      placeholder: MihomoI18n.t("siteIconUrlHint")
      onAccepted: root.submit()
      onEscaped: root.close()
    }
    Txt {
      width: parent.width
      text: MihomoI18n.t(root.iconMode === "auto" ? "siteIconAutoHint" : root.iconMode === "url" ? "siteIconUrlNote" : "siteIconLetterHint")
      faint: true; font.pixelSize: Style.font.bodySmall; wrapMode: Text.Wrap
    }
  }
  Column {
    width: parent.width
    spacing: Style.space(6)
    Txt { text: MihomoI18n.t("sitePreview"); dim: true; bold: true; font.pixelSize: Style.font.body }
    SiteCard {
      width: parent.width
      svc: root.svc
      site: ({ name: nameField.text.trim() || SiteModels.defaultName(root.url) || "—", url: root.url })
      iconFile: root.site && root.site.icon_file && root.iconMode === SiteModels.iconMode(root.site.icon) && root.url === root.site.url ? root.site.icon_file : ""
    }
  }
  Row {
    anchors.right: parent.right
    spacing: Style.space(8)
    Btn { text: MihomoI18n.t("cancel"); onClicked: root.close() }
    Btn { objectName: "siteSave"; primary: true; icon: Icons.check; text: MihomoI18n.t("siteSaveBtn"); enabled: root.valid; onClicked: root.submit() }
  }
}
