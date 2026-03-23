import Testing
@testable import FonosApp

/// Scaffold test suite — individual test classes go in separate files.
struct FonosAppTests {
    @Test func appConfigDefaultValues() {
        let config = AppConfig()
        #expect(config.sttProvider == "apple")
        #expect(config.defaultMode.id == "raw")
        #expect(config.recordMode == .tap)
    }

    @Test func modeBuiltInCount() {
        #expect(Mode.builtInModes.count == 5)
    }

    @Test func rawModeDoesNotRequireLLM() {
        let raw = Mode.builtInModes.first { $0.id == "raw" }
        #expect(raw != nil)
        #expect(raw?.requiresLLM == false)
    }

    @Test func llmModesRequireLLM() {
        let llmModes = Mode.builtInModes.filter { $0.id != "raw" }
        #expect(llmModes.allSatisfy { $0.requiresLLM })
    }
}
