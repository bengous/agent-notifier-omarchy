pragma Singleton
import QtQuick

QtObject {
  readonly property var brandColors: ({ claude: "#d97757", codex: "#10a37f", pi: "#a78bfa" })
  readonly property real dimFactor: 1.55
  readonly property real readMarkOpacity: 0.6
  readonly property int hoverAnimationMs: 120
  readonly property int relativeTimeRefreshMs: 30000
  readonly property int cliReprobeMs: 15000
}
