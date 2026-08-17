import QtQuick
import qs.Commons
import qs.Ui
import ".."

Column {
  id: root

  property color foreground: Color.foreground
  property color dim: Color.foreground
  property color urgent: Color.urgent
  property string fontFamily: Style.font.family
  property bool cliMissing: false

  spacing: Style.spacing.xl

  PanelHero {
    width: parent.width
    foreground: root.foreground
    fontFamily: root.fontFamily
    title: "Setup required"
    meta: root.cliMissing ? "binary not found" : "hooks not wired"
    iconComponent: Component {
      Text {
        text: "󰂚"
        color: root.urgent
        font.family: root.fontFamily
        font.pixelSize: Style.font.display
      }
    }
  }

  Rectangle {
    width: parent.width
    visible: root.cliMissing
    implicitHeight: binaryText.implicitHeight + Style.spacing.rowGap * 2
    radius: Style.cornerRadius
    color: "transparent"

    Rectangle {
      anchors.left: parent.left
      anchors.top: parent.top
      anchors.bottom: parent.bottom
      anchors.leftMargin: Style.space(4)
      anchors.topMargin: Style.space(6)
      anchors.bottomMargin: Style.space(6)
      width: Style.space(3)
      radius: width / 2
      color: root.urgent
    }

    Column {
      id: binaryText
      anchors.left: parent.left
      anchors.right: parent.right
      anchors.verticalCenter: parent.verticalCenter
      anchors.leftMargin: Style.spacing.rowPaddingX
      anchors.rightMargin: Style.spacing.rowPaddingX
      spacing: Style.space(2)

      Text {
        width: parent.width
        text: "The agent-notifier binary is not on PATH"
        color: root.foreground
        font.family: root.fontFamily
        font.pixelSize: Style.font.body
        font.bold: true
        wrapMode: Text.WordWrap
      }

      Text {
        width: parent.width
        text: "Install it first: README, section Install. The widget re-checks every "
              + Math.round(Theme.cliReprobeMs / 1000) + " s."
        color: root.dim
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        wrapMode: Text.WordWrap
      }
    }
  }
}
