import QtQuick
import qs.Commons
import qs.Ui
import ".."
import "../js/setup.js" as Setup

Column {
  id: root

  property color foreground: Color.foreground
  property color dim: Color.foreground
  property color urgent: Color.urgent
  property string fontFamily: Style.font.family
  property bool cliMissing: false
  property var setup: null

  signal setupHelpRequested()

  readonly property var harnessRows: !cliMissing && setup ? setup.harnesses : []
  readonly property bool listenerRowVisible: !cliMissing && setup !== null && setup.listenerLive === false

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

  SetupRow {
    width: parent.width
    visible: root.cliMissing
    foreground: root.foreground
    dim: root.dim
    railColor: root.urgent
    fontFamily: root.fontFamily
    title: "The agent-notifier binary is not on PATH"
    detail: "Install it first: README, section Install. The widget re-checks every "
            + Math.round(Theme.cliReprobeMs / 1000) + " s."
  }

  Repeater {
    model: root.harnessRows

    SetupRow {
      required property var modelData

      width: parent.width
      foreground: root.foreground
      dim: root.dim
      railColor: modelData.state === "wired"
        ? (Theme.brandColors[modelData.harness] || Color.accent)
        : root.urgent
      dimmed: modelData.state === "wired"
      fontFamily: root.fontFamily
      title: modelData.displayName + ": " + Setup.stateLabel(modelData.state)
    }
  }

  SetupRow {
    width: parent.width
    visible: root.listenerRowVisible
    foreground: root.foreground
    dim: root.dim
    railColor: root.dim
    fontFamily: root.fontFamily
    title: "Focused-window listener: not live"
    detail: "Optional: without it, focusing a window by hand does not mark its events read."
  }

  Button {
    visible: !root.cliMissing
    text: "Show setup steps"
    foreground: root.foreground
    bordered: true
    fontSize: Style.font.body
    onClicked: root.setupHelpRequested()
  }
}
