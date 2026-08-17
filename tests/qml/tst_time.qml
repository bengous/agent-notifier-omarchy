import QtQuick
import QtTest
import "../../js/time.js" as Time

TestCase {
  name: "Time"

  // Local-midnight construction keeps every day-boundary assertion true at
  // any wall-clock hour the suite runs.
  function localMoment(daysAgo, hour, minute) {
    var now = new Date()
    return new Date(now.getFullYear(), now.getMonth(), now.getDate() - daysAgo, hour, minute)
  }

  function test_a_moment_under_a_minute_old_reads_just_now() {
    var moment = localMoment(0, 12, 0)
    compare(Time.relativeTime(moment.toISOString(), moment.getTime() + 30 * 1000), "just now")
  }

  function test_a_moment_under_an_hour_old_counts_minutes() {
    var moment = localMoment(0, 12, 0)
    compare(Time.relativeTime(moment.toISOString(), moment.getTime() + 5 * 60 * 1000), "5 min ago")
    compare(Time.relativeTime(moment.toISOString(), moment.getTime() + 59 * 60 * 1000), "59 min ago")
  }

  function test_a_moment_an_hour_old_falls_back_to_the_absolute_time() {
    var moment = localMoment(0, 12, 0)
    var iso = moment.toISOString()
    compare(Time.relativeTime(iso, moment.getTime() + 60 * 60 * 1000), Time.absoluteTime(iso))
  }

  function test_a_moment_today_formats_as_clock_time() {
    compare(Time.absoluteTime(localMoment(0, 12, 30).toISOString()), "12:30")
  }

  function test_a_moment_this_week_carries_its_weekday() {
    var moment = localMoment(3, 9, 5)
    compare(Time.absoluteTime(moment.toISOString()), Qt.formatDateTime(moment, "ddd HH:mm"))
  }

  function test_a_moment_older_than_the_week_carries_its_date() {
    var moment = localMoment(10, 9, 5)
    compare(Time.absoluteTime(moment.toISOString()), Qt.formatDateTime(moment, "MMM d HH:mm"))
  }

  function test_an_unreadable_moment_passes_through_verbatim() {
    compare(Time.relativeTime("not-a-date", Date.now()), "not-a-date")
    compare(Time.absoluteTime("not-a-date"), "not-a-date")
  }
}
