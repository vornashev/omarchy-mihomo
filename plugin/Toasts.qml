pragma Singleton
import QtQuick

// Desktop notifications shared by every copy of the widget. The shell puts
// one bar on each monitor, each with its own Service and daemon connection,
// so every event arrives once per bar; only the first copy may toast it.
QtObject {
  // How long an event counts as already toasted.
  property int windowMs: 5000
  // Notification id per notice key, so an update replaces the toast.
  property var ids: ({})
  // Event id -> time first claimed.
  property var claimed: ({})
  // Notice key -> how many open panels show it.
  property var watched: ({})

  // True for the first copy to claim `id` within the window.
  function claim(id, now) {
    var t = now === undefined ? Date.now() : now
    var next = {}
    for (var k in claimed) if (t - claimed[k] < windowMs) next[k] = claimed[k]
    var first = next[id] === undefined
    if (first) next[id] = t
    claimed = next
    return first
  }

  // A panel moved from showing notice `from` to showing `to` ("" = none).
  function watch(from, to) {
    var next = Object.assign({}, watched)
    if (from && next[from] > 0 && --next[from] === 0) delete next[from]
    if (to) next[to] = (next[to] || 0) + 1
    watched = next
  }

  function onScreen(key) { return watched[key] > 0 }
}
