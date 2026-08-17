import QtQuick

// The QML half of the widget contract: the keys BarWidget binds to, projected
// out of a real list-display-json. A display state that stops carrying one of
// them breaks here the way it breaks the widget.
QtObject {
  id: root

  readonly property var readKeys: [
    "id",
    "agent",
    "status",
    "createdAt",
    "displayLabel",
    "displayProject"
  ]

  // The QML mirror of the Rust half:
  // the_setup_summary_exposes_exactly_the_keys_the_widget_reads.
  readonly property var setupKeys: ["ready", "listenerLive", "harnesses"]
  readonly property var setupRowKeys: ["harness", "displayName", "state"]

  property var state: ({})

  readonly property bool eventsIsArray: Array.isArray(root.state.events)
  readonly property var rows: root.eventsIsArray ? root.state.events.map(root.project) : []
  readonly property var sections: root.rows.map(function (row) { return row.displayProject })
  readonly property var setup: root.state.setup

  function apply(raw) {
    root.state = JSON.parse(String(raw || ""))
  }

  function project(event) {
    var row = {}
    for (var index = 0; index < root.readKeys.length; index++) {
      var key = root.readKeys[index]
      row[key] = event[key] === undefined ? "" : String(event[key])
    }
    return row
  }
}
