import Foundation
import SystemConfiguration
import ProxySettings
import Darwin

struct ProxyFailure: Error, CustomStringConvertible { let description: String }
func require(_ ok: Bool, _ message: String) throws {
    if !ok { throw ProxyFailure(description: "\(message): \(String(cString: SCErrorString(SCError())))") }
}
func writeJournal(_ records: [String: Any], url: URL) throws {
    let data = try JSONSerialization.data(withJSONObject: records, options: [.sortedKeys])
    try data.write(to: url, options: .atomic)
    guard chmod(url.path, 0o600) == 0 else { throw ProxyFailure(description: "Cannot protect proxy journal") }
}
func run() throws {
    guard geteuid() == 0, CommandLine.arguments.count == 4 else { throw ProxyFailure(description: "Installed helper must invoke this tool as root") }
    let mode = CommandLine.arguments[1], port = Int(CommandLine.arguments[2]) ?? 0
    guard ["enable", "sync", "restore"].contains(mode), mode == "restore" || (1...65535).contains(port) else { throw ProxyFailure(description: "Invalid proxy operation") }
    let journal = URL(fileURLWithPath: CommandLine.arguments[3])
    var records: [String: [String: Any]] = [:]
    if FileManager.default.fileExists(atPath: journal.path) {
        guard let decoded = try JSONSerialization.jsonObject(with: Data(contentsOf: journal)) as? [String: [String: Any]] else { throw ProxyFailure(description: "Invalid recovery journal; refusing to overwrite it") }
        records = decoded
    }
    guard let preferences = SCPreferencesCreate(nil, "Omarchy Mihomo" as CFString, nil) else { throw ProxyFailure(description: "Cannot open network preferences") }
    try require(SCPreferencesLock(preferences, true), "Cannot lock network preferences")
    defer { SCPreferencesUnlock(preferences) }
    guard let services = SCNetworkServiceCopyAll(preferences) as? [SCNetworkService] else { throw ProxyFailure(description: "Cannot read network services") }
    var writes: [(SCNetworkProtocol, [String: Any], [String: Any])] = []
    var enabled = false
    var conflict = false
    for service in services {
        guard let serviceID = SCNetworkServiceGetServiceID(service) else { continue }
        let id = serviceID as String
        guard let proto = SCNetworkServiceCopyProtocol(service, kSCNetworkProtocolTypeProxies) else { continue }
        let current = SCNetworkProtocolGetConfiguration(proto) as? [String: Any] ?? [:]
        let record = records[id]
        let before = record?["before"] as? [String: Any] ?? current
        let applied = record?["applied"] as? [String: Any] ?? [:]
        if mode == "restore" {
            if record != nil && !applied.isEmpty { writes.append((proto, current, restored(current, before: before, applied: applied))) }
            continue
        }
        guard SCNetworkServiceGetEnabled(service) else { continue }
        if mode == "sync", record?["blocked"] as? Bool == true { conflict = true; continue }
        if mode == "sync", !applied.isEmpty, !keys.allSatisfy({ same(current[$0], applied[$0]) }) {
            let restore = restored(current, before: before, applied: applied)
            writes.append((proto, current, restore))
            records[id] = ["before": before, "applied": applied, "blocked": true]
            conflict = true
            continue
        }
        let original: [String: Any]
        if mode == "enable" && !applied.isEmpty && !keys.allSatisfy({ same(current[$0], applied[$0]) }) { original = current }
        else { original = before }
        let desired = target(current, port: port)
        records[id] = ["before": original, "applied": desired]
        if !NSDictionary(dictionary: current).isEqual(to: desired) { writes.append((proto, current, desired)) }
        enabled = true
    }
    // Persist recovery intent before changing any network preference.
    if mode != "restore" { try writeJournal(records, url: journal) }
    if !writes.isEmpty {
        do {
            for (proto, _, value) in writes { try require(SCNetworkProtocolSetConfiguration(proto, value as CFDictionary), "Cannot set proxy") }
            try require(SCPreferencesCommitChanges(preferences), "Cannot save proxy")
            try require(SCPreferencesApplyChanges(preferences), "Cannot apply proxy")
        } catch {
            for (proto, old, _) in writes { _ = SCNetworkProtocolSetConfiguration(proto, old as CFDictionary) }
            _ = SCPreferencesCommitChanges(preferences); _ = SCPreferencesApplyChanges(preferences)
            throw error
        }
    }
    if mode == "restore" && !records.isEmpty { try writeJournal([:], url: journal) }
    let result: [String: Any] = ["enabled": enabled && mode != "restore", "error": conflict ? "Network proxy settings were changed outside Mihomo; those services were released." : NSNull()]
    print(String(data: try JSONSerialization.data(withJSONObject: result), encoding: .utf8)!)
}
do { try run() } catch { fputs("\(error)\n", stderr); exit(1) }
