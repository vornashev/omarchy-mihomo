import SwiftUI
import OMMClient
struct ProxiesView: View {
    @EnvironmentObject var model: Model
    @State private var tab = 0
    @State private var editor: JSON?
    @State private var importing = false
    @State private var provider: JSON?
    @State private var search = ""
    private var collection: JSON { tab == 1 ? model.groupDefinitions : model.nodes }
    var body: some View {
        VStack {
            HStack {
                Picker("", selection: $tab) { Text(tr("nodes")).tag(0); Text(tr("groups")).tag(1); Text(tr("quickSwitch")).tag(2) }.pickerStyle(.segmented).frame(width: 370)
                Spacer()
                if tab != 2 { Button(tr("add")) { editor = [:] }; Button(tr("import")) { importing = true } }
            }.padding()
            if tab == 2 { Form { RuntimeGroups() }.formStyle(.grouped) }
            else {
                TextField(tr("search"), text: $search).padding(.horizontal)
                List {
                    ForEach(collection["items"].array.filter {search.isEmpty || $0.pretty.localizedCaseInsensitiveContains(search)}) { node in
                        HStack {
                            VStack(alignment: .leading) { Text(node["name"].string).font(.headline); Text([node["type"].string,node["server"].string,node["source"].string].filter {!$0.isEmpty}.joined(separator: " · ")).foregroundStyle(.secondary) }
                            Spacer()
                            if node["delay"] != .null { Text("\(Int(node["delay"].number)) ms").monospacedDigit() }
                            Button(tr("test")) { model.command("proxies.delay", ["name":node["name"]]) }.disabled(!model.running)
                            Button(tr("edit")) { editor = node }
                        }.padding(.vertical, 4)
                    }
                    if tab == 0 && !collection["providers"].array.isEmpty {
                        Section(tr("providers")) { ForEach(collection["providers"].array, id: \.string) { name in Button(name.string) { provider = ["name":name] } } }
                    }
                    if !collection["removed"].array.isEmpty { Section(tr("removed")) { ForEach(collection["removed"].array, id: \.string) { name in Button(tr("restore") + " " + name.string) { var p = context; p["name"] = name; model.command(tab == 1 ? "proxy_groups.reset" : "proxy_nodes.reset", p) } } } }
                }
            }
        }.sheet(item: $editor) { node in ProxyEditor(node: node, group: tab == 1) }
        .sheet(isPresented: $importing) { ProxyImportSheet(context: model.nodes) }
        .sheet(item: $provider) { ProviderEditor(name: $0["name"].string) }
    }
    private var context: JSON { ["profile_id": collection["profile_id"], "revision":collection["revision"]] }
}
struct ProxyEditor: View {
    @EnvironmentObject var model: Model
    @Environment(\.dismiss) var dismiss
    let node: JSON
    let group: Bool
    @State private var context: JSON = [:]
    @State private var definition: JSON = [:]
    @State private var yaml = ""
    @State private var raw = false
    @State private var advanced = false
    @State private var preview = ""
    @State private var error: String?
    @State private var busy = false
    @State private var history: [JSON] = []
    @State private var members: Set<String> = []
    @State private var replacement = ""
    private var prefix: String { group ? "proxy_groups" : "proxy_nodes" }
    private var schemas: [JSON] { model.catalog[group ? "groups" : "nodes"].array }
    private var fields: [JSON] { schemas.first { $0["type"] == definition["type"] }?["fields"].array ?? [] }
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack { Text(node["name"].string.isEmpty ? tr("addProxy") : node["name"].string).font(.title2); Spacer(); Toggle("YAML", isOn: Binding(get: {raw}, set: {changeMode($0)})).toggleStyle(.switch) }
            if raw { TextEditor(text: $yaml).font(.system(.body, design: .monospaced)) }
            else {
                Form {
                    Picker(tr("type"), selection: Binding(get: {definition["type"].string}, set: {definition["type"] = .string($0)})) { ForEach(schemas, id: \.schemaType) { Text($0["type"].string).tag($0["type"].string) } }
                    Toggle(tr("advanced"), isOn: $advanced)
                    ForEach(fields.filter {advanced || $0["required"].bool}, id: \.schemaKey) { field in SchemaField(field: field, object: $definition) }
                    if !group { Section(tr("memberOf")) { ForEach(context["groups"].array, id: \.string) { name in Toggle(name.string, isOn: Binding(get: {members.contains(name.string)}, set: {if $0 {members.insert(name.string)} else {members.remove(name.string)}})) } } }
                }.formStyle(.grouped)
            }
            if !history.isEmpty { DisclosureGroup(tr("history")) { ScrollView { VStack(alignment: .leading) { ForEach(history) { item in Button(item["at"].text) { definition = item["definition"]; raw = false; preview = "" } } } }.frame(maxHeight: 90) } }
            if !preview.isEmpty { ScrollView { Text(preview).font(.system(.caption, design: .monospaced)).textSelection(.enabled) }.frame(maxHeight: 140) }
            if let error { Text(error).foregroundStyle(.red).textSelection(.enabled) }
            if !node["name"].string.isEmpty { HStack { TextField(tr("replacement"), text: $replacement); Button(tr("remove"), role: .destructive) { mutate("remove") }; Button(tr("reset")) { mutate("reset") }.disabled(busy) }
            HStack {
                Button(tr("cancel")) { dismiss() }; Spacer()
                Button(tr("preview")) { act { var p = params; p["operation"] = "save"; p["group"] = .bool(group); preview = (try await model.rpc.call("proxy_config.preview", p)).pretty } }
                Button(tr("save")) { mutate("save") }.buttonStyle(.borderedProminent).disabled(busy || context["revision"] == .null)
            }
        }.padding(24).frame(width: 760, height: 650)
        .task {
            do {
                if node["name"].string.isEmpty {
                    context = try await model.rpc.call(prefix + ".list")
                    definition = group ? ["name":"", "type":"select", "proxies":.array(["DIRECT"])] : ["name":"", "type":"socks5", "server":"", "port":1080]
                } else {
                    context = try await model.rpc.call(prefix + ".get", ["name":node["name"],"provider":node["provider"]])
                    definition = context["items"].array.first?["definition"] ?? [:]
                    yaml = context["items"].array.first?["yaml"].string ?? ""
                    members = Set(context["member_of"].array.map(\.string))
                    history = (try? await model.rpc.call(prefix + ".history", ["profile_id":context["profile_id"],"name":node["name"]]).array) ?? []
                }
            } catch { self.error = error.localizedDescription }
        }
    }
    private var params: JSON {
        var p:JSON = ["profile_id":context["profile_id"],"revision":context["revision"],"name":node["name"],"member_of":.array(members.sorted().map(JSON.string))]
        if raw {p["yaml"] = .string(yaml)} else {p["definition"] = definition}
        if !replacement.isEmpty {p["replacement"] = .string(replacement)}
        return p
    }
    private func changeMode(_ value: Bool) {
        act { let response = try await model.rpc.call("proxy_config.convert", raw ? ["yaml":.string(yaml)] : ["definition":definition]); definition = response["definition"]; yaml = response["yaml"].string; raw = value }
    }
    private func mutate(_ operation: String) { act { _ = try await model.rpc.call(prefix + "." + operation, params); await model.refresh(); dismiss() } }
    private func act(_ f: @escaping () async throws -> Void) { busy = true; error = nil; Task { do { try await f() } catch { self.error = error.localizedDescription }; busy = false } }
}
struct SchemaField: View {
    let field: JSON
    @Binding var object: JSON
    @State private var structured = ""
    private var key: String { field["key"].string }
    private var label: String { key + (field["required"].bool ? " *" : "") }
    var body: some View {
        Group {
            if field["kind"].string == "boolean" {
                Toggle(label, isOn: Binding(get: {object[key].bool}, set: {object[key] = .bool($0)}))
            } else if !field["choices"].array.isEmpty {
                Picker(label, selection: Binding(get: {object[key].string}, set: {object[key] = .string($0)})) { Text("—").tag(""); ForEach(field["choices"].array, id: \.string) { Text($0.string).tag($0.string) } }
            } else if field["kind"].string == "number" {
                TextField(label, value: Binding(get: {object[key].number}, set: {object[key] = .number($0)}), format: .number)
            } else if field["kind"].string == "object" {
                DisclosureGroup(label) { ForEach(field["fields"].array, id: \.schemaKey) { child in AnyView(SchemaField(field: child, object: Binding(get: {object[key]}, set: {object[key] = $0}))) } }
            } else if ["map", "array", "list", "json"].contains(field["kind"].string) {
                VStack(alignment: .leading) {
                    Text(label + " (JSON)")
                    TextField("[] / {}", text: $structured, axis: .vertical).font(.system(.caption, design: .monospaced))
                        .onAppear { structured = object[key] == .null ? "" : object[key].pretty }
                        .onChange(of: structured) { _, text in if text.isEmpty { var values = object.object; values.removeValue(forKey: key); object = .object(values) } else {object[key] = (try? JSON.parse(text)) ?? .string(text)} }
                    if !structured.isEmpty && (try? JSON.parse(structured)) == nil { Text(tr("invalidJSON")).foregroundStyle(.red) }
                }
            } else if field["secret"].bool && !field["multiline"].bool {
                SecureField(label, text: Binding(get: {object[key].string}, set: {object[key] = .string($0)}))
            } else {
                TextField(label, text: Binding(get: {object[key].string}, set: {object[key] = .string($0)}), axis: field["multiline"].bool ? .vertical : .horizontal)
            }
        }
    }
}
struct ProxyImportSheet: View {
    @EnvironmentObject var model: Model
    @Environment(\.dismiss) var dismiss
    let context: JSON
    @State private var text = ""
    @State private var items: [JSON] = []
    @State private var error: String?
    @State private var busy = false
    var body: some View {
        VStack(alignment: .leading) {
            Text(tr("importProxies")).font(.title2)
            TextEditor(text: $text).font(.system(.body, design: .monospaced)).onChange(of: text) { _, _ in items = [] }
            if !items.isEmpty { List(items) { item in VStack(alignment: .leading) { Text(item["definition"]["name"].string); ForEach(item["warnings"].array, id: \.string) { Text($0.string).foregroundStyle(.orange) }; if !item["error"].string.isEmpty {Text(item["error"].string).foregroundStyle(.red)} } }.frame(height: 150) }
            if let error { Text(error).foregroundStyle(.red) }
            HStack { Button(tr("cancel")) { dismiss() }; Spacer(); Button(tr("preview")) { act { let result = try await model.rpc.call("proxy_import.preview", ["content":.string(text)]); items = result.array } }; Button(tr("import")) { act { _ = try await model.rpc.call("proxy_import.commit", ["profile_id":context["profile_id"],"revision":context["revision"],"items":.array(items.compactMap { $0["definition"] == .null ? nil : $0["definition"] })]); await model.refresh(); dismiss() } }.disabled(items.isEmpty || items.contains {$0["error"] != .null} || busy) }
        }.padding(24).frame(width: 700, height: 530)
    }
    private func act(_ f: @escaping () async throws -> Void) { busy = true; Task { do { try await f() } catch { self.error = error.localizedDescription }; busy = false } }
}
struct ProviderEditor: View {
    @EnvironmentObject var model: Model
    @Environment(\.dismiss) var dismiss
    let name: String
    @State private var context: JSON = [:]
    @State private var text = ""
    @State private var error: String?
    var body: some View {
        VStack { Text(name).font(.title2); TextEditor(text: $text).font(.system(.body, design: .monospaced)); if let error {Text(error).foregroundStyle(.red)}; HStack { Button(tr("cancel")) {dismiss()}; Spacer(); Button(tr("save")) { Task { do { let definition = try await model.rpc.call("proxy_config.convert", ["yaml":.string(text)]); _ = try await model.rpc.call("proxy_providers.save", ["profile_id":context["profile_id"],"revision":context["revision"],"name":.string(name),"definition":definition["definition"]]); await model.refresh(); dismiss() } catch {self.error = error.localizedDescription} } } } }
        .padding(24).frame(width: 700, height: 500)
        .task { do { context = try await model.rpc.call("proxy_providers.get", ["name":.string(name)]); text = try await model.rpc.call("proxy_config.convert", ["definition":context["definition"]])["yaml"].string } catch {self.error = error.localizedDescription} }
    }
}

extension JSON { var schemaType: String { self["type"].string }; var schemaKey: String { self["key"].string } }
