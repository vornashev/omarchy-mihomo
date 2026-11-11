import QtQuick
import qs.Commons
import qs.Commons as Commons
import "SiteModels.js" as SiteModels

// A site on the check page · SiteCard: favicon or letter, name and host,
// the last latency and the route it went by. A click checks it.
Rectangle {
  id: root
  property var site: ({})
  property var result: null
  property bool testing: false
  property bool selected: false
  property string iconFile: ""
  property var svc
  // Tiles pick their letter's colour from these.
  property var tints: [svc.red, svc.green, svc.yellow, svc.peach, Commons.Color.accent]
  readonly property string kind: SiteModels.state(result, testing)
  readonly property color stateColor: kind === "good" ? svc.green : kind === "slow" ? svc.yellow : kind === "fail" ? svc.red
    : kind === "testing" ? Commons.Color.accent : Util.alpha(Commons.Color.popups.text, 0.42)
  readonly property bool hovered: ma.containsMouse || menuBtn.hot
  signal checkRequested()
  signal menuRequested(Item anchor)
  signal pressedAt()

  function errorText(e) {
    if (e === "timeout") return MihomoI18n.t("siteTimeout")
    if (e === "connect") return MihomoI18n.t("siteNoConnect")
    if (e === "core_down") return MihomoI18n.t("siteCoreDown")
    return MihomoI18n.t("siteError")
  }
  readonly property string latencyText: kind === "testing" ? MihomoI18n.t("siteTesting")
    : kind === "idle" ? "—"
    : kind === "fail" ? errorText(result.error)
    : MihomoI18n.t("siteMs").arg(result.delay)
  readonly property string routeText: kind === "idle" ? MihomoI18n.t("siteNotChecked") : SiteModels.route(result)

  implicitHeight: col.implicitHeight + Style.space(24)
  radius: Style.cornerRadius + 2
  color: selected ? Style.selectedFillFor(Commons.Color.popups.text, Commons.Color.accent)
    : (hovered ? Style.hoverFillFor(Commons.Color.popups.text, Commons.Color.accent) : Util.alpha(Commons.Color.popups.text, 0.03))
  border.width: 1
  border.color: selected ? Util.alpha(Commons.Color.accent, 0.7) : (testing ? Util.alpha(Commons.Color.accent, 0.5) : Util.alpha(Commons.Color.popups.text, 0.12))

  MouseArea {
    id: ma
    anchors.fill: parent
    hoverEnabled: true
    acceptedButtons: Qt.LeftButton | Qt.RightButton
    cursorShape: Qt.PointingHandCursor
    onPressed: root.pressedAt()
    onClicked: function(mouse) {
      if (mouse.button === Qt.RightButton) root.menuRequested(menuBtn)
      else root.checkRequested()
    }
  }

  Column {
    id: col
    anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
    anchors.margins: Style.space(12)
    spacing: Style.space(12)
    Item {
      width: parent.width
      height: Style.space(32)
      Rectangle {
        id: tile
        width: Style.space(32); height: width
        radius: Style.cornerRadius + 2
        color: Util.alpha(Commons.Color.popups.text, 0.08)
        Image {
          id: favicon
          objectName: "siteFavicon"
          anchors.centerIn: parent
          width: Style.space(20); height: width
          sourceSize.width: width * 2; sourceSize.height: height * 2
          fillMode: Image.PreserveAspectFit
          smooth: true
          visible: root.iconFile !== "" && status === Image.Ready
          source: root.iconFile !== "" ? "file://" + root.iconFile : ""
        }
        Txt {
          anchors.centerIn: parent
          visible: !favicon.visible
          text: SiteModels.letter(root.site)
          bold: true
          font.pixelSize: Style.font.subtitle + 2
          color: SiteModels.tint(root.site.url, root.tints)
        }
      }
      Column {
        anchors.left: tile.right; anchors.leftMargin: Style.space(10)
        anchors.right: menuBtn.left; anchors.rightMargin: Style.space(6)
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(2)
        Txt { width: parent.width; text: root.site.name || ""; bold: true; elide: Text.ElideRight }
        Txt { width: parent.width; text: SiteModels.shortHost(root.site.url); faint: true; font.pixelSize: Style.font.bodySmall; elide: Text.ElideRight }
      }
      Btn {
        id: menuBtn
        objectName: "siteMenuButton"
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        ghost: true
        icon: Icons.more
        tooltip: MihomoI18n.t("siteMenu")
        opacity: root.hovered || root.selected ? 1 : 0
        onClicked: root.menuRequested(menuBtn)
      }
    }
    Item {
      width: parent.width
      height: latency.implicitHeight
      Rectangle {
        id: dot
        anchors.verticalCenter: parent.verticalCenter
        width: Style.space(6); height: width; radius: width / 2
        color: root.stateColor
      }
      Txt {
        id: latency
        objectName: "siteLatency"
        anchors.left: dot.right; anchors.leftMargin: Style.space(6)
        anchors.verticalCenter: parent.verticalCenter
        text: root.latencyText
        bold: root.kind !== "idle"
        color: root.kind === "testing" ? Util.alpha(Commons.Color.popups.text, 0.68) : root.stateColor
        HoverHandler { id: errHover }
        Hint { visible: errHover.hovered && root.kind === "fail" && !!root.result.error; text: root.result && root.result.error ? root.result.error : "" }
      }
      Txt {
        id: routeLabel
        objectName: "siteRoute"
        anchors.left: latency.right; anchors.leftMargin: Style.space(10)
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        horizontalAlignment: Text.AlignRight
        elide: Text.ElideRight
        text: root.routeText
        faint: root.kind === "idle"
        dim: root.kind !== "idle"
        font.pixelSize: Style.font.bodySmall
        HoverHandler { id: routeHover }
        Hint { visible: routeHover.hovered && !!(root.result && root.result.rule); text: root.result && root.result.rule ? MihomoI18n.t("siteRule").arg(root.result.rule) : "" }
      }
    }
  }
}
