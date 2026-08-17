import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell.Io
import qs.Commons
import qs.Ui
import "components"
import "js/time.js" as Time

BarWidget {
  id: root
  moduleName: "io.github.bengous.agent-notifier"

  property var events: []
  property var versionInfo: null
  property bool cliMissing: false
  property bool popupOpen: false
  property bool refreshQueued: false
  property var pendingCommands: []
  property real nowMs: Date.now()

  function close() { popupOpen = false }

  onPopupOpenChanged: if (popupOpen) refresh()

  readonly property int unreadCount: events.filter(event => String(event.status) === "unread").length

  readonly property string tooltipLines: {
    var lines = events
      .filter(event => String(event.status) === "unread")
      .map(event => String(event.displayLabel || "") + " " + Time.absoluteTime(event.createdAt))
    return lines.length === 0 ? "No agent completions" : lines.join("\n")
  }

  readonly property string versionTooltip: {
    if (!versionInfo) return "agent-notifier\nversion info unavailable"
    var lines = ["agent-notifier " + versionInfo.version]
    lines.push("commit " + versionInfo.commit + (versionInfo.dirty ? " (dirty)" : ""))
    if (versionInfo.commitDate && versionInfo.commitDate !== "unknown")
      lines.push("committed " + String(versionInfo.commitDate).slice(0, 10))
    return lines.join("\n")
  }

  readonly property color foreground: bar ? bar.foreground : Color.foreground
  readonly property color urgent: bar ? bar.urgent : Color.urgent
  readonly property color dim: Qt.darker(foreground, 1.55)
  readonly property string fontFamily: bar ? bar.fontFamily : Style.font.family

  readonly property var brandColors: ({ claude: "#d97757", codex: "#10a37f", pi: "#a78bfa" })

  function brandColor(agent) {
    var color = brandColors[String(agent || "").trim()]
    return color ? color : Color.accent
  }

  // Same convention as omarchy.agents: assets/<agent>.svg, with a -light twin
  // picked on light surfaces for marks that only ship in white.
  function agentIcon(agent) {
    var name = String(agent || "").trim()
    if (name === "claude") return Qt.resolvedUrl("assets/claude.svg")
    if (name === "codex")
      return Qt.resolvedUrl(Color.background.hslLightness >= 0.5 ? "assets/codex-light.svg" : "assets/codex.svg")
    return ""
  }

  function refresh() {
    if (listProcess.running) {
      refreshQueued = true
      return
    }
    listProcess.running = true
  }

  function applyDisplayState(raw) {
    var parsed = null
    try {
      parsed = JSON.parse(String(raw || ""))
    } catch (error) {
      console.warn("agent-notifier", "Ignoring bad display JSON", error)
      return
    }
    root.events = parsed && Array.isArray(parsed.events) ? parsed.events : []
  }

  // Silent on failure: the fallback tooltip text is the user-facing signal,
  // and an older binary without version-json must not spam the journal.
  function applyVersionInfo(raw) {
    var parsed = null
    try {
      parsed = JSON.parse(String(raw || ""))
    } catch (error) {
      return
    }
    if (parsed && typeof parsed.version === "string" && parsed.version !== "")
      root.versionInfo = parsed
  }

  function reportMissingCli() {
    if (cliMissing) return
    cliMissing = true
    console.warn("agent-notifier", "The agent-notifier binary is not on PATH; hiding the widget")
  }

  function enqueue(args) {
    pendingCommands = pendingCommands.concat([args])
    pumpCommands()
  }

  function pumpCommands() {
    if (commandProcess.running || pendingCommands.length === 0) return
    var next = pendingCommands[0]
    pendingCommands = pendingCommands.slice(1)
    commandProcess.command = ["agent-notifier"].concat(next)
    commandProcess.running = true
  }

  function activateEvent(event) {
    if (!event) return
    var id = String(event.id || "")
    if (id === "") return
    enqueue(["focus-id", id])
    close()
  }

  visible: !cliMissing
  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  Component.onCompleted: refresh()

  IpcHandler {
    target: "io.github.bengous.agent-notifier"

    function open(): void { root.popupOpen = true }
    function close(): void { root.close() }
    function toggle(): void { root.popupOpen = !root.popupOpen }
  }

  Timer {
    running: root.popupOpen
    interval: 30000
    repeat: true
    triggeredOnStart: true
    onTriggered: root.nowMs = Date.now()
  }

  // The binary owns the state path and reports it as statePath; the watch
  // starts once version-json delivers it and stays off with an older binary
  // rather than deriving a second path from the environment (README, State).
  FileView {
    path: root.versionInfo ? String(root.versionInfo.statePath || "") : ""
    watchChanges: true
    printErrors: false
    onFileChanged: reload()
    onLoaded: root.refresh()
    onLoadFailed: root.refresh()
  }

  CliProcess {
    id: listProcess
    command: ["agent-notifier", "list-display-json"]
    onSucceeded: function(stdout) { root.applyDisplayState(stdout) }
    onStartFailed: root.reportMissingCli()
    onSettled: {
      if (root.refreshQueued) {
        root.refreshQueued = false
        root.refresh()
      }
    }
  }

  CliProcess {
    id: versionProcess
    running: true
    command: ["agent-notifier", "version-json"]
    warnStderr: false
    onSucceeded: function(stdout) { root.applyVersionInfo(stdout) }
  }

  CliProcess {
    id: commandProcess
    onStartFailed: root.reportMissingCli()
    onSettled: root.pumpCommands()
  }

  BarIconButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: "󰂚"
    active: root.unreadCount > 0
    dimmed: root.unreadCount === 0
    tooltipText: root.tooltipLines

    onPressed: function(pressedButton) {
      if (pressedButton === Qt.LeftButton) root.popupOpen = !root.popupOpen
    }

    BorderSurface {
      id: badge
      visible: root.unreadCount > 0
      anchors.right: parent.right
      anchors.top: parent.top
      anchors.rightMargin: Math.max(0, (button.width - Style.bar.iconCanvas) / 2 - Style.space(3))
      anchors.topMargin: Math.max(0, (button.height - Style.bar.iconCanvas) / 2 - Style.space(3))
      implicitWidth: Math.max(height, badgeCount.implicitWidth + Style.space(5))
      implicitHeight: Math.max(Style.space(10), badgeCount.implicitHeight + Style.space(2))
      radius: height / 2
      color: root.urgent
      borderSpec: Border.flat(Color.bar.background, 1)

      Text {
        id: badgeCount
        anchors.centerIn: parent
        text: String(root.unreadCount)
        color: Color.background
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        font.bold: true
      }
    }
  }

  PopupCard {
    id: popup
    anchorItem: button
    bar: root.bar
    owner: root
    open: root.popupOpen
    contentWidth: popup.fittedContentWidth(Style.space(380))
    contentHeight: popup.cappedContentHeight(Style.space(460))

    ColumnLayout {
      anchors.fill: parent
      spacing: Style.spacing.xl

      RowLayout {
        Layout.fillWidth: true
        spacing: Style.spacing.lg

        Text {
          text: "Agent completions"
          color: root.foreground
          font.family: root.fontFamily
          font.pixelSize: Style.font.title
          font.bold: true
        }

        Item { Layout.fillWidth: true }

        Text {
          visible: root.unreadCount > 0
          text: root.unreadCount + " unread"
          color: root.dim
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
        }
      }

      Text {
        Layout.fillWidth: true
        visible: root.events.length > 0
        text: "Click a completion to focus its session"
        color: root.dim
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        elide: Text.ElideRight
      }

      ListView {
        id: eventList
        Layout.fillWidth: true
        Layout.fillHeight: true
        visible: root.events.length > 0
        clip: true
        spacing: Style.space(4)
        model: root.events
        boundsBehavior: Flickable.StopAtBounds
        section.property: "displayProject"
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

        section.delegate: Column {
          width: ListView.view.width
          spacing: Style.spacing.sm

          PanelSeparator {
            foreground: root.foreground
          }

          PanelSectionHeader {
            width: parent.width
            leftPadding: Style.spacing.rowPaddingX
            rightPadding: Style.spacing.rowPaddingX
            text: section
            foreground: root.foreground
            fontFamily: root.fontFamily
            elide: Text.ElideRight
          }
        }

        delegate: Rectangle {
          id: eventRow
          required property var modelData

          readonly property bool unread: String(modelData.status) === "unread"

          width: ListView.view.width
          implicitHeight: rowText.implicitHeight + Style.spacing.rowGap * 2
          radius: Style.cornerRadius
          color: rowHover.pressed ? Style.pressedFill : rowHover.containsMouse ? Style.hoverFill : "transparent"

          Behavior on color {
            ColorAnimation { duration: 120 }
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
            color: root.brandColor(eventRow.modelData.agent)
            opacity: eventRow.unread ? 1 : 0
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
              text: String(eventRow.modelData.displayLabel || "")
              color: eventRow.unread ? root.foreground : root.dim
              font.family: root.fontFamily
              font.pixelSize: Style.font.body
              font.bold: eventRow.unread
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
                opacity: eventRow.unread ? 1 : 0.6

                readonly property string icon: root.agentIcon(eventRow.modelData.agent)

                Image {
                  visible: agentMark.icon !== ""
                  source: agentMark.icon
                  width: Style.font.body
                  height: Style.font.body
                  sourceSize.width: width
                  sourceSize.height: height
                  fillMode: Image.PreserveAspectFit
                  anchors.verticalCenter: parent.verticalCenter
                }

                Rectangle {
                  visible: agentMark.icon === ""
                  width: Style.space(6)
                  height: Style.space(6)
                  radius: height / 2
                  color: root.brandColor(eventRow.modelData.agent)
                  anchors.verticalCenter: parent.verticalCenter
                }

                Text {
                  text: String(eventRow.modelData.agent || "").trim()
                  color: root.brandColor(eventRow.modelData.agent)
                  font.family: root.fontFamily
                  font.pixelSize: Style.font.caption
                  anchors.verticalCenter: parent.verticalCenter
                }
              }

              Text {
                width: metaRow.width - agentMark.width - metaRow.spacing
                text: Time.relativeTime(eventRow.modelData.createdAt, root.nowMs)
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
            onClicked: root.activateEvent(eventRow.modelData)
          }
        }
      }

      Item {
        Layout.fillWidth: true
        Layout.fillHeight: true
        visible: root.events.length === 0

        Text {
          anchors.centerIn: parent
          text: "No agent completions"
          color: root.dim
          font.family: root.fontFamily
          font.pixelSize: Style.font.body
        }
      }

      PanelSeparator {
        Layout.fillWidth: true
        foreground: root.foreground
      }

      RowLayout {
        Layout.fillWidth: true
        spacing: Style.spacing.sm

        FooterButton {
          id: infoButton
          iconText: "󰋼"
          iconSize: Style.font.iconSmall
          hotForeground: root.foreground
          idleForeground: root.dim
          fontFamily: root.fontFamily

          // Button's built-in tooltip centers on the button and would clip
          // outside the card-sized popup surface; x: 0 keeps it inside.
          PanelToolTip {
            visible: infoButton.hot
            text: root.versionTooltip
            fontFamily: root.fontFamily
            x: 0
          }
        }

        Item { Layout.fillWidth: true }

        FooterButton {
          text: "Clear read"
          hotForeground: root.foreground
          idleForeground: root.dim
          fontFamily: root.fontFamily
          onClicked: root.enqueue(["clear-read"])
        }

        FooterButton {
          text: "Clear all"
          hotForeground: root.foreground
          idleForeground: root.dim
          fontFamily: root.fontFamily
          onClicked: root.enqueue(["clear-all"])
        }
      }
    }
  }
}
