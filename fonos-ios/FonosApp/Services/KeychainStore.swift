import Foundation
import Security

/// Simple wrapper around the iOS Keychain for storing string values (e.g., API keys).
///
/// On simulator builds (where Keychain requires code signing entitlements),
/// falls back to an isolated in-memory store so unit tests can run without signing.
struct KeychainStore: Sendable {
    let service: String

    enum KeychainError: LocalizedError {
        case unexpectedStatus(OSStatus)
        case unexpectedData

        var errorDescription: String? {
            switch self {
            case .unexpectedStatus(let status):
                return ".unexpectedStatus(\(status))"
            case .unexpectedData:
                return ".unexpectedData"
            }
        }
    }

    // MARK: - Public API

    /// Stores (or updates) a string value for the given key.
    func set(_ value: String, forKey key: String) throws {
#if targetEnvironment(simulator)
        SimulatorKeychain.shared.set(value, service: service, key: key)
#else
        try keychainSet(value, forKey: key)
#endif
    }

    /// Retrieves a string value for the given key, or `nil` if not found.
    func get(_ key: String) throws -> String? {
#if targetEnvironment(simulator)
        return SimulatorKeychain.shared.get(service: service, key: key)
#else
        return try keychainGet(key)
#endif
    }

    /// Deletes the value for the given key. No-op if the key does not exist.
    func delete(_ key: String) throws {
#if targetEnvironment(simulator)
        SimulatorKeychain.shared.delete(service: service, key: key)
#else
        try keychainDelete(key)
#endif
    }

    // MARK: - Real Keychain (device)

    private func keychainSet(_ value: String, forKey key: String) throws {
        guard let data = value.data(using: .utf8) else {
            throw KeychainError.unexpectedData
        }

        // Attempt update first
        let updateQuery: [CFString: Any] = [
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: service,
            kSecAttrAccount: key
        ]
        let updateAttributes: [CFString: Any] = [
            kSecValueData: data
        ]
        let updateStatus = SecItemUpdate(updateQuery as CFDictionary, updateAttributes as CFDictionary)

        if updateStatus == errSecItemNotFound {
            // Not found — add new item
            let addQuery: [CFString: Any] = [
                kSecClass: kSecClassGenericPassword,
                kSecAttrService: service,
                kSecAttrAccount: key,
                kSecValueData: data,
                kSecAttrAccessible: kSecAttrAccessibleAfterFirstUnlock
            ]
            let addStatus = SecItemAdd(addQuery as CFDictionary, nil)
            guard addStatus == errSecSuccess else {
                throw KeychainError.unexpectedStatus(addStatus)
            }
        } else if updateStatus != errSecSuccess {
            throw KeychainError.unexpectedStatus(updateStatus)
        }
    }

    private func keychainGet(_ key: String) throws -> String? {
        let query: [CFString: Any] = [
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: service,
            kSecAttrAccount: key,
            kSecReturnData: true,
            kSecMatchLimit: kSecMatchLimitOne
        ]
        var result: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &result)

        if status == errSecItemNotFound {
            return nil
        }
        guard status == errSecSuccess else {
            throw KeychainError.unexpectedStatus(status)
        }
        guard let data = result as? Data,
              let string = String(data: data, encoding: .utf8) else {
            throw KeychainError.unexpectedData
        }
        return string
    }

    private func keychainDelete(_ key: String) throws {
        let query: [CFString: Any] = [
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: service,
            kSecAttrAccount: key
        ]
        let status = SecItemDelete(query as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw KeychainError.unexpectedStatus(status)
        }
    }
}

// MARK: - Simulator in-memory Keychain fallback

/// Thread-safe in-memory store used by KeychainStore on the simulator,
/// where the real Keychain requires code-signing entitlements.
///
/// This is only compiled into simulator builds.
#if targetEnvironment(simulator)
final class SimulatorKeychain: @unchecked Sendable {
    static let shared = SimulatorKeychain()
    private var store: [String: String] = [:]
    private let lock = NSLock()

    private init() {}

    func set(_ value: String, service: String, key: String) {
        lock.withLock {
            store["\(service):\(key)"] = value
        }
    }

    func get(service: String, key: String) -> String? {
        lock.withLock {
            store["\(service):\(key)"]
        }
    }

    func delete(service: String, key: String) {
        lock.withLock {
            store.removeValue(forKey: "\(service):\(key)")
        }
    }
}
#endif
