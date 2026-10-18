import XCTest
import ProxySettings
final class ProxySettingsTests: XCTestCase {
    func testRestoresPACAndExceptions() {
        let before: [String: Any] = ["ProxyAutoConfigEnable": 1, "ProxyAutoConfigURLString": "https://example.test/proxy.pac", "ExceptionsList": ["localhost", "*.local"]]
        let applied = target(before, port: 7897)
        XCTAssertEqual(applied["ProxyAutoConfigEnable"] as? Int, 0)
        let result = restored(applied, before: before, applied: applied)
        XCTAssertTrue(NSDictionary(dictionary: result).isEqual(to: before))
    }
    func testPreservesEditedEndpointsAndRestoresOtherProtocols() {
        let before: [String: Any] = ["HTTPEnable": 1, "HTTPProxy": "office", "HTTPPort": 3128]
        let applied = target(before, port: 7897)
        var external = applied; external["HTTPProxy"] = "new-office"
        let result = restored(external, before: before, applied: applied)
        XCTAssertEqual(result["HTTPProxy"] as? String, "new-office")
        XCTAssertEqual(result["HTTPPort"] as? Int, 7897)
        XCTAssertNil(result["SOCKSProxy"])
    }
    func testPreservesValuesNotOwnedByTheApp() {
        let before: [String: Any] = ["ExcludeSimpleHostnames": 1]
        var applied = target(before, port: 7897)
        applied["ExceptionsList"] = ["new.example"]
        let result = restored(applied, before: before, applied: target(before, port: 7897))
        XCTAssertEqual(result["ExceptionsList"] as? [String], ["new.example"])
    }
}
