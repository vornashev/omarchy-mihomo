import SwiftUI
import AppKit
import OMMClient

func tr(_ key: String) -> String { NSLocalizedString(key, bundle: .module, comment: "") }
extension JSON: Identifiable {
    public var id: String { if self["id"] != .null { return self["id"].text }; if self["name"] != .null { return self["provider"].string + "/" + self["name"].string }; if self["host"] != .null { return self["host"].string }; return pretty }
}
enum Page: String, CaseIterable, Identifiable {
    case overview, profiles, proxies, rules, problems, connections, logs, settings
    var id: String { rawValue }
    var title: String { tr(rawValue) }
    var icon: String { switch self { case .overview: "gauge.with.dots.needle.50percent"; case .profiles: "doc.text"; case .proxies: "network"; case .rules: "arrow.triangle.branch"; case .problems: "exclamationmark.triangle"; case .connections: "point.3.connected.trianglepath.dotted"; case .logs: "text.alignleft"; case .settings: "gearshape" } }
}
@MainActor final class Model: ObservableObject {
    let rpc = RPCClient()
    @Published var connected = false
    @Published var status: JSON = [:]
    @Published var traffic: JSON = [:]
    @Published var profiles: JSON = [:]
    @Published var groups: JSON = .array([])
    @Published var nodes: JSON = [:]
    @Published var groupDefinitions: JSON = [:]
    @Published var rules: JSON = [:]
    @Published var problems: JSON = [:]
    @Published var connections: JSON = [:]
    @Published var settings: JSON = [:]
    @Published var catalog: JSON = [:]
    @Published var logs: [JSON] = []
    @Published var error: String?
    @Published var busy = false
    @Published var page: Page = .overview { didSet { Task { await refresh() } } }
    @Published var badge = 0
    private var stopping = false
    private var reconnecting = false
    private var timer: Task<Void, Never>?
    init() {
        rpc.onEvent = { [weak self] event, data in self?.event(event, data) }
        rpc.onDisconnect = { [weak self] in
            guard let self else { return }; self.connected = false
            if !self.stopping { Task { await self.connect() } }
        }
        NSWorkspace.shared.notificationCenter.addObserver(forName: NSWorkspace.didWakeNotification, object: nil, queue: .main) { [weak self] _ in
            Task { @MainActor in
                guard let self else { return }
                if self.connected { await self.refresh() } else { await self.connect() }
            }
        }
        Task { await connect() }
        timer = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(4))
                guard let self else { return }
                if self.connected && !self.busy { await self.refresh(quiet: true) }
            }
        }
    }
    var running: Bool { status["core"]["running"].bool }
    func connect() async {
        guard !reconnecting && !stopping else { return }; reconnecting = true
        defer { reconnecting = false }
        while !stopping {
            do {
                try rpc.connect()
                let first = try await rpc.call("status")
                guard first["protocol"].number == 1 else { throw RPCFailure("version", tr("protocolMismatch")) }
                _ = try await rpc.call("session.open")
                _ = try await rpc.call("events.subscribe", ["topics": .array(["status", "problems", "traffic", "logs", "profiles"])])
                status = first; connected = true; error = nil
                catalog = try await rpc.call("proxy_catalog.get")
                await refresh()
                // Restore explicit user intent only after a successful handshake.
                if UserDefaults.standard.bool(forKey: "sessionWanted"), !running, first["helper"]["available"].bool {
                    _ = try await rpc.call("session.start")
                }
                return
            } catch {
                self.error = error.localizedDescription; rpc.disconnect(notify: false); connected = false
                try? await Task.sleep(for: .seconds(3))
            }
        }
    }
    func event(_ event: String, _ data: JSON) {
        switch event {
        case "status": status = data
        case "traffic": traffic = data
        case "problems": badge = Int(data["counts"]["new"].number)
        case "logs":
            if data["lines"] != .null { logs = data["lines"].array }
            else { logs.append(data) }
            if logs.count > 400 { logs.removeFirst(logs.count - 400) }
        case "profiles": Task { await refresh(quiet: true) }
        default: break
        }
    }
    func refresh(quiet: Bool = false) async {
        guard connected else { return }
        do {
            status = try await rpc.call("status", timeout: 8)
            switch page {
            case .overview: profiles = try await rpc.call("profiles.list"); if running { groups = try await rpc.call("proxies.list") }
            case .profiles: profiles = try await rpc.call("profiles.list")
            case .proxies:
                nodes = try await rpc.call("proxy_nodes.list"); groupDefinitions = try await rpc.call("proxy_groups.list")
                if running { groups = try await rpc.call("proxies.list") }
            case .rules: rules = try await rpc.call("rules.list"); nodes = try await rpc.call("proxy_nodes.list")
            case .problems: problems = try await rpc.call("problems.list")
            case .connections: if running { connections = try await rpc.call("connections.list") }
            case .settings: settings = try await rpc.call("settings.get")
            case .logs: break
            }
        } catch { if !quiet { self.error = error.localizedDescription } }
    }
    func perform(_ action: @escaping () async throws -> Void) {
        guard !busy else { return }; busy = true; error = nil
        Task {
            defer { busy = false }
            do { try await action(); await refresh() } catch { self.error = error.localizedDescription }
        }
    }
    func command(_ method: String, _ params: JSON = [:]) { perform { _ = try await self.rpc.call(method, params) } }
    func toggleSession() {
        let enable = !running
        perform {
            _ = try await self.rpc.call(enable ? "session.start" : "session.stop")
            UserDefaults.standard.set(enable, forKey: "sessionWanted")
        }
    }
    func quit() {
        guard !stopping else { return }; stopping = true; timer?.cancel()
        Task {
            // Preserve wanted settings for the next launch; stop releases actual network state.
            if connected { _ = try? await rpc.call("session.stop", timeout: 12) }
            rpc.disconnect(notify: false)
            (NSApplication.shared.delegate as? AppDelegate)?.terminating = true
            NSApplication.shared.terminate(nil)
        }
    }
}
