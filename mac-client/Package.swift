// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "DualLink",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .executable(name: "DualLink", targets: ["DualLinkApp"]),
    ],
    dependencies: [
        // WebRTC — adicionar quando iniciar Fase 1
        // .package(url: "https://github.com/stasel/WebRTC.git", from: "114.0.0"),
    ],
    targets: [
        // MARK: — App Entry Point
        .executableTarget(
            name: "DualLinkApp",
            dependencies: [
                "DualLinkCore",
                "VirtualDisplay",
                "ScreenCapture",
                "VideoEncoder",
            ],
            path: "Sources/DualLinkApp",
            resources: [
                .process("Resources"),
            ]
        ),

        // MARK: — Core (tipos compartilhados, config, errors)
        .target(
            name: "DualLinkCore",
            path: "Sources/DualLinkCore"
        ),

        // MARK: — Virtual Display ObjC Helper (CGVirtualDisplayMode creation)
        // Needed because initWithWidth:height:refreshRate: takes primitive types
        // that can't be bridged via Swift's perform() selector API.
        .target(
            name: "VirtualDisplayObjC",
            path: "Sources/VirtualDisplayObjC",
            publicHeadersPath: "include"
        ),

        // MARK: — Virtual Display (CGVirtualDisplay)
        .target(
            name: "VirtualDisplay",
            dependencies: ["DualLinkCore", "VirtualDisplayObjC"],
            path: "Sources/VirtualDisplay"
        ),

        // MARK: — Screen Capture (ScreenCaptureKit)
        .target(
            name: "ScreenCapture",
            dependencies: ["DualLinkCore"],
            path: "Sources/ScreenCapture"
        ),

        // MARK: — Video Encoder (VideoToolbox H.264/H.265)
        .target(
            name: "VideoEncoder",
            dependencies: ["DualLinkCore"],
            path: "Sources/VideoEncoder"
        ),

        // MARK: — Streaming (WebRTC) — placeholder para Fase 1
        .target(
            name: "Streaming",
            dependencies: ["DualLinkCore"],
            path: "Sources/Streaming"
        ),

        // MARK: — Signaling (WebSocket) — placeholder para Fase 1
        .target(
            name: "Signaling",
            dependencies: ["DualLinkCore"],
            path: "Sources/Signaling"
        ),

        // MARK: — Discovery (mDNS/Bonjour)
        .target(
            name: "Discovery",
            dependencies: ["DualLinkCore"],
            path: "Sources/Discovery"
        ),

        // MARK: — Input Injection (CGEvent) — placeholder para Fase 2
        .target(
            name: "InputInjection",
            dependencies: ["DualLinkCore"],
            path: "Sources/InputInjection"
        ),

        // MARK: — Tests
        .testTarget(
            name: "DualLinkTests",
            dependencies: [
                "DualLinkCore",
                "VirtualDisplay",
                "ScreenCapture",
                "VideoEncoder",
            ],
            path: "Tests/DualLinkTests"
        ),
    ]
)
