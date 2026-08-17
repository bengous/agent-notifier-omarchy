import QtQuick
import qs.Commons
import qs.Ui

Column {
  id: root

  required property string title
  property color foreground: Color.foreground
  property string fontFamily: Style.font.family

  width: ListView.view ? ListView.view.width : implicitWidth
  spacing: Style.spacing.sm

  PanelSeparator {
    foreground: root.foreground
  }

  PanelSectionHeader {
    width: parent.width
    leftPadding: Style.spacing.rowPaddingX
    rightPadding: Style.spacing.rowPaddingX
    text: root.title
    foreground: root.foreground
    fontFamily: root.fontFamily
    elide: Text.ElideRight
  }
}
