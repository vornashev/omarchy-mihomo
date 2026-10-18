import XCTest
@testable import OMMClient
final class ProtocolTests: XCTestCase {
    func testPartialAndInterleavedFrames() throws {
        var decoder = FrameDecoder()
        XCTAssertEqual(try decoder.append(Data("{\"id\":2,".utf8)), [])
        let frames = try decoder.append(Data("\"result\":true}\n{\"event\":\"status\",\"data\":{}}\n{\"id\":1,\"error\":{\"code\":\"conflict\"}}\n".utf8))
        XCTAssertEqual(frames.count, 3)
        XCTAssertEqual(frames[0]["id"], 2)
        XCTAssertEqual(frames[1]["event"], "status")
        XCTAssertEqual(frames[2]["error"]["code"], "conflict")
    }
    func testNestedJSONRoundTripAndUnknownFields() throws {
        let value = try JSON.parse("{\"helper\":{\"available\":true},\"future\":[null,1,\"ru\"]}")
        XCTAssertEqual(try JSON.parse(value.pretty), value)
        XCTAssertEqual(value["missing"], .null)
    }
}
