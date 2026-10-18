import SwiftUI
import OMMClient
struct RulesView: View {
    @EnvironmentObject var model: Model
    @State private var editor: JSON?
    @State private var history = false
    @State private var search = ""
    var body: some View {
        VStack {
            HStack { Button(tr("addRule")) { editor = [:] }; Button(tr("history")) { history = true }; Spacer(); TextField(tr("search"), text: $search).frame(width: 230) }.padding()
            List(model.rules["items"].array.filter {search.isEmpty || $0.pretty.localizedCaseInsensitiveContains(search)}) { rule in
                HStack {
                    Toggle("", isOn: Binding(get: {rule["enabled"].bool}, set: {model.command("rules.update", ["id":rule["id"],"enabled":.bool($0)])})).labelsHidden()
                    VStack(alignment: .leading) { Text(rule["value"].string).font(.headline); Text(rule["kind"].string + " → " + rule["target"].string).foregroundStyle(rule["orphan"].bool ? .orange : .secondary) }
                    Spacer()
                    Button(tr("edit")) { editor = rule }
                    Button(role: .destructive) { model.command("rules.delete", ["id":rule["id"]]) } label: { Image(systemName: "trash") }
                }
            }
        }.sheet(item: $editor) { RuleEditor(rule: $0) }.sheet(isPresented: $history) { RuleHistory() }
    }
}
struct RuleEditor: View {
    @EnvironmentObject var model: Model
    @Environment(\.dismiss) var dismiss
    var rule: JSON = [:]
    @State private var kind = "domain-suffix"
    @State private var value = ""
    @State private var target = "DIRECT"
    @State private var note = ""
    @State private var preview = ""
    @State private var error: String?
    @State private var busy = false
    @State private var reconnect = true
    @State private var lifetime = "forever"
    @State private var targets = ["DIRECT", "REJECT"]
    @State private var suggestions: [JSON] = []
    private let kinds = ["domain", "domain-suffix", "domain-keyword", "domain-regex", "geosite", "geoip", "ip-cidr", "ip-suffix", "process-name", "process-path", "process-name-regex", "process-path-regex", "dst-port", "src-port", "network", "and", "or", "not"]
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(tr("routingRule")).font(.title2)
            Form {
                Picker(tr("match"), selection: $kind) { ForEach(kinds, id: \.self) { Text($0).tag($0) } }
                TextField(tr("value"), text: $value)
                Picker(tr("target"), selection: $target) { ForEach(targets, id: \.self) { Text($0).tag($0) } }
                TextField(tr("note"), text: $note)
                Picker(tr("lifetime"), selection: $lifetime) { Text(tr("forever")).tag("forever"); Text(tr("untilReboot")).tag("boot") }
                Toggle(tr("reconnectMatches"), isOn: $reconnect)
            }
            HStack {
                Button(tr("suggest")) { act { suggestions = (try await model.rpc.call("rules.suggest", ["kind":.string(kind),"query":.string(value)]))["items"].array } }
                Button(tr("preview")) { act { preview = (try await model.rpc.call("rules.preview", params)).pretty } }
                Button(tr("testRoute")) { act { preview = (try await model.rpc.call("rules.test", ["host":.string(value),"targets":.array([.string(target)])])).pretty } }
            }
            if !suggestions.isEmpty { ScrollView { VStack(alignment: .leading) { ForEach(suggestions) { item in Button(item["value"].string) { value = item["value"].string; suggestions = [] } } } }.frame(maxHeight: 100) }
            if !preview.isEmpty { ScrollView { Text(preview).font(.system(.caption, design: .monospaced)).textSelection(.enabled) }.frame(maxHeight: 150) }
            if let error { Text(error).foregroundStyle(.red) }
            HStack { Button(tr("cancel")) { dismiss() }; Spacer(); Button(tr("save")) { act { _ = try await model.rpc.call("rules.save", params); await model.refresh(); dismiss() } }.buttonStyle(.borderedProminent).disabled(busy || value.isEmpty) }
        }.padding(24).frame(width: 620)
        .task {
            if !rule["kind"].string.isEmpty { kind = rule["kind"].string }; value = rule["value"].string
            if !rule["target"].string.isEmpty { target = rule["target"].string }; note = rule["note"].string
            if let nodes = try? await model.rpc.call("proxy_nodes.list") { targets = Array(Set(nodes["targets"].array.map(\.string) + [target])).sorted() }
        }
    }
    private var params: JSON {
        var p:JSON = ["kind":.string(kind),"value":.string(value),"target":.string(target),"note":.string(note),"reconnect":.bool(reconnect),"lifetime": ["until_reboot":.bool(lifetime == "boot")]]
        if rule["id"] != .null { p["replace"] = rule["id"] }
        return p
    }
    private func act(_ f: @escaping () async throws -> Void) { busy = true; error = nil; Task { do { try await f() } catch { self.error = error.localizedDescription }; busy = false } }
}
struct RuleHistory: View {
    @EnvironmentObject var model: Model
    @Environment(\.dismiss) var dismiss
    @State private var entries: [JSON] = []
    @State private var error: String?
    var body: some View {
        VStack {
            Text(tr("history")).font(.title2)
            List(entries) { item in HStack { Text(item.pretty).font(.system(.caption, design: .monospaced)); Spacer(); Button(tr("rollback")) { Task { do { _ = try await model.rpc.call("rules.rollback", ["to":item["id"]]); await model.refresh(); dismiss() } catch { self.error = error.localizedDescription } } } } }
            if let error { Text(error).foregroundStyle(.red) }
            Button(tr("close")) { dismiss() }
        }.padding(24).frame(width: 700, height: 480)
        .task { do { entries = try await model.rpc.call("rules.history", ["limit":50]).array } catch { self.error = error.localizedDescription } }
    }
}
