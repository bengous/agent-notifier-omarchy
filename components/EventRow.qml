import QtQuick
import qs.Commons
import ".."
import "../js/time.js" as Time

Rectangle {
  id: root

  required property var modelData
  property color foreground: Color.foreground
  property color dim: Color.foreground
  property string fontFamily: Style.font.family
  property real nowMs: Date.now()

  signal activated(var event)

  readonly property bool unread: String(modelData.status) === "unread"
  readonly property string agent: String(modelData.agent || "").trim()
  readonly property color brandColor: Theme.brandColors[agent] || Color.accent

  // Same convention as omarchy.agents: assets/<agent>.svg, with a -light twin
  // picked on light surfaces for marks that only ship in white.
  readonly property string agentIcon: {
    if (agent === "claude") return Qt.resolvedUrl("../assets/claude.svg")
    if (agent === "codex")
      return Qt.resolvedUrl(Color.background.hslLightness >= 0.5 ? "../assets/codex-light.svg" : "../assets/codex.svg")
    return ""
  }

  width: ListView.view ? ListView.view.width : implicitWidth
  implicitHeight: rowText.implicitHeight + Style.spacing.rowGap * 2
  radius: Style.cornerRadius
  color: rowHover.pressed ? Style.pressedFill : rowHover.containsMouse ? Style.hoverFill : "transparent"

  Behavior on color {
    ColorAnimation { duration: Theme.hoverAnimationMs }
  }

  // Brand-colored unread rail: encodes which agent and unread in one
  // element. It sits inside the rowPaddingX inset, so read and unread
  // titles stay aligned.
  Rectangle {
    anchors.left: parent.left
    anchors.top: parent.top
    anchors.bottom: parent.bottom
    anchors.leftMargin: Style.space(4)
    anchors.topMargin: Style.space(6)
    anchors.bottomMargin: Style.space(6)
    width: Style.space(3)
    radius: width / 2
    color: root.brandColor
    opacity: root.unread ? 1 : 0
  }

  Column {
    id: rowText
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.verticalCenter: parent.verticalCenter
    anchors.leftMargin: Style.spacing.rowPaddingX
    anchors.rightMargin: Style.spacing.rowPaddingX
    spacing: Style.space(2)

    Text {
      width: parent.width
      text: String(root.modelData.displayLabel || "")
      color: root.unread ? root.foreground : root.dim
      font.family: root.fontFamily
      font.pixelSize: Style.font.body
      font.bold: root.unread
      elide: Text.ElideRight
    }

    Row {
      id: metaRow
      width: parent.width
      spacing: Style.space(6)

      Row {
        id: agentMark
        spacing: Style.space(4)
        anchors.verticalCenter: parent.verticalCenter
        opacity: root.unread ? 1 : 0.6

        Image {
          visible: root.agentIcon !== ""
          source: root.agentIcon
          width: Style.font.body
          height: Style.font.body
          sourceSize.width: width
          sourceSize.height: height
          fillMode: Image.PreserveAspectFit
          anchors.verticalCenter: parent.verticalCenter
        }

        Rectangle {
          visible: root.agentIcon === ""
          width: Style.space(6)
          height: Style.space(6)
          radius: height / 2
          color: root.brandColor
          anchors.verticalCenter: parent.verticalCenter
        }

        Text {
          text: root.agent
          color: root.brandColor
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
          anchors.verticalCenter: parent.verticalCenter
        }
      }

      Text {
        width: metaRow.width - agentMark.width - metaRow.spacing
        text: Time.relativeTime(root.modelData.createdAt, root.nowMs)
        color: root.dim
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        elide: Text.ElideRight
        anchors.verticalCenter: parent.verticalCenter
      }
    }
  }

  MouseArea {
    id: rowHover
    anchors.fill: parent
    hoverEnabled: true
    cursorShape: Qt.PointingHandCursor
    onClicked: root.activated(root.modelData)
  }
}
