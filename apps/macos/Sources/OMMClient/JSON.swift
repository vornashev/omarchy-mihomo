import Foundation
public enum JSON: Codable, Equatable, Sendable {
    case null, bool(Bool), number(Double), string(String), array([JSON]), object([String: JSON])
    public init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if c.decodeNil() { self = .null }
        else if let v = try? c.decode(Bool.self) { self = .bool(v) }
        else if let v = try? c.decode(Double.self) { self = .number(v) }
        else if let v = try? c.decode(String.self) { self = .string(v) }
        else if let v = try? c.decode([JSON].self) { self = .array(v) }
        else { self = .object(try c.decode([String: JSON].self)) }
    }
    public func encode(to encoder: Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self { case .null: try c.encodeNil(); case .bool(let v): try c.encode(v); case .number(let v): try c.encode(v); case .string(let v): try c.encode(v); case .array(let v): try c.encode(v); case .object(let v): try c.encode(v) }
    }
    public subscript(_ key: String) -> JSON {
        get { object[key] ?? .null }
        set { var v = object; v[key] = newValue; self = .object(v) }
    }
    public var object: [String: JSON] { if case .object(let v) = self { return v }; return [:] }
    public var array: [JSON] { if case .array(let v) = self { return v }; return [] }
    public var string: String { if case .string(let v) = self { return v }; return "" }
    public var bool: Bool { if case .bool(let v) = self { return v }; return false }
    public var number: Double { if case .number(let v) = self { return v }; return 0 }
    public var text: String { if case .string(let v) = self { return v }; return pretty }
    public var pretty: String { let e = JSONEncoder(); e.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]; return String(data: (try? e.encode(self)) ?? Data(), encoding: .utf8) ?? "" }
    public static func parse(_ text: String) throws -> JSON { try JSONDecoder().decode(JSON.self, from: Data(text.utf8)) }
}
extension JSON: ExpressibleByStringLiteral { public init(stringLiteral value: String) { self = .string(value) } }
extension JSON: ExpressibleByBooleanLiteral { public init(booleanLiteral value: Bool) { self = .bool(value) } }
extension JSON: ExpressibleByIntegerLiteral { public init(integerLiteral value: Int) { self = .number(Double(value)) } }
extension JSON: ExpressibleByDictionaryLiteral { public init(dictionaryLiteral elements: (String, JSON)...) { self = .object(Dictionary(uniqueKeysWithValues: elements)) } }

public struct RPCFailure: Error, LocalizedError, Sendable {
    public let code: String
    public let message: String
    public init(_ code: String, _ message: String) { self.code = code; self.message = message }
    public var errorDescription: String? { message }
}
public struct FrameDecoder {
    private var buffer = Data()
    public init() {}
    public mutating func append(_ bytes: Data) throws -> [JSON] {
        buffer.append(bytes)
        var frames: [JSON] = []
        while let newline = buffer.firstIndex(of: 10) {
            let line = buffer[..<newline]
            guard line.count <= 8 * 1024 * 1024 else { throw RPCFailure("protocol", "Frame too large") }
            if !line.isEmpty { frames.append(try JSONDecoder().decode(JSON.self, from: line)) }
            buffer.removeSubrange(...newline)
        }
        if buffer.count > 8 * 1024 * 1024 { throw RPCFailure("protocol", "Frame too large") }
        return frames
    }
}
