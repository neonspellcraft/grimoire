// swift-tools-version:6.0
import PackageDescription

let package = Package(
    name: "Grimoire",
    platforms: [.macOS(.v14)],
    targets: [
        // Pure model logic (playlist, m3u, time math), kept separate so it can be unit tested.
        .target(name: "GrimoireCore"),
        .executableTarget(name: "Grimoire", dependencies: ["GrimoireCore"]),
        .testTarget(name: "GrimoireTests", dependencies: ["GrimoireCore"]),
    ],
    swiftLanguageModes: [.v5]
)
