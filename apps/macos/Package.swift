// swift-tools-version: 5.10
import PackageDescription
let package = Package(
    name: "OmarchyMihomo",
    defaultLocalization: "en",
    platforms: [.macOS(.v14)],
    products: [.executable(name: "Mihomo", targets: ["Mihomo"]), .executable(name: "omm-system-proxy", targets: ["SystemProxy"])],
    targets: [
        .target(name: "OMMClient"),
        .target(name: "ProxySettings"),
        .executableTarget(name: "Mihomo", dependencies: ["OMMClient"], resources: [.process("Resources")]),
        .executableTarget(name: "SystemProxy", dependencies: ["ProxySettings"]),
        .testTarget(name: "OMMClientTests", dependencies: ["OMMClient", "ProxySettings"])
    ]
)
