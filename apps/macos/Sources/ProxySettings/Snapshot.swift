import Foundation
public let keys = ["HTTPEnable", "HTTPProxy", "HTTPPort", "HTTPSEnable", "HTTPSProxy", "HTTPSPort", "SOCKSEnable", "SOCKSProxy", "SOCKSPort", "ProxyAutoConfigEnable", "ProxyAutoDiscoveryEnable"]
public func same(_ a: Any?, _ b: Any?) -> Bool { NSDictionary(dictionary: ["v": a ?? NSNull()]).isEqual(to: ["v": b ?? NSNull()]) }
public func target(_ original: [String: Any], port: Int) -> [String: Any] {
    var config = original
    for prefix in ["HTTP", "HTTPS", "SOCKS"] { config["\(prefix)Enable"] = 1; config["\(prefix)Proxy"] = "127.0.0.1"; config["\(prefix)Port"] = port }
    config["ProxyAutoConfigEnable"] = 0; config["ProxyAutoDiscoveryEnable"] = 0
    return config
}
public func restored(_ current: [String: Any], before: [String: Any], applied: [String: Any]) -> [String: Any] {
    var result = current
    for key in keys where same(current[key], applied[key]) { result[key] = before[key] }
    return result
}
