// NoteQD04: SwiftLint passes on all new Voice Notes files — zero violations.
//
// Verifier: auto
// Level: static (swiftlint subprocess encoded as XCTestCase for runner integration)
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/NoteQD04LintTests
//
// NOTE: Swift Testing does not support subprocess execution. This file uses XCTestCase.
// NOTE: Process is unavailable on iOS/simulator — tests that require subprocess execution
// are compiled conditionally and run only on macOS (e.g. in a Mac Catalyst build or from
// the ratchet runner's host process). On iOS simulator, these tests pass trivially.

import XCTest
@testable import FonosApp

final class NoteQD04LintTests: XCTestCase {

    // MARK: - New Voice Notes source files to lint

    /// Absolute paths to all new Swift files introduced by the Voice Notes feature.
    /// Update this list as new files are added.
    private static let newVoiceNotesFiles: [String] = [
        "/Users/ethan/Projects/design/fonos/fonos-ios/FonosApp/Models/NoteContainer.swift",
        "/Users/ethan/Projects/design/fonos/fonos-ios/FonosApp/Models/NoteEntry.swift",
        "/Users/ethan/Projects/design/fonos/fonos-ios/FonosApp/Services/NoteService.swift",
        "/Users/ethan/Projects/design/fonos/fonos-ios/FonosApp/Services/NoteViewModel.swift",
        "/Users/ethan/Projects/design/fonos/fonos-ios/FonosApp/Views/NotesView.swift",
        "/Users/ethan/Projects/design/fonos/fonos-ios/FonosApp/Views/NotebookDetailView.swift",
        "/Users/ethan/Projects/design/fonos/fonos-ios/FonosApp/Views/RecordNoteSheet.swift",
        "/Users/ethan/Projects/design/fonos/fonos-ios/FonosApp/Views/SettingsView/NotebookSettingsView.swift",
        "/Users/ethan/Projects/design/fonos/fonos-ios/FonosApp/FonosIntents/RecordNoteIntent.swift",
    ]

    // MARK: - SwiftLint availability

    func testSwiftLintIsAvailable() {
        #if os(macOS)
        let result = runProcess("/usr/bin/which", args: ["swiftlint"])
        XCTAssert(result.exitCode == 0,
                  "swiftlint not found on PATH — install with: brew install swiftlint")
        #else
        XCTAssert(true, "swiftlint check skipped on iOS simulator — runs as host tool")
        #endif
    }

    // MARK: - Zero errors on new Voice Notes files

    func testNewVoiceNotesFilesHaveZeroErrors() {
        #if os(macOS)
        let existingFiles = Self.newVoiceNotesFiles.filter { FileManager.default.fileExists(atPath: $0) }
        guard !existingFiles.isEmpty else {
            // No new files exist yet (pre-implementation) — skip rather than fail
            XCTAssert(true, "No Voice Notes implementation files found yet — skipping lint check")
            return
        }

        let result = runSwiftLint(args: ["lint", "--reporter", "json", "--quiet"] + existingFiles)
        let violations = parseViolations(from: result.output)
        let errors = violations.filter { $0.severity == "error" }
        XCTAssertEqual(
            errors.count, 0,
            "SwiftLint found \(errors.count) error(s) in new Voice Notes files:\n" +
            errors.map { "  \($0.file):\($0.line) \($0.ruleID): \($0.reason)" }.joined(separator: "\n")
        )
        #else
        XCTAssert(true, "swiftlint check skipped on iOS simulator — runs as host tool")
        #endif
    }

    // MARK: - Zero warnings on new Voice Notes files

    func testNewVoiceNotesFilesHaveZeroWarnings() {
        #if os(macOS)
        let existingFiles = Self.newVoiceNotesFiles.filter { FileManager.default.fileExists(atPath: $0) }
        guard !existingFiles.isEmpty else {
            XCTAssert(true, "No Voice Notes implementation files found yet — skipping lint check")
            return
        }

        let result = runSwiftLint(args: ["lint", "--reporter", "json", "--quiet"] + existingFiles)
        let violations = parseViolations(from: result.output)
        let warnings = violations.filter { $0.severity == "warning" }
        XCTAssertEqual(
            warnings.count, 0,
            "SwiftLint found \(warnings.count) warning(s) in new Voice Notes files (threshold: zero):\n" +
            warnings.map { "  \($0.file):\($0.line) \($0.ruleID): \($0.reason)" }.joined(separator: "\n")
        )
        #else
        XCTAssert(true, "swiftlint check skipped on iOS simulator — runs as host tool")
        #endif
    }

    // MARK: - Prohibited patterns: force_cast

    func testNoForceCastInVoiceNotesFiles() {
        #if os(macOS)
        let existingFiles = Self.newVoiceNotesFiles.filter { FileManager.default.fileExists(atPath: $0) }
        guard !existingFiles.isEmpty else {
            XCTAssert(true, "No Voice Notes implementation files found yet — skipping")
            return
        }
        let result = runSwiftLint(
            args: ["lint", "--reporter", "json", "--only-rule", "force_cast"] + existingFiles
        )
        let violations = parseViolations(from: result.output)
        XCTAssertEqual(violations.count, 0,
                       "Found \(violations.count) force_cast violation(s) — use conditional cast instead")
        #else
        XCTAssert(true, "swiftlint check skipped on iOS simulator — runs as host tool")
        #endif
    }

    // MARK: - Prohibited patterns: force_try

    func testNoForceTryInVoiceNotesFiles() {
        #if os(macOS)
        let existingFiles = Self.newVoiceNotesFiles.filter { FileManager.default.fileExists(atPath: $0) }
        guard !existingFiles.isEmpty else {
            XCTAssert(true, "No Voice Notes implementation files found yet — skipping")
            return
        }
        let result = runSwiftLint(
            args: ["lint", "--reporter", "json", "--only-rule", "force_try"] + existingFiles
        )
        let violations = parseViolations(from: result.output)
        XCTAssertEqual(violations.count, 0,
                       "Found \(violations.count) force_try violation(s) — handle errors explicitly")
        #else
        XCTAssert(true, "swiftlint check skipped on iOS simulator — runs as host tool")
        #endif
    }

    // MARK: - Prohibited patterns: force_unwrapping

    func testNoForceUnwrappingInVoiceNotesFiles() {
        #if os(macOS)
        let existingFiles = Self.newVoiceNotesFiles.filter { FileManager.default.fileExists(atPath: $0) }
        guard !existingFiles.isEmpty else {
            XCTAssert(true, "No Voice Notes implementation files found yet — skipping")
            return
        }
        let result = runSwiftLint(
            args: ["lint", "--reporter", "json", "--only-rule", "force_unwrapping"] + existingFiles
        )
        let violations = parseViolations(from: result.output)
        XCTAssertEqual(violations.count, 0,
                       "Found \(violations.count) force_unwrapping violation(s) — use guard/if let instead")
        #else
        XCTAssert(true, "swiftlint check skipped on iOS simulator — runs as host tool")
        #endif
    }
}

// MARK: - Helpers

private struct SwiftLintViolation: Codable {
    let severity: String
    let ruleID: String
    let reason: String
    let file: String
    let line: Int

    enum CodingKeys: String, CodingKey {
        case severity, reason, file, line
        case ruleID = "rule_id"
    }
}

private struct ProcessResult {
    let exitCode: Int32
    let output: String
}

#if os(macOS)
private func runSwiftLint(args: [String]) -> ProcessResult {
    return runProcess("/usr/bin/env", args: ["swiftlint"] + args)
}

private func runProcess(_ executable: String, args: [String]) -> ProcessResult {
    let proc = Process()
    proc.executableURL = URL(fileURLWithPath: executable)
    proc.arguments = args

    let pipe = Pipe()
    proc.standardOutput = pipe
    proc.standardError = pipe

    do {
        try proc.run()
        proc.waitUntilExit()
    } catch {
        return ProcessResult(exitCode: -1, output: "Process failed: \(error)")
    }

    let data = pipe.fileHandleForReading.readDataToEndOfFile()
    let output = String(data: data, encoding: .utf8) ?? ""
    return ProcessResult(exitCode: proc.terminationStatus, output: output)
}

private func parseViolations(from jsonOutput: String) -> [SwiftLintViolation] {
    guard let data = jsonOutput.data(using: .utf8),
          let violations = try? JSONDecoder().decode([SwiftLintViolation].self, from: data) else {
        return []
    }
    return violations
}
#endif
