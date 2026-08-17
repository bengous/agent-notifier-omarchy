import qs.Commons
import qs.Ui

FooterButton {
  id: root

  property var info: null

  iconText: "󰋼"
  iconSize: Style.font.iconSmall

  readonly property string versionText: {
    if (!info) return "agent-notifier\nversion info unavailable"
    var lines = ["agent-notifier " + info.version]
    lines.push("commit " + info.commit + (info.dirty ? " (dirty)" : ""))
    if (info.commitDate && info.commitDate !== "unknown")
      lines.push("committed " + String(info.commitDate).slice(0, 10))
    return lines.join("\n")
  }

  // Button's built-in tooltip centers on the button and would clip outside
  // the card-sized popup surface; x: 0 keeps it inside.
  PanelToolTip {
    visible: root.hot
    text: root.versionText
    fontFamily: root.fontFamily
    x: 0
  }
}
