import QtQuick
import QtTest

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
