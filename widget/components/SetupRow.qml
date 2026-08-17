import QtQuick
import qs.Commons
import ".."

Rectangle {
  id: root

  property color foreground: Color.foreground
  property color dim: Color.foreground
  property color railColor: Color.accent
  property bool dimmed: false
  property string fontFamily: Style.font.family
  property string title: ""
  property string detail: ""

  implicitHeight: rowText.implicitHeight + Style.spacing.rowGap * 2
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
    color: root.railColor
    opacity: root.dimmed ? Theme.readMarkOpacity : 1
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
      text: root.title
      color: root.dimmed ? root.dim : root.foreground
      font.family: root.fontFamily
      font.pixelSize: Style.font.body
      wrapMode: Text.WordWrap
    }

    Text {
      width: parent.width
      visible: root.detail !== ""
      text: root.detail
      color: root.dim
      font.family: root.fontFamily
      font.pixelSize: Style.font.caption
      wrapMode: Text.WordWrap
    }
  }
}
