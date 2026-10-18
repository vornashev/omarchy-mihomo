import XCTest
import Foundation
import Darwin
@testable import OMMClient

private final class TestServer: @unchecked Sendable {
    let path = "/tmp/omm-test-\(UUID().uuidString).sock"
    let listener: Int32
    init() throws {
        listener = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
        var address = sockaddr_un(); address.sun_family = sa_family_t(AF_UNIX)
        let bytes = Array(path.utf8CString)
        withUnsafeMutableBytes(of: &address.sun_path) { p in bytes.withUnsafeBytes { p.copyBytes(from: $0) } }
        let result = withUnsafePointer(to: &address) { p in p.withMemoryRebound(to: sockaddr.self, capacity: 1) { Darwin.bind(listener, $0, socklen_t(MemoryLayout<sockaddr_un>.size)) } }
        guard result == 0, Darwin.listen(listener, 2) == 0 else { throw RPCFailure("socket", "Cannot listen") }
    }
    func serve(_ handler: @escaping @Sendable (FileHandle) -> Void) {
        DispatchQueue.global().async { [self] in
            let fd = Darwin.accept(listener, nil, nil)
            guard fd >= 0 else { return }
            let file = FileHandle(fileDescriptor: fd, closeOnDealloc: true)
            handler(file); try? file.close()
        }
    }
    deinit { Darwin.close(listener); unlink(path) }
}
private func readRequests(_ file: FileHandle, count: Int) -> [JSON] {
    var decoder = FrameDecoder(); var frames: [JSON] = []
    while frames.count < count {
        let bytes = file.availableData
        if bytes.isEmpty { break }
        frames += (try? decoder.append(bytes)) ?? []
    }
    return frames
}
final class TransportTests: XCTestCase {
    @MainActor func testOutOfOrderResponsesAndEvents() async throws {
        let server = try TestServer()
        server.serve { file in
            let requests = readRequests(file, count: 2)
            guard requests.count == 2 else { return }
            for request in requests.reversed() {
                let response: JSON = ["id":request["id"],"result":request["method"]]
                var data = try! JSONEncoder().encode(response); data.append(10)
                try? file.write(contentsOf: data)
                try? file.write(contentsOf: Data("{\"event\":\"status\",\"data\":{\"running\":true}}\n".utf8))
            }
            Thread.sleep(forTimeInterval: 0.2)
        }
        let rpc = RPCClient(); var events = 0
        rpc.onEvent = { name, _ in if name == "status" { events += 1 } }
        try rpc.connect(path: server.path)
        async let first = rpc.call("first")
        async let second = rpc.call("second")
        let values = try await [first, second]
        XCTAssertEqual(values, ["first", "second"])
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertEqual(events, 2)
        rpc.disconnect(notify: false)
    }
    @MainActor func testDisconnectFailsOutstandingRequest() async throws {
        let server = try TestServer()
        server.serve { file in _ = readRequests(file, count: 1) }
        let rpc = RPCClient(); try rpc.connect(path: server.path)
        do { _ = try await rpc.call("wait", timeout: 2); XCTFail("expected disconnection") }
        catch let e as RPCFailure { XCTAssertEqual(e.code, "disconnected") }
    }
    @MainActor func testTimeoutDoesNotResendMutation() async throws {
        let server = try TestServer()
        server.serve { file in _ = readRequests(file, count: 1); Thread.sleep(forTimeInterval: 0.3) }
        let rpc = RPCClient(); try rpc.connect(path: server.path)
        do { _ = try await rpc.call("save", timeout: 0.05); XCTFail("expected timeout") }
        catch let e as RPCFailure { XCTAssertEqual(e.code, "timeout") }
        rpc.disconnect(notify: false)
    }
}
