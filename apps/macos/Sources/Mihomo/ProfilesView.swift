import SwiftUI
import AppKit
import OMMClient

struct ProfilesView: View {
    @EnvironmentObject var model: Model
    @State private var adding = false
    @State private var editing: JSON?
    @State private var deleting: JSON?
    var body: some View {
        VStack(alignment: .leading) {
            HStack {
                Button(tr("addProfile")) { adding = true }
                Button(tr("importFile")) {
                    let panel = NSOpenPanel(); panel.allowsMultipleSelection = false
                    if panel.runModal() == .OK, let url = panel.url { model.command("profiles.add", ["path": .string(url.path)]) }
                }
                Button(tr("updateAll")) { model.command("profiles.update") }
                Spacer()
            }.padding()
            List(model.profiles["items"].array) { profile in
                HStack {
                    Image(systemName: profile["id"] == model.profiles["current"] ? "checkmark.circle.fill" : "doc.text").foregroundStyle(profile["id"] == model.profiles["current"] ? Color.accentColor : .secondary)
                    VStack(alignment: .leading) { Text(profile["name"].string).font(.headline); if !profile["last_error"].string.isEmpty { Text(profile["last_error"].string).foregroundStyle(.orange) } }
                    Spacer()
                    Button(tr("select")) { model.command("profiles.select", ["id": profile["id"]]) }
                    Button(tr("update")) { model.command("profiles.update", ["id":profile["id"],"force":true]) }
                    Button(tr("edit")) { editing = profile }
                    Button(role: .destructive) { deleting = profile } label: { Image(systemName: "trash") }
                }.padding(.vertical, 5)
            }
        }
        .sheet(isPresented: $adding) { AddProfileSheet() }
        .sheet(item: $editing) { profile in ProfileEditor(profile: profile) }
        .confirmationDialog(tr("removeProfile"), isPresented: Binding(get: {deleting != nil}, set: {if !$0 {deleting = nil}})) {
            Button(tr("remove"), role: .destructive) { if let p = deleting { model.command("profiles.remove", ["id":p["id"]]) }; deleting = nil }
        }
    }
}
struct AddProfileSheet: View {
    @EnvironmentObject var model: Model
    @Environment(\.dismiss) var dismiss
    @State private var name = ""
    @State private var url = ""
    @State private var yaml = ""
    @State private var remote = true
    @State private var error: String?
    @State private var saving = false
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(tr("addProfile")).font(.title2)
            TextField(tr("name"), text: $name)
            Picker(tr("source"), selection: $remote) { Text(tr("subscription")).tag(true); Text("YAML").tag(false) }.pickerStyle(.segmented)
            if remote { TextField("https://…", text: $url) } else { TextEditor(text: $yaml).font(.system(.body, design: .monospaced)).frame(minHeight: 250) }
            if let error { Text(error).foregroundStyle(.red).textSelection(.enabled) }
            HStack { Button(tr("cancel")) { dismiss() }; Spacer(); Button(tr("add")) {
                saving = true
                Task {
                    do { var p:JSON = remote ? ["url":.string(url)] : ["content":.string(yaml)]; if !name.isEmpty {p["name"] = .string(name)}; _ = try await model.rpc.call("profiles.add", p); await model.refresh(); dismiss() }
                    catch { self.error = error.localizedDescription }; saving = false
                }
            }.buttonStyle(.borderedProminent).disabled(saving || (remote ? url.isEmpty : yaml.isEmpty)) }
        }.padding(24).frame(width: 620)
    }
}
struct ProfileEditor: View {
    @EnvironmentObject var model: Model
    @Environment(\.dismiss) var dismiss
    let profile: JSON
    @State private var text = ""
    @State private var original = ""
    @State private var error: String?
    @State private var saving = false
    private var path: URL { URL(fileURLWithPath: model.profiles["root"].string).appendingPathComponent("profiles").appendingPathComponent(profile["id"].string + ".yaml") }
    var body: some View {
        VStack(alignment: .leading) {
            Text(profile["name"].string).font(.title2)
            Text(tr("subscriptionEditHint")).foregroundStyle(.secondary)
            TextEditor(text: $text).font(.system(.body, design: .monospaced))
            if let error { Text(error).foregroundStyle(.red).textSelection(.enabled) }
            HStack { Button(tr("cancel")) { dismiss() }; Spacer(); Button(tr("save")) {
                saving = true
                Task {
                    do {
                        _ = try await model.rpc.call("profiles.save", ["id":profile["id"],"expected":.string(original),"content":.string(text)])
                        await model.refresh(); dismiss()
                    } catch { self.error = error.localizedDescription }
                    saving = false
                }
            }.disabled(saving).buttonStyle(.borderedProminent) }
        }.padding(24).frame(width: 760, height: 580)
        .task { do { text = try await model.rpc.call("profiles.read", ["id":profile["id"]])["content"].string; original = text } catch { self.error = error.localizedDescription } }
    }
}
