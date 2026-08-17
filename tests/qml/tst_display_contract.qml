import QtQuick
import QtTest
import "../../widget/js/setup.js" as Setup

TestCase {
  id: testCase
  name: "DisplayContract"

  property int fixtureEventCount: 3
  property int fixtureUnreadCount: 2

  DisplayContract {
    id: contract
  }

  // Written by tests/qml/run.sh: the real binary printing a real state.
  function readDisplayState() {
    var request = new XMLHttpRequest()
    request.open("GET", Qt.resolvedUrl("generated/display-state.json"), false)
    request.send()
    return request.responseText
  }

  function initTestCase() {
    contract.apply(testCase.readDisplayState())
  }

  function test_the_display_state_lists_its_events_as_an_array() {
    verify(contract.eventsIsArray)
    compare(contract.rows.length, testCase.fixtureEventCount)
  }

  function test_every_row_carries_the_keys_the_widget_binds_to() {
    for (var index = 0; index < contract.rows.length; index++) {
      var row = contract.rows[index]
      for (var key = 0; key < contract.readKeys.length; key++) {
        verify(row[contract.readKeys[key]] !== "",
               "row " + index + " carries no " + contract.readKeys[key])
      }
    }
  }

  function test_created_at_is_a_moment_the_widget_can_format() {
    for (var index = 0; index < contract.rows.length; index++) {
      var moment = new Date(contract.rows[index].createdAt)
      verify(!isNaN(moment.getTime()),
             "row " + index + " has an unreadable createdAt: " + contract.rows[index].createdAt)
    }
  }

  function test_an_unread_completion_says_unread() {
    var unread = contract.rows.filter(function (row) { return row.status === "unread" })
    compare(unread.length, testCase.fixtureUnreadCount)
  }

  // The gate's sandbox wires exactly one harness (a stub claude), so the
  // summary below is the same on every machine that runs it.
  function test_the_display_state_carries_the_setup_summary() {
    verify(contract.setup !== undefined, "the display state carries no setup")
    for (var key = 0; key < contract.setupKeys.length; key++)
      verify(contract.setup[contract.setupKeys[key]] !== undefined,
             "setup carries no " + contract.setupKeys[key])

    var rows = contract.setup.harnesses
    verify(Array.isArray(rows))
    compare(rows.map(function (row) { return row.harness }).join(","), "claude,codex,pi")
    for (var index = 0; index < rows.length; index++)
      for (var rowKey = 0; rowKey < contract.setupRowKeys.length; rowKey++)
        verify(rows[index][contract.setupRowKeys[rowKey]] !== undefined,
               "row " + index + " carries no " + contract.setupRowKeys[rowKey])

    compare(rows[0].state, "wired")
    compare(contract.setup.ready, true)
  }

  function test_a_harness_row_with_an_unknown_state_is_dropped() {
    var coerced = Setup.coerce({
      ready: false,
      listenerLive: false,
      harnesses: [
        { harness: "claude", displayName: "Claude", state: "wired" },
        { harness: "next", displayName: "Next", state: "quantum-entangled" }
      ]
    })
    compare(coerced.harnesses.length, 1)
    compare(coerced.harnesses[0].harness, "claude")
    compare(Setup.coerce(null), null)
  }

  // ListView groups on adjacency: a project that came back after another one
  // would draw its section header twice.
  function test_every_project_owns_one_run_of_rows() {
    var seen = []
    for (var index = 0; index < contract.sections.length; index++) {
      var section = contract.sections[index]
      if (index > 0 && section === contract.sections[index - 1]) continue
      verify(seen.indexOf(section) === -1, "project " + section + " opens a second section")
      seen.push(section)
    }
    compare(seen.length, 2)
  }
}
