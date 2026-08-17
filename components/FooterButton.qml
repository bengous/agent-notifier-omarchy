import QtQuick
import qs.Commons
import qs.Ui

Button {
  id: root

  property color hotForeground: Color.foreground
  property color idleForeground: Color.foreground

  foreground: root.hot ? root.hotForeground : root.idleForeground
  fontSize: Style.font.caption
  horizontalPadding: Style.space(5)
  verticalPadding: Style.space(3)
}
