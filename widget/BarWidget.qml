import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "."
import "components"
import "js/setup.js" as Setup
import "js/time.js" as Time

BarWidget {
  id: root
  moduleName: "io.github.bengous.agent-notifier"

  property var events: []
  property var setup: null
  property var versionInfo: null
  property bool cliMissing: false
  property bool popupOpen: false
  property bool refreshQueued: false
  property var pendingCommands: []
  property real nowMs: Date.now()

  function close() { popupOpen = false }

  onPopupOpenChanged: if (popupOpen) refresh()

  readonly property int unreadCount: events.filter(event => String(event.status) === "unread").length
  readonly property bool needsSetup: cliMissing || (setup !== null && setup.ready === false)

  // The popup shows exactly one of these faces; the list always wins.
  readonly property string face: events.length > 0 ? "list"
    : cliMissing ? "binary-missing"
    : needsSetup ? "setup"
    : "waiting"

  readonly property string tooltipLines: {
    var lines = events
      .filter(event => String(event.status) === "unread")
      .map(event => String(event.displayLabel || "") + " " + Time.absoluteTime(event.createdAt))
    if (lines.length > 0) return lines.join("\n")
    if (cliMissing) return "agent-notifier is not on PATH. Install it first: README, section Install."
    if (needsSetup) return "Setup required — run: agent-notifier doctor"
    return "Waiting for agent completions"
  }

  readonly property color foreground: bar ? bar.foreground : Color.foreground
  readonly property color urgent: bar ? bar.urgent : Color.urgent
  readonly property color dim: Qt.darker(foreground, Theme.dimFactor)
  readonly property string fontFamily: bar ? bar.fontFamily : Style.font.family

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
    root.setup = parsed ? Setup.coerce(parsed.setup) : null
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
    console.warn("agent-notifier", "The agent-notifier binary is not on PATH; showing the setup card")
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

  // The one piece of the clone the widget runs, and only on a click. The
  // helper joins its arguments with spaces into a single `bash -c`, so it gets
  // exactly one: the script path. Quickshell exposes no path to the plugin
  // clone, so the script is resolved against this file.
  function launchOnboarding() {
    var resolved = String(Qt.resolvedUrl("../scripts/onboard.sh"))
    var script = decodeURIComponent(resolved.replace(/^file:\/\//, ""))
    Quickshell.execDetached(["omarchy-launch-floating-terminal-with-presentation", script])
  }

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
    interval: Theme.relativeTimeRefreshMs
    repeat: true
    triggeredOnStart: true
    onTriggered: root.nowMs = Date.now()
  }

  // An unfinished setup is a reversible state: once onboarding wires it, the
  // next probe sees it. Nothing comes back from the terminal the CTA opens, so
  // this re-probe is what observes the end of it.
  Timer {
    running: root.needsSetup
    interval: Theme.cliReprobeMs
    repeat: true
    onTriggered: root.refresh()
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
    onSucceeded: function(stdout) {
      root.cliMissing = false
      root.applyDisplayState(stdout)
    }
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
      visible: root.unreadCount > 0 || root.needsSetup
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
        text: root.unreadCount > 0 ? String(root.unreadCount) : "!"
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
        visible: root.face === "list"
        clip: true
        spacing: Style.space(4)
        model: root.events
        boundsBehavior: Flickable.StopAtBounds
        section.property: "displayProject"
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

        section.delegate: ProjectSection {
          foreground: root.foreground
          fontFamily: root.fontFamily
        }

        delegate: EventRow {
          foreground: root.foreground
          dim: root.dim
          fontFamily: root.fontFamily
          nowMs: root.nowMs
          onActivated: function(event) { root.activateEvent(event) }
        }
      }

      SetupCard {
        Layout.fillWidth: true
        visible: root.face === "binary-missing" || root.face === "setup"
        foreground: root.foreground
        dim: root.dim
        urgent: root.urgent
        fontFamily: root.fontFamily
        cliMissing: root.cliMissing
        setup: root.setup
        onOnboardingRequested: root.launchOnboarding()
      }

      Item {
        Layout.fillHeight: true
        visible: root.face === "binary-missing" || root.face === "setup"
      }

      Item {
        Layout.fillWidth: true
        Layout.fillHeight: true
        visible: root.face === "waiting"

        Text {
          anchors.centerIn: parent
          text: "Waiting for agent completions"
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

        VersionInfo {
          info: root.versionInfo
          hotForeground: root.foreground
          idleForeground: root.dim
          fontFamily: root.fontFamily
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
