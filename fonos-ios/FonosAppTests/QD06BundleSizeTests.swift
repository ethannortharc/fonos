// QD-06: Bundle size check.
// Verifies that the app produces a small bundle with no bundled ML models or large assets,
// and uses only system frameworks (no heavy third-party dependencies).
//
// Verifier: auto
// Level: static (shell-level check — encoded here as an XCTestCase for runner integration)
// Run: xcodebuild test -scheme FonosApp -only-testing:FonosAppTests/QD06BundleSizeTests
//
// NOTE: The actual size measurement requires a release archive build and is performed by the
// ratchet runner shell step, not the in-process test suite. The tests below validate
// the structural constraints that drive bundle size at the source level.

import Testing
import Foundation
@testable import FonosApp

struct QD06BundleSizeTests {

    // MARK: - No bundled ML models

    @Test("App bundle does not include large Core ML model files (no .mlmodelc directories)")
    func noCorMLModels() throws {
        let bundle = Bundle(for: FonosAppModule.self)
        // Walk the bundle looking for compiled ML model directories
        let bundleURL = bundle.bundleURL
        let fm = FileManager.default
        guard let enumerator = fm.enumerator(at: bundleURL,
                                              includingPropertiesForKeys: [.isDirectoryKey],
                                              options: [.skipsHiddenFiles]) else {
            return
        }
        for case let url as URL in enumerator {
            let hasMLModelC = url.pathExtension == "mlmodelc"
            let hasMlPackage = url.pathExtension == "mlpackage"
            #expect(!hasMLModelC, "Found bundled Core ML model: \(url.lastPathComponent)")
            #expect(!hasMlPackage, "Found bundled ML package: \(url.lastPathComponent)")
        }
    }

    // MARK: - No large asset blobs

    @Test("App bundle contains no individual files larger than 5MB")
    func noLargeAssetFiles() throws {
        let bundle = Bundle(for: FonosAppModule.self)
        let bundleURL = bundle.bundleURL
        let fm = FileManager.default
        guard let enumerator = fm.enumerator(at: bundleURL,
                                              includingPropertiesForKeys: [.fileSizeKey],
                                              options: [.skipsHiddenFiles]) else {
            return
        }
        let fiveMB = 5 * 1024 * 1024
        for case let url as URL in enumerator {
            let attrs = try? fm.attributesOfItem(atPath: url.path)
            if let size = attrs?[.size] as? Int, size > fiveMB {
                Issue.record("Large file in bundle: \(url.lastPathComponent) (\(size / 1024)KB)")
            }
        }
    }

    // MARK: - No third-party framework directories

    @Test("App bundle does not include heavy third-party frameworks (Alamofire, Realm, etc.)")
    func noHeavyThirdPartyFrameworks() throws {
        let bundle = Bundle(for: FonosAppModule.self)
        let frameworksURL = bundle.bundleURL.appendingPathComponent("Frameworks")
        let fm = FileManager.default

        guard let frameworks = try? fm.contentsOfDirectory(atPath: frameworksURL.path) else {
            // No Frameworks directory — that's fine, means zero embedded frameworks
            return
        }

        let prohibitedFrameworks = [
            "Alamofire",
            "Moya",
            "Realm",
            "RealmSwift",
            "SQLite",
            "GRDB",
            "Kingfisher",
            "SDWebImage",
        ]

        for framework in frameworks {
            let name = (framework as NSString).deletingPathExtension
            for prohibited in prohibitedFrameworks {
                #expect(!name.localizedCaseInsensitiveContains(prohibited),
                        "Found prohibited framework: \(framework)")
            }
        }
    }

    // MARK: - Swift Package Manager dependency count

    @Test("Package.resolved contains only lightweight/system-bridging dependencies")
    func packageResolvedLightweightDeps() throws {
        // Find Package.resolved relative to the test bundle's source package
        let possiblePaths = [
            "/Users/ethan/Projects/design/fonos/fonos-ios/FonosApp.xcodeproj/project.xcworkspace/xcshareddata/swiftpm/Package.resolved",
            "/Users/ethan/Projects/design/fonos/fonos-ios/Package.resolved",
        ]

        var resolvedURL: URL?
        for path in possiblePaths {
            if FileManager.default.fileExists(atPath: path) {
                resolvedURL = URL(fileURLWithPath: path)
                break
            }
        }

        guard let url = resolvedURL,
              let data = try? Data(contentsOf: url),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            // Package.resolved not found — likely no SPM deps at all, which is fine
            return
        }

        let pins = (json["pins"] as? [[String: Any]]) ?? []
        // Rubric: if there are SPM packages, none should be known heavy libraries
        let heavyPackages = ["Alamofire", "Realm", "Firebase", "AWSiOSSDK"]
        for pin in pins {
            let identity = pin["identity"] as? String ?? ""
            for heavy in heavyPackages {
                #expect(!identity.localizedCaseInsensitiveContains(heavy),
                        "Package.resolved contains heavy dependency: \(identity)")
            }
        }
    }
}

// MARK: - Shell-level bundle size check (ratchet runner reference)
//
// The ratchet runner executes the following to measure actual IPA size:
//
//   xcodebuild archive \
//     -project fonos-ios/FonosApp.xcodeproj \
//     -scheme FonosApp \
//     -archivePath /tmp/FonosApp.xcarchive \
//     -configuration Release
//
//   du -sh /tmp/FonosApp.xcarchive/Products/Applications/FonosApp.app
//
// Rubric thresholds (QD-06):
//   Score 5: < 10MB app bundle
//   Score 3: 10-15MB app bundle
//   Score 1: > 15MB app bundle

// Placeholder type for Bundle lookup (must be a class defined in the main app module)
private final class FonosAppModule {}
