import QtQuick
import QtTest
import "../../plugin" as Plugin

Rectangle {
  id: scene
  width: 760; height: 620
  color: "#1e1e2e"
  QtObject {
    id: service
    property color red: "#f38ba8"
    property var calls: []
    property var info: null
    function request(m, params, cb) {
      calls = calls.concat([{ method: m, params: params }])
      if (m === "dns.query") cb({ Status: 0, Answer: [{ data: "104.18.32.47" }, { data: "172.64.155.209" }] }, null)
      else if (cb) cb(info, null)
    }
  }
  QtObject {
    id: panel
    property string notification: ""
    function notify(text) { notification = text }
  }
  Plugin.DnsSettings {
    id: page
    x: 20; y: 20; width: 720; height: 580
    svc: service
    panel: panel
  }
  TestCase {
    name: "DnsSettings"
    when: windowShown

    function init() {
      Plugin.MihomoI18n.language = "ru"
      service.calls = []
      service.info = {
        settings: { "enhanced-mode": null, nameserver: null, fallback: null, "default-nameserver": null, "fake-ip-filter": [], policy: [{ domain: "+.x.org", server: "https://dns.quad9.net/dns-query" }], sniffer: null },
        effective: { dns: { "enhanced-mode": "redir-host", nameserver: ["https://one.one.one.one/dns-query"], "fake-ip-filter": ["+.lan", "+.local"] }, sniffer: true },
        doh: "https://one.one.one.one/dns-query"
      }
      page.reload()
      waitForRendering(page)
    }
    function last() { return service.calls[service.calls.length - 1] }

    function test_profile_values_are_placeholders() {
      var servers = findChild(page, "dnsServers")
      compare(servers.text, "")
      compare(servers.profile, "https://one.one.one.one/dns-query")
      compare(findChild(page, "dnsModeSeg").current, "", "the profile decides the mode")
      grabImage(scene).save("/tmp/omm-dns-settings.png")
    }
    function test_list_override_and_reset() {
      var servers = findChild(page, "dnsServers")
      servers.submit("https://a/dns-query, 1.1.1.1")
      compare(last().method, "dns.set")
      compare(last().params.nameserver, ["https://a/dns-query", "1.1.1.1"])
      compare(last().params.policy.length, 1, "other values are kept")
      servers.submit("  ")
      compare(last().params.nameserver, null, "empty hands it back to the profile")
    }
    function test_remove_policy_and_lookup() {
      mouseClick(findChild(page, "dnsPolicyRemove0"))
      compare(last().params.policy.length, 0)
      var q = findChild(page, "dnsQueryField")
      q.text = "chatgpt.com"
      mouseClick(findChild(page, "dnsQueryBtn"))
      compare(last().params, { name: "chatgpt.com", type: "A" })
      compare(findChild(page, "dnsAnswer").text, "104.18.32.47\n172.64.155.209")
    }
  }
}
