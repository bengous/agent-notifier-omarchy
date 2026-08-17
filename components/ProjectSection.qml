import QtQuick
import qs.Commons
import qs.Ui

Column {
  id: root

  // Named section by contract: a section delegate that declares required
  // properties gets the section value injected into exactly this name, and
  // the context-property fallback is switched off.
  required property string section
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
    text: root.section
    foreground: root.foreground
    fontFamily: root.fontFamily
    elide: Text.ElideRight
  }
}
