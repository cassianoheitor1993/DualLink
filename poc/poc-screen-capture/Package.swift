// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "PoCScreenCapture",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(
            name: "PoCScreenCapture",
            path: ".",
            sources: ["main.swift"]
        )
    ]
)
