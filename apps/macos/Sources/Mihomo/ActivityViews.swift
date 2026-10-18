import SwiftUI
import AppKit
import ServiceManagement
import OMMClient

struct ProblemsView: View {
    @EnvironmentObject var model: Model
    @State private var search = ""
    @State private var rule: JSON?
    @State private var ignores: [JSON] = []
    @State private var showingIgnored = false
    var body: some View {
        VStack {
            HStack { Button(tr("markSeen")) { model.command("problems.mark_seen") }; Button(tr("hidden")) { Task { do { ignores = try await model.rpc.call("problems.ignores").array; showingIgnored = true } catch {model.error = error.localizedDescription} } }; Spacer(); TextField(tr("search"), text: $search).frame(width: 230) }.padding()
            List(model.problems["items"].array.filter {search.isEmpty || $0.pretty.localizedCaseInsensitiveContains(search)}) { item in
                HStack(alignment: .top) {
                    Image(systemName: item["new"].bool ? "circle.fill" : "circle").foregroundStyle(.orange).font(.caption)
                    VStack(alignment: .leading, spacing: 4) { Text(item["host"].string).font(.headline); Text(item["last_error"].string).font(.caption).foregroundStyle(.secondary).textSelection(.enabled); Text("\(Int(item["count"].number)) · " + item["apps"].object.keys.sorted().joined(separator: ", ")).font(.caption) }
                    Spacer()
                    Button(tr("route")) { rule = ["kind":"domain-suffix", "value":item["host"],"target":"DIRECT"] }
                    Button(tr("hide")) { model.command("problems.hide", ["kind":"host","value":item["host"]]) }
                }.padding(.vertical, 5)
            }
        }.sheet(item: $rule) { RuleEditor(rule: $0) }
        .sheet(isPresented: $showingIgnored) { VStack { Text(tr("hidden")).font(.title2); List(ignores) { item in HStack { Text(item["value"].string); Spacer(); Button(tr("restore")) { model.command("problems.unhide", ["id":item["id"]]); ignores.removeAll {$0["id"] == item["id"]} } } }; Button(tr("close")) {showingIgnored = false} }.padding().frame(width: 550, height: 400) }
    }
}
struct ConnectionsView: View {
    @EnvironmentObject var model: Model
    @State private var search = ""
    @State private var rule: JSON?
    var body: some View {
        VStack {
            HStack { TextField(tr("search"), text: $search); Spacer(); Button(tr("closeAll"), role: .destructive) {model.command("connections.close")} }.padding()
            List(model.connections["connections"].array.filter {search.isEmpty || $0.pretty.localizedCaseInsensitiveContains(search)}) { item in
                HStack {
                    let process = item["metadata"]["processPath"].string
                    if !process.isEmpty { Image(nsImage: NSWorkspace.shared.icon(forFile: process)).resizable().frame(width: 24, height: 24) }
                    VStack(alignment: .leading) { Text(item["metadata"]["host"].string.isEmpty ? item["metadata"]["destinationIP"].string : item["metadata"]["host"].string).font(.headline); Text(item["metadata"]["process"].string + " · " + item["chains"].array.map(\.string).joined(separator: " → ")).foregroundStyle(.secondary) }
                    Spacer()
                    Text("↓ \(Int(item["download"].number / 1024)) KB  ↑ \(Int(item["upload"].number / 1024)) KB").font(.caption).monospacedDigit()
                    Button(tr("route")) { rule = ["kind":"domain", "value":item["metadata"]["host"], "target":"DIRECT"] }
                    Button(tr("close")) {model.command("connections.close", ["id":item["id"]])}
                }
            }
        }.sheet(item: $rule) { RuleEditor(rule: $0) }
    }
}
struct LogsView: View {
    @EnvironmentObject var model: Model
    @State private var search = ""
    var body: some View {
        VStack {
            HStack { TextField(tr("search"), text: $search); Button(tr("copy")) { NSPasteboard.general.clearContents(); NSPasteboard.general.setString(lines.joined(separator: "\n"), forType: .string) }; Button(tr("clear")) {model.logs = []} }.padding()
            ScrollView { LazyVStack(alignment: .leading, spacing: 6) { ForEach(Array(lines.enumerated()), id: \.offset) { _, line in Text(line).font(.system(.caption, design: .monospaced)).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading) } }.padding() }
        }
    }
    private var lines: [String] { model.logs.map { value in value["payload"].string.isEmpty ? value.pretty : "[\(value["type"].string)] \(value["payload"].string)" }.filter {search.isEmpty || $0.localizedCaseInsensitiveContains(search)} }
}
struct SettingsView: View {
    @EnvironmentObject var model: Model
    @State private var draft: JSON = [:]
    @State private var loaded = false
    @State private var login = SMAppService.mainApp.status == .enabled
    @State private var extra = ""
    var body: some View {
        Form {
            Section(tr("application")) {
                Toggle(tr("launchAtLogin"), isOn: Binding(get: {login}, set: { wanted in
                    do { if wanted {try SMAppService.mainApp.register()} else {try SMAppService.mainApp.unregister()}; login = wanted } catch {model.error = error.localizedDescription}
                }))
                Text(tr("quitHint")).foregroundStyle(.secondary)
            }
            if loaded {
                Section(tr("network")) {
                    TextField(tr("mixedPort"), value: Binding(get: {Int(draft["mixed-port"].number)}, set: {draft["mixed-port"] = .number(Double($0))}), format: .number)
                    Toggle(tr("allowLAN"), isOn: Binding(get: {draft["allow-lan"].bool}, set: {draft["allow-lan"] = .bool($0)}))
                    Toggle("IPv6", isOn: Binding(get: {draft["ipv6"].bool}, set: {draft["ipv6"] = .bool($0)}))
                    Picker(tr("tunStack"), selection: Binding(get: {draft["tun"]["stack"].string}, set: {draft["tun"]["stack"] = .string($0)})) { ForEach(["gvisor", "system", "mixed"], id: \.self) {Text($0).tag($0)} }
                    Picker(tr("logLevel"), selection: Binding(get: {draft["log-level"].string}, set: {draft["log-level"] = .string($0)})) { ForEach(["debug", "info", "warning", "error", "silent"], id: \.self) {Text($0).tag($0)} }
                }
                Section(tr("advanced")) {
                    Text(tr("extraSettingsHint")).foregroundStyle(.secondary)
                    TextEditor(text: $extra).font(.system(.caption, design: .monospaced)).frame(height: 150)
                }
                Button(tr("save")) {
                    model.perform {
                        draft["extra"] = try JSON.parse(extra)
                        _ = try await model.rpc.call("settings.set", draft)
                    }
                }.buttonStyle(.borderedProminent)
            }
            Section(tr("installation")) {
                Text(tr("updateHint")).textSelection(.enabled)
                Button(tr("openLogs")) { NSWorkspace.shared.open(FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Logs/omarchy-mihomo")) }
            }
        }.formStyle(.grouped)
        .task { do { draft = try await model.rpc.call("settings.get"); extra = draft["extra"].pretty; loaded = true } catch { model.error = error.localizedDescription } }
    }
}
