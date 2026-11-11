import QtQuick
import qs.Commons
import qs.Commons as Commons
import "ProxyModels.js" as Models

// One node in the proxies list: name and server, protocol, delay with its
// history, groups and source. A right click asks the page for its menu.
Rectangle {
  id: row
  objectName: "proxyNodeRow"
  // The proxies page: selection, testing state, latency history, actions.
  property var page
  required property var item
  required property int index
  signal menuRequested
  height: Style.space(54)
  radius: Style.cornerRadius
  color: row.page.selected[row.item.name] ? Style.selectedFillFor(Commons.Color.popups.text, Commons.Color.accent) : Util.alpha(Commons.Color.popups.text, row.index % 2 ? 0.02 : 0.04)
  MouseArea {
    anchors.fill: parent
    acceptedButtons: Qt.LeftButton | Qt.RightButton
    cursorShape: Qt.PointingHandCursor
    onClicked: function (mouse) {
      row.page.cursor = row.index;
      if (mouse.button === Qt.RightButton)
        row.menuRequested();
      else
        row.page.edit(row.item, false);
    }
  }
  Row {
    anchors.fill: parent
    ProxyCheck {
      width: Style.space(28)
      anchors.verticalCenter: parent.verticalCenter
      checked: !!row.page.selected[row.item.name]
      onToggled: row.page.selected = Models.put(row.page.selected, row.item.name, checked)
    }
    Column {
      width: (parent.width - Style.space(28)) * 0.35
      anchors.verticalCenter: parent.verticalCenter
      spacing: Style.space(4)
      Txt {
        width: parent.width
        text: row.item.name
        elide: Text.ElideRight
        font.pixelSize: Style.font.body
      }
      Txt {
        width: parent.width
        text: row.item.server ? row.item.server + ":" + (row.item.port || "") : "—"
        elide: Text.ElideRight
        faint: true
        font.pixelSize: Style.font.bodySmall
      }
    }
    Txt {
      width: (parent.width - Style.space(28)) * 0.16
      text: row.item.type
      color: Commons.Color.accent
      anchors.verticalCenter: parent.verticalCenter
      elide: Text.ElideRight
      font.pixelSize: Style.font.bodySmall
    }
    Item {
      id: delayCell
      readonly property var history: row.page.latency[row.item.name] || null
      width: (parent.width - Style.space(28)) * 0.16
      height: Style.space(16)
      anchors.verticalCenter: parent.verticalCenter
      Txt {
        id: nodeDelay
        anchors.verticalCenter: parent.verticalCenter
        text: row.page.testing[row.item.name] ? "…" : row.item.delay === 0 ? MihomoI18n.t("timeout") : Models.delayText(row.item.delay, MihomoI18n.t("ms"))
        color: row.item.delay === 0 ? row.page.svc.red : row.item.delay > 0 ? row.page.svc.green : Commons.Color.muted
        font.pixelSize: Style.font.bodySmall
      }
      Sparkline {
        objectName: "nodeSpark"
        visible: !!delayCell.history && width > Style.space(16)
        anchors.left: nodeDelay.right
        anchors.leftMargin: Style.space(6)
        anchors.right: parent.right
        anchors.rightMargin: Style.space(6)
        anchors.verticalCenter: parent.verticalCenter
        height: Style.space(12)
        points: delayCell.history ? delayCell.history.points : []
        color: Models.latencyLoss(delayCell.history) >= 10 ? row.page.svc.yellow : row.page.svc.green
        failColor: row.page.svc.red
      }
      HoverHandler { id: delayHover }
      Hint { visible: delayHover.hovered && !!delayCell.history; text: Models.latencyHint(delayCell.history, MihomoI18n.t) }
    }
    Txt {
      width: (parent.width - Style.space(28)) * 0.16
      text: row.page.groupsFor(row.item)
      dim: true
      anchors.verticalCenter: parent.verticalCenter
      elide: Text.ElideRight
      font.pixelSize: Style.font.bodySmall
    }
    Txt {
      width: (parent.width - Style.space(28)) * 0.17
      text: MihomoI18n.t("proxySource" + row.item.source.charAt(0).toUpperCase() + row.item.source.slice(1))
      dim: true
      anchors.verticalCenter: parent.verticalCenter
      elide: Text.ElideRight
      font.pixelSize: Style.font.bodySmall
    }
  }
}
