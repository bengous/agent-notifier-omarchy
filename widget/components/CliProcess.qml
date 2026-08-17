import Quickshell.Io

Process {
  id: root

  property bool warnStderr: true
  property bool exitSeen: false

  signal succeeded(string stdout)
  signal startFailed()
  signal settled()

  stdout: StdioCollector {
    id: collectedStdout
    waitForEnd: true
  }

  stderr: StdioCollector {
    waitForEnd: true
    onStreamFinished: if (root.warnStderr && text.trim() !== "") console.warn("agent-notifier", text.trim())
  }

  onExited: function(exitCode) {
    exitSeen = true
    if (exitCode === 0) root.succeeded(collectedStdout.text)
  }

  // A binary that is not on PATH never reaches onExited: Process reverts
  // running to false on a failed start, and that is the only signal QML gets.
  // After a successful run, a failed start never passes through running ==
  // true, so exitSeen also resets when the cycle settles — otherwise the
  // exit seen last run masks the failure.
  onRunningChanged: {
    if (running) {
      exitSeen = false
      return
    }
    if (!exitSeen) root.startFailed()
    exitSeen = false
    root.settled()
  }
}
