import Foundation
import Darwin

@MainActor public final class RPCClient: ObservableObject {
    @Published public private(set) var connected = false
    public var onEvent: ((String, JSON) -> Void)?
    public var onDisconnect: (() -> Void)?
    private var handle: FileHandle?
    private var decoder = FrameDecoder()
    private var serial = 0
    private var generation = 0
    private var pending: [Int: CheckedContinuation<JSON, Error>] = [:]
    private let writer = DispatchQueue(label: "omm.rpc.writer")
    public init() {}
    public static var socketPath: String { ProcessInfo.processInfo.environment["OMM_SOCKET"] ?? FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".omm/daemon.sock").path }
    public func connect(path: String = RPCClient.socketPath) throws {
        disconnect(notify: false)
        var address = sockaddr_un(); address.sun_family = sa_family_t(AF_UNIX)
        let bytes = Array(path.utf8CString)
        guard bytes.count <= MemoryLayout.size(ofValue: address.sun_path) else { throw RPCFailure("path", "Socket path is too long") }
        withUnsafeMutableBytes(of: &address.sun_path) { destination in bytes.withUnsafeBytes { destination.copyBytes(from: $0) } }
        address.sun_len = UInt8(MemoryLayout<sockaddr_un>.size)
        let fd = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { throw RPCFailure("socket", String(cString: strerror(errno))) }
        var noSigPipe: Int32 = 1
        setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &noSigPipe, socklen_t(MemoryLayout<Int32>.size))
        let result = withUnsafePointer(to: &address) { ptr in ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { Darwin.connect(fd, $0, socklen_t(MemoryLayout<sockaddr_un>.size)) } }
        guard result == 0 else { Darwin.close(fd); throw RPCFailure("connect", "Daemon unavailable. Run packaging/macos/install.sh.") }
        let file = FileHandle(fileDescriptor: fd, closeOnDealloc: true)
        handle = file; connected = true; generation += 1
        let version = generation
        file.readabilityHandler = { [weak self] file in
            let bytes = file.availableData
            DispatchQueue.main.async { [weak self] in
                guard let self, self.generation == version else { return }
                if bytes.isEmpty { self.disconnect(); return }
                do {
                    for frame in try self.decoder.append(bytes) {
                        if !frame["event"].string.isEmpty { self.onEvent?(frame["event"].string, frame["data"]); continue }
                        let id = Int(frame["id"].number)
                        guard let continuation = self.pending.removeValue(forKey: id) else { continue }
                        if frame["error"] != .null { continuation.resume(throwing: RPCFailure(frame["error"]["code"].string, frame["error"]["message"].string)) }
                        else { continuation.resume(returning: frame["result"]) }
                    }
                } catch { self.disconnect() }
            }
        }
    }
    public func disconnect(notify: Bool = true) {
        generation += 1
        handle?.readabilityHandler = nil
        try? handle?.close(); handle = nil; connected = false; decoder = FrameDecoder()
        let requests = pending; pending.removeAll()
        for request in requests.values { request.resume(throwing: RPCFailure("disconnected", "Connection to daemon closed")) }
        if notify { onDisconnect?() }
    }
    public func call(_ method: String, _ params: JSON = [:], timeout: TimeInterval = 75) async throws -> JSON {
        guard let handle, connected else { throw RPCFailure("disconnected", "Daemon unavailable") }
        serial += 1; let id = serial; let version = generation
        let frame: JSON = ["id": .number(Double(id)), "method": .string(method), "params": params]
        var data = try JSONEncoder().encode(frame); data.append(10)
        return try await withCheckedThrowingContinuation { continuation in
            pending[id] = continuation
            let bytes = data
            writer.async { [weak self] in
                do { try handle.write(contentsOf: bytes) }
                catch { DispatchQueue.main.async { [weak self] in if self?.generation == version { self?.disconnect() } } }
            }
            Task { [weak self] in
                try? await Task.sleep(for: .seconds(timeout))
                guard let self, self.generation == version, let request = self.pending.removeValue(forKey: id) else { return }
                request.resume(throwing: RPCFailure("timeout", "The operation timed out. Refresh its status before retrying."))
            }
        }
    }
}
