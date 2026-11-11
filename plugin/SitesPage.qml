import QtQuick
import QtQuick.Controls
import qs.Commons
import "SiteModels.js" as SiteModels

// Site checks · «Проверка»: a card per site; a click measures it through the
// route its traffic takes (the daemon asks the core through the mixed port).
Item {
  id: root
  property var svc
  property var panel

  property var sites: []
  property var results: ({})
  property var testing: ({})
  // Site id → cached icon file; ids asked for once per page.
  property var icons: ({})
  property var iconAsked: ({})
  property int cursor: 0
  property string sort: "custom"
  property bool loaded: false
  readonly property var shown: SiteModels.sorted(sites, results, sort)
  readonly property var current: shown[cursor] || null
  readonly property bool busy: Object.keys(testing).length > 0
  readonly property var counts: SiteModels.counts(sites, results)
  readonly property bool typing: dialog.visible

  function apply(r) {
    if (!r) return
    sites = r.sites || []
    results = r.results || {}
    var next = {}
    sites.forEach(function(s) { if (s.icon_file) next[s.id] = s.icon_file; else if (icons[s.id]) next[s.id] = icons[s.id] })
    icons = next
    cursor = Math.min(cursor, Math.max(0, sites.length - 1))
    loaded = true
    sites.forEach(fetchIcon)
  }
  function reload() { svc.request("sites.list", null, function(r, err) { if (err) panel.notify(err.message); else apply(r) }) }
  function reply(r, err) { if (err) panel.notify(err.message); else apply(r) }

  function fetchIcon(s) {
    if (s.icon === "letter" || icons[s.id] || iconAsked[s.id + "|" + (s.icon || "")]) return
    var asked = Object.assign({}, iconAsked); asked[s.id + "|" + (s.icon || "")] = true; iconAsked = asked
    svc.request("sites.icon", { id: s.id }, function(r) {
      if (!r || !r.file) return
      var next = Object.assign({}, icons); next[s.id] = r.file; icons = next
    })
  }

  function setTesting(id, on) { var t = Object.assign({}, testing); if (on) t[id] = true; else delete t[id]; testing = t }
  function check(s) {
    if (!s || testing[s.id]) return
    setTesting(s.id, true)
    svc.request("sites.test", { id: s.id }, function(r, err) {
      setTesting(s.id, false)
      if (err) { panel.notify(err.message); return }
      var next = Object.assign({}, results); next[s.id] = r; results = next
    })
  }
  function checkAll() { sites.forEach(check) }

  function save(form) {
    svc.request("sites.save", form, function(r, err) {
      if (err) { panel.notify(err.message); return }
      dialog.close()
      apply(r)
      var i = shown.findIndex(function(s) { return s.id === r.saved })
      if (i >= 0) cursor = i
      var s = sites.find(function(x) { return x.id === r.saved })
      if (s) check(s)
    })
  }
  function del(s) {
    if (!s) return
    svc.request("sites.remove", { id: s.id }, function(r, err) {
      reply(r, err)
      if (!err) panel.notify(MihomoI18n.t("siteRemoved").arg(s.name))
    })
  }
  function order(ids) { svc.request("sites.order", { ids: ids }, reply) }
  // Keyboard reordering works in the user's own order only.
  function shift(step) {
    if (sort !== "custom" || !current) return
    var id = current.id
    var ids = SiteModels.moved(sites, id, step)
    cursor = ids.indexOf(id)
    order(ids)
  }
  function dropAt(from, to) {
    if (sort !== "custom" || to < 0 || from === to || !shown[from]) return
    var ids = SiteModels.moved(sites, shown[from].id, to - from)
    cursor = to
    order(ids)
  }
  function addDefaults() { svc.request("sites.defaults", null, reply) }
  function add() { dialog.edit(null) }
  function edit(s) { if (s) dialog.edit(s) }
  function openSite(s) { if (s) Qt.openUrlExternally(s.url) }
  function copyUrl(s) {
    if (!s) return
    clipboard.text = s.url
    clipboard.selectAll(); clipboard.copy(); clipboard.deselect()
    panel.notify(MihomoI18n.t("copiedToast").arg(s.url))
  }
  function openMenu(s, anchor) {
    cursor = shown.indexOf(s)
    siteMenu.site = s
    if (anchor) siteMenu.popup(anchor, anchor.width - siteMenu.width, anchor.height + Style.space(4))
    else siteMenu.popup()
  }

  // Page hooks the panel's key catcher calls.
  function move(dx, dy) {
    if (shown.length === 0) return
    cursor = Math.max(0, Math.min(shown.length - 1, cursor + dx + dy * grid.columns))
    grid.positionViewAtIndex(cursor, GridView.Contain)
  }
  function activate() { check(current) }
  function remove() { del(current) }
  function textKey(t) {
    if (t === "t") checkAll()
    else if (t === "n" || t === "a") add()
    else if (t === "e") edit(current)
    else if (t === "o") openSite(current)
    else if (t === "m" && current) openMenu(current, null)
    else if (t === "[") shift(-1)
    else if (t === "]") shift(1)
  }

  Component.onCompleted: reload()
  TextEdit { id: clipboard; visible: false }

  Column {
    id: head
    width: parent.width
    spacing: Style.space(12)
    Row {
      width: parent.width
      spacing: Style.space(8)
      Column {
        width: parent.width - addBtn.width - allBtn.width - parent.spacing * 2
        spacing: Style.space(3)
        Txt { text: MihomoI18n.t("sitesTitle"); bold: true; font.pixelSize: Style.font.heading }
        Txt {
          objectName: "sitesSubtitle"
          width: parent.width
          elide: Text.ElideRight
          text: root.sites.length === 0 ? MihomoI18n.t("sitesEmptySubtitle")
            : [MihomoI18n.t("sitesCount").arg(root.sites.length), MihomoI18n.t("sitesHow")]
              .concat(SiteModels.lastChecked(root.results) ? [MihomoI18n.t("sitesChecked").arg(root.svc.ago(SiteModels.lastChecked(root.results)))] : []).join(" · ")
          dim: true; font.pixelSize: Style.font.body
        }
      }
      Btn { id: addBtn; objectName: "siteAdd"; icon: Icons.add; text: MihomoI18n.t("add"); keyHint: "n"; onClicked: root.add() }
      Btn {
        id: allBtn
        objectName: "sitesCheckAll"
        visible: root.sites.length > 0
        width: visible ? implicitWidth : 0
        primary: true; icon: Icons.test; text: MihomoI18n.t("sitesCheckAll"); keyHint: "t"
        busy: root.busy
        onClicked: root.checkAll()
      }
    }
    Item {
      visible: root.sites.length > 0
      width: parent.width
      height: visible ? sortSeg.implicitHeight : 0
      Seg {
        id: sortSeg
        objectName: "sitesSort"
        model: [{ key: "custom", label: MihomoI18n.t("sitesSortCustom") }, { key: "delay", label: MihomoI18n.t("sitesSortDelay") }, { key: "name", label: MihomoI18n.t("sitesSortName") }]
        current: root.sort
        onPicked: function(key) { root.sort = key; root.cursor = 0 }
      }
      Row {
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(14)
        Repeater {
          model: [{ n: root.counts.ok, c: root.svc.green, k: "sitesOk" }, { n: root.counts.slow, c: root.svc.yellow, k: "sitesSlow" }, { n: root.counts.fail, c: root.svc.red, k: "sitesFail" }]
          Row {
            required property var modelData
            visible: modelData.n > 0
            spacing: Style.space(6)
            Rectangle { anchors.verticalCenter: parent.verticalCenter; width: Style.space(6); height: width; radius: width / 2; color: parent.modelData.c }
            Txt { text: MihomoI18n.t(parent.modelData.k).arg(parent.modelData.n); dim: true; font.pixelSize: Style.font.bodySmall }
          }
        }
      }
    }
  }

  GridView {
    id: grid
    objectName: "sitesGrid"
    anchors.top: head.bottom; anchors.topMargin: Style.space(12)
    anchors.left: parent.left; anchors.right: parent.right
    anchors.bottom: keys.top; anchors.bottomMargin: Style.space(10)
    readonly property int columns: Math.max(1, Math.floor(width / Style.space(250)))
    readonly property int gap: Style.space(10)
    visible: root.sites.length > 0
    clip: true
    cellWidth: Math.floor(width / columns)
    cellHeight: Style.space(89) + gap
    model: root.shown
    interactive: contentHeight > height
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
    delegate: Item {
      id: cell
      required property var modelData
      required property int index
      width: grid.cellWidth
      height: grid.cellHeight
      z: drag.active ? 2 : 1
      SiteCard {
        id: card
        objectName: "siteCard"
        width: cell.width - grid.gap
        height: grid.cellHeight - grid.gap
        svc: root.svc
        site: cell.modelData
        result: root.results[cell.modelData.id] || null
        testing: !!root.testing[cell.modelData.id]
        iconFile: root.icons[cell.modelData.id] || ""
        selected: cell.index === root.cursor
        onPressedAt: root.cursor = cell.index
        onCheckRequested: root.check(cell.modelData)
        onMenuRequested: function(anchor) { root.openMenu(cell.modelData, anchor) }
        opacity: drag.active ? 0.85 : 1
      }
      DragHandler {
        id: drag
        target: card
        enabled: root.sort === "custom"
        onActiveChanged: {
          if (active) return
          var p = card.mapToItem(grid.contentItem, card.width / 2, card.height / 2)
          var to = grid.indexAt(p.x, p.y)
          card.x = 0; card.y = 0
          root.dropAt(cell.index, to)
        }
      }
    }
  }

  Column {
    objectName: "sitesEmpty"
    visible: root.loaded && root.sites.length === 0
    anchors.centerIn: parent
    width: Math.min(parent.width, Style.space(440))
    spacing: Style.space(10)
    Txt { anchors.horizontalCenter: parent.horizontalCenter; text: Icons.sites; faint: true; font.pixelSize: Style.space(40) }
    Txt { anchors.horizontalCenter: parent.horizontalCenter; text: MihomoI18n.t("sitesEmpty"); bold: true }
    Txt { width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; text: MihomoI18n.t("sitesEmptyHint"); dim: true; font.pixelSize: Style.font.body }
    Row {
      anchors.horizontalCenter: parent.horizontalCenter
      spacing: Style.space(8)
      Btn { objectName: "sitesDefaults"; primary: true; icon: Icons.sparkles; text: MihomoI18n.t("sitesDefaults"); onClicked: root.addDefaults() }
      Btn { icon: Icons.add; text: MihomoI18n.t("siteAddOne"); onClicked: root.add() }
    }
  }

  Row {
    id: keys
    anchors.left: parent.left; anchors.bottom: parent.bottom
    spacing: Style.space(14)
    visible: root.sites.length > 0
    height: visible ? implicitHeight : 0
    KeyHint { key: "←↑↓→"; label: MihomoI18n.t("keySelect") }
    KeyHint { key: "Enter"; label: MihomoI18n.t("keyCheck") }
    KeyHint { key: "t"; label: MihomoI18n.t("keyCheckAll") }
    KeyHint { key: "e"; label: MihomoI18n.t("keyEdit") }
    KeyHint { visible: root.sort === "custom"; key: "[ ]"; label: MihomoI18n.t("keyReorder") }
    KeyHint { key: "Del"; label: MihomoI18n.t("keyDelete") }
  }

  ActionMenu {
    id: siteMenu
    objectName: "siteMenu"
    property var site: null
    ActionMenuItem { glyph: Icons.test; text: MihomoI18n.t("siteCheck"); keyHint: "Enter"; onTriggered: root.check(siteMenu.site) }
    ActionMenuItem { glyph: Icons.edit; text: MihomoI18n.t("siteEdit"); keyHint: "e"; onTriggered: root.edit(siteMenu.site) }
    ActionMenuItem { glyph: Icons.globe; text: MihomoI18n.t("siteOpen"); keyHint: "o"; onTriggered: root.openSite(siteMenu.site) }
    ActionMenuItem { glyph: Icons.copy; text: MihomoI18n.t("siteCopy"); onTriggered: root.copyUrl(siteMenu.site) }
    ActionMenuSeparator {}
    ActionMenuItem { glyph: Icons.remove; text: MihomoI18n.t("siteDelete"); keyHint: "Del"; dangerous: true; onTriggered: root.del(siteMenu.site) }
  }

  SiteDialog {
    id: dialog
    svc: root.svc
    onSaveRequested: function(form) { root.save(form) }
    onClosed: root.panel.forceKeys()
  }
}
