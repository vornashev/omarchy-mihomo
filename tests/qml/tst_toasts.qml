import QtQuick
import QtTest
import "../../plugin" as Plugin

// Each monitor's bar runs its own copy of the widget; an event must still
// toast once, and not at all while any copy's panel shows it.
TestCase {
  name: "Toasts"

  function init() {
    Plugin.Toasts.claimed = {}
    Plugin.Toasts.watched = {}
  }

  function test_secondCopyOfAnEventIsDropped() {
    verify(Plugin.Toasts.claim("notice:group:AI@1", 1000))
    verify(!Plugin.Toasts.claim("notice:group:AI@1", 1001))
    verify(Plugin.Toasts.claim("notice:group:AI@2", 1001), "a new event with the same key still toasts")
  }

  function test_claimsExpire() {
    verify(Plugin.Toasts.claim("profile:x", 1000))
    verify(Plugin.Toasts.claim("profile:x", 1000 + Plugin.Toasts.windowMs))
    compare(Object.keys(Plugin.Toasts.claimed).length, 1)
  }

  function test_anyOpenPanelSuppresses() {
    Plugin.Toasts.watch("", "group:AI")
    Plugin.Toasts.watch("", "group:AI")
    Plugin.Toasts.watch("group:AI", "")
    verify(Plugin.Toasts.onScreen("group:AI"), "the other panel still shows it")
    Plugin.Toasts.watch("group:AI", "group:WARP")
    verify(!Plugin.Toasts.onScreen("group:AI"))
    verify(Plugin.Toasts.onScreen("group:WARP"))
  }
}
