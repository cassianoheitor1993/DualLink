// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "PoCVirtualDisplayApp",
    platforms: [.macOS(.v14)],
    targets: [
        // ObjC helper — provides DualLinkCreateVirtualDisplayMode()
        .target(
            name: "VirtualDisplayHelper",
            path: "ObjCHelper",
            publicHeadersPath: "include"
        ),
        // Main Swift executable
        .executableTarget(
            name: "PoCVirtualDisplayApp",
            dependencies: ["VirtualDisplayHelper"],
            path: "Sources"
        )
    ]
)
