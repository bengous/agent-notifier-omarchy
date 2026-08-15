import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

BarWidget {
  id: root
  moduleName: "io.github.bengous.agent-notifier"

  readonly property string stateDir: (Quickshell.env("XDG_STATE_HOME") || (Quickshell.env("HOME") + "/.local/state")) + "/agent-notifier"

  property var events: []
  property bool cliMissing: false
  property bool popupOpen: false
  property bool refreshQueued: false
  property var pendingCommands: []

  function close() { popupOpen = false }

  readonly property int unreadCount: {
    var count = 0
    for (var i = 0; i < events.length; i++)
      if (String(events[i].status) === "unread") count++
    return count
  }

  // The lines agent-notifier itself prints as its tooltip: one unread
  // completion per line, the same order the state file keeps them in.
  readonly property string tooltipLines: {
    var lines = []
    for (var i = 0; i < events.length; i++) {
      var event = events[i]
      if (String(event.status) !== "unread") continue
      lines.push(String(event.displayLabel || "") + " " + String(event.displayCreatedAt || ""))
    }
    return lines.length === 0 ? "No agent completions" : lines.join("\n")
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
    listProcess.exitSeen = false
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

  // A binary that is not on PATH never reaches onExited: Process reverts
  // running to false on a failed start, and that is the only signal QML gets.
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
    commandProcess.exitSeen = false
    commandProcess.command = ["agent-notifier"].concat(next)
    commandProcess.running = true
  }

  function activateEvent(event) {
    if (!event) return
    var id = String(event.id || "")
    if (id === "") return
    enqueue(["focus-id", id])
    enqueue(["mark-read", id])
    close()
  }

  visible: !cliMissing
  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  FileView {
    path: root.stateDir + "/events.json"
    watchChanges: true
    printErrors: false
    onFileChanged: reload()
    onLoaded: root.refresh()
    onLoadFailed: root.refresh()
  }

  Process {
    id: listProcess
    property bool exitSeen: false
    command: ["agent-notifier", "list-display-json"]

    stdout: StdioCollector {
      id: listOutput
      waitForEnd: true
    }

    stderr: StdioCollector {
      waitForEnd: true
      onStreamFinished: if (text.trim() !== "") console.warn("agent-notifier", text.trim())
    }

    onExited: function(exitCode) {
      exitSeen = true
      if (exitCode === 0) root.applyDisplayState(listOutput.text)
    }

    onRunningChanged: {
      if (running) return
      if (!exitSeen) root.reportMissingCli()
      if (root.refreshQueued) {
        root.refreshQueued = false
        root.refresh()
      }
    }
  }

  Process {
    id: commandProcess
    property bool exitSeen: false

    stderr: StdioCollector {
      waitForEnd: true
      onStreamFinished: if (text.trim() !== "") console.warn("agent-notifier", text.trim())
    }

    onExited: exitSeen = true

    onRunningChanged: {
      if (running) return
      if (!exitSeen) root.reportMissingCli()
      root.pumpCommands()
    }
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
      spacing: Style.space(10)

      RowLayout {
        Layout.fillWidth: true
        spacing: Style.space(8)

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

      ListView {
        id: eventList
        Layout.fillWidth: true
        Layout.fillHeight: true
        visible: root.events.length > 0
        clip: true
        spacing: Style.space(4)
        model: root.events
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

        delegate: Rectangle {
          id: eventRow
          required property var modelData

          readonly property bool unread: String(modelData.status) === "unread"

          width: ListView.view.width
          implicitHeight: rowText.implicitHeight + Style.spacing.rowGap * 2
          radius: Style.cornerRadius
          color: rowHover.containsMouse ? Style.hoverFill : "transparent"

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
                text: String(eventRow.modelData.displayCreatedAt || "")
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

      RowLayout {
        Layout.fillWidth: true
        spacing: Style.spacing.md

        Button {
          Layout.fillWidth: true
          text: "Clear read"
          bordered: true
          foreground: root.foreground
          fontFamily: root.fontFamily
          fontSize: Style.font.bodySmall
          verticalPadding: Style.spacing.controlPaddingY
          onClicked: root.enqueue(["clear-read"])
        }

        Button {
          Layout.fillWidth: true
          text: "Clear all"
          bordered: true
          foreground: root.foreground
          fontFamily: root.fontFamily
          fontSize: Style.font.bodySmall
          verticalPadding: Style.spacing.controlPaddingY
          onClicked: root.enqueue(["clear-all"])
        }
      }
    }
  }
}
