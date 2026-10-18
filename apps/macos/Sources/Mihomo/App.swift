import SwiftUI
import AppKit
import OMMClient

final class AppDelegate: NSObject, NSApplicationDelegate {
    var model: Model?
    var terminating = false
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { false }
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        if terminating { return .terminateNow }
        terminating = true
        Task { @MainActor in model?.quit() }
        return .terminateCancel
    }
}
@main struct MihomoApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var delegate
    @StateObject private var model = Model()
    var body: some Scene {
        Window(tr("appName"), id: "main") {
            MainView().environmentObject(model).onAppear { delegate.model = model }
        }.defaultSize(width: 1080, height: 720)
        .commands {
            CommandGroup(replacing: .appTermination) { Button(tr("quit")) { model.quit() }.keyboardShortcut("q") }
        }
        MenuBarExtra {
            MenuContent().environmentObject(model)
        } label: {
            Label(model.badge > 0 ? "\(model.badge)" : "Mihomo", systemImage: model.running ? "network.badge.shield.half.filled" : "network")
        }
    }
}
struct MenuContent: View {
    @EnvironmentObject var model: Model
    @Environment(\.openWindow) var openWindow
    var body: some View {
        Text(model.running ? tr("connected") : tr("disconnected"))
        Button(tr("openWindow")) { openWindow(id: "main"); NSApp.activate(ignoringOtherApps: true) }
        Divider()
        Button(model.running ? tr("disconnect") : tr("connect")) { model.toggleSession() }.disabled(!model.connected || model.busy)
        Toggle(tr("tun"), isOn: Binding(get: {model.status["core"]["tun"].bool}, set: {model.command("tun.set", ["enable": .bool($0)])})).disabled(!model.running)
        Toggle(tr("systemProxy"), isOn: Binding(get: {model.status["system_proxy"]["enabled"].bool}, set: {model.command("system_proxy.set", ["enable": .bool($0)])})).disabled(!model.running)
        Divider()
        Button(tr("quit")) { model.quit() }
    }
}
struct MainView: View {
    @EnvironmentObject var model: Model
    var body: some View {
        NavigationSplitView {
            List(Page.allCases, selection: Binding<Page?>(get: {model.page}, set: {if let p = $0 {model.page = p}})) { page in
                Label(page.title, systemImage: page.icon).tag(page)
            }.navigationSplitViewColumnWidth(min: 170, ideal: 190)
        } detail: {
            VStack(spacing: 0) {
                if let error = model.error {
                    HStack { Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange); Text(error).textSelection(.enabled); Spacer(); Button { model.error = nil } label: { Image(systemName: "xmark") } }.padding().background(.orange.opacity(0.1))
                }
                if !model.connected { ContentUnavailableView(tr("daemonUnavailable"), systemImage: "network.slash", description: Text(tr("installHint"))) }
                else {
                    switch model.page {
                    case .overview: OverviewView()
                    case .profiles: ProfilesView()
                    case .proxies: ProxiesView()
                    case .rules: RulesView()
                    case .problems: ProblemsView()
                    case .connections: ConnectionsView()
                    case .logs: LogsView()
                    case .settings: SettingsView()
                    }
                }
            }.navigationTitle(model.page.title)
            .toolbar {
                ToolbarItem { if model.busy { ProgressView().controlSize(.small) } }
                ToolbarItem { Button { Task { await model.refresh() } } label: { Image(systemName: "arrow.clockwise") }.help(tr("refresh")) }
            }
        }.frame(minWidth: 820, minHeight: 550)
    }
}
struct OverviewView: View {
    @EnvironmentObject var model: Model
    var body: some View {
        Form {
            Section {
                LabeledContent(tr("status"), value: model.running ? tr("connected") : tr("disconnected"))
                LabeledContent(tr("profile"), value: model.status["config"]["profile"]["name"].string)
                HStack { Button(model.running ? tr("disconnect") : tr("connect")) { model.toggleSession() }.buttonStyle(.borderedProminent).disabled(model.busy); Spacer(); Text("↓ \(Int(model.traffic["down"].number / 1024)) KB/s   ↑ \(Int(model.traffic["up"].number / 1024)) KB/s").monospacedDigit() }
            }
            Section(tr("network")) {
                Picker(tr("mode"), selection: Binding(get: {model.status["core"]["mode"].string}, set: {model.command("mode.set", ["mode": .string($0)])})) { Text(tr("ruleMode")).tag("rule"); Text(tr("globalMode")).tag("global"); Text(tr("directMode")).tag("direct") }.disabled(!model.running)
                Toggle(tr("tun"), isOn: Binding(get: {model.status["core"]["tun"].bool}, set: {model.command("tun.set", ["enable": .bool($0)])})).disabled(!model.running)
                Toggle(tr("systemProxy"), isOn: Binding(get: {model.status["system_proxy"]["enabled"].bool}, set: {model.command("system_proxy.set", ["enable": .bool($0)])})).disabled(!model.running)
            }
            if !model.status["issues"].array.isEmpty { Section(tr("attention")) { ForEach(model.status["issues"].array) { issue in Text(issue["detail"].string.isEmpty ? issue["id"].string : issue["detail"].string).foregroundStyle(.orange) } } }
            if model.running { RuntimeGroups() }
        }.formStyle(.grouped)
    }
}
struct RuntimeGroups: View {
    @EnvironmentObject var model: Model
    var body: some View {
        ForEach(model.groups.array) { group in
            Section(group["name"].string) {
                Picker(tr("proxy"), selection: Binding(get: {group["now"].string}, set: {model.command("proxies.select", ["group":group["name"],"name":.string($0)])})) {
                    ForEach(group["members"].array) { node in Text(node["name"].string + (node["delay"] == .null ? "" : " · \(Int(node["delay"].number)) ms")).tag(node["name"].string) }
                }.disabled(group["kind"].string.lowercased() != "selector")
            }
        }
    }
}
