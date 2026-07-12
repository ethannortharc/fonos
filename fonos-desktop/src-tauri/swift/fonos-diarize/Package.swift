// swift-tools-version:5.10
// Swift 5 语言模式（工具链 6.x 可编译）：避免 Swift 6 严格并发对
// progressHandler 闭包内计数器的 Sendable 报错；FluidAudio 自身的
// tools-version 与此无关，SPM 各包独立。
import PackageDescription

let package = Package(
    name: "fonos-diarize",
    // NOTE: brief specified .v13; resolved FluidAudio 0.15.5 (satisfies
    // "from: 0.12.4") declares `platforms: [.macOS(.v14), .iOS(.v17)]` in its
    // own Package.swift, so SPM rejects .v13 here ("depends on the product
    // 'FluidAudio' which requires macos 14.0"). Bumped to .v14 — the minimum
    // version the resolved dependency actually requires. Recorded in task-1
    // report; CLI protocol/behavior unaffected.
    platforms: [.macOS(.v14)],
    dependencies: [
        .package(url: "https://github.com/FluidInference/FluidAudio.git", from: "0.12.4")
    ],
    targets: [
        .executableTarget(
            name: "fonos-diarize",
            dependencies: [.product(name: "FluidAudio", package: "FluidAudio")]
        )
    ]
)
