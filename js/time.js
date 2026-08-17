.pragma library

const MS_PER_MINUTE = 60000
const MS_PER_DAY = 86400000

function relativeTime(createdAt, nowMs) {
  var moment = new Date(String(createdAt || ""))
  if (isNaN(moment.getTime())) return String(createdAt || "")
  var minutes = Math.floor((nowMs - moment.getTime()) / MS_PER_MINUTE)
  if (minutes < 1) return "just now"
  if (minutes < 60) return minutes + " min ago"
  return absoluteTime(createdAt)
}

function absoluteTime(createdAt) {
  var moment = new Date(String(createdAt || ""))
  if (isNaN(moment.getTime())) return String(createdAt || "")
  var now = new Date()
  var startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate())
  var startOfDay = new Date(moment.getFullYear(), moment.getMonth(), moment.getDate())
  var days = Math.round((startOfToday.getTime() - startOfDay.getTime()) / MS_PER_DAY)
  if (days < 1) return Qt.formatDateTime(moment, "HH:mm")
  if (days < 6) return Qt.formatDateTime(moment, "ddd HH:mm")
  return Qt.formatDateTime(moment, "MMM d HH:mm")
}
