.pragma library

var STATES = ["harness-absent", "config-absent", "hook-absent", "hook-stale", "wired"]

var STATE_LABELS = {
  "harness-absent": "not installed",
  "config-absent": "config absent",
  "hook-absent": "hook absent",
  "hook-stale": "hook stale",
  "wired": "wired"
}

function coerce(raw) {
  if (!raw || typeof raw !== "object") return null
  var rows = Array.isArray(raw.harnesses) ? raw.harnesses : []
  var harnesses = []
  for (var index = 0; index < rows.length; index++) {
    var row = rows[index] || {}
    var state = String(row.state || "")
    if (STATES.indexOf(state) === -1) {
      console.warn("agent-notifier", "Dropping a harness row with unknown state", state)
      continue
    }
    harnesses.push({
      harness: String(row.harness || ""),
      displayName: String(row.displayName || ""),
      state: state
    })
  }
  return {
    ready: raw.ready === true,
    listenerLive: raw.listenerLive === true,
    harnesses: harnesses
  }
}

function stateLabel(state) {
  return STATE_LABELS[state] || String(state)
}
