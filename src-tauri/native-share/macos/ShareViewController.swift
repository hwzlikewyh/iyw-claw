import AppKit
import Darwin
import UniformTypeIdentifiers

private let maxFileBytes: Int64 = 512 * 1024 * 1024
private let maxBatchFiles = 32

private struct ShareItem: Encodable, Sendable {
    let name: String
    let relativePath: String
    let size: Int64
}

private struct ShareBatch: Encodable {
    let schemaVersion = 1
    let id: String
    let createdAt: Date
    let items: [ShareItem]
}

@objc(IywShareViewController)
final class ShareViewController: NSViewController {
    private var started = false

    override func loadView() {
        view = NSView(frame: NSRect(x: 0, y: 0, width: 1, height: 1))
        view.alphaValue = 0
        preferredContentSize = .zero
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        guard !started, let context = extensionContext else { return }
        started = true
        Task { @MainActor in await receive(context) }
    }

    @MainActor
    private func receive(_ context: NSExtensionContext) async {
        var staging: URL?
        var phase = "input"
        do {
            let providers = (context.inputItems as? [NSExtensionItem] ?? [])
                .flatMap { $0.attachments ?? [] }
            guard !providers.isEmpty, providers.count <= maxBatchFiles else {
                throw failure("一次需要分享 1 到 32 个文件。")
            }
            phase = "shared-container"
            let root = try inboxRoot()
            let id = UUID().uuidString.lowercased()
            let directory = root.appendingPathComponent("Staging/\(id)")
            staging = directory
            try FileManager.default.createDirectory(at: directory.appendingPathComponent("files"),
                withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
            try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: directory.path)
            var items: [ShareItem] = []
            phase = "copy"
            for (index, provider) in providers.enumerated() {
                items.append(try await copyProvider(provider, directory: directory, index: index))
                guard items.reduce(Int64(0), { $0 + $1.size }) <= maxFileBytes else {
                    throw failure("分享文件总量超过 512 MiB。")
                }
            }
            phase = "commit"
            try commit(ShareBatch(id: id, createdAt: Date(), items: items), directory: directory, root: root)
            staging = nil
            launchMain()
            context.completeRequest(returningItems: nil, completionHandler: nil)
        } catch {
            let reason = error as NSError
            NSLog("原助理分享失败 phase=%@ domain=%@ code=%ld", phase, reason.domain, reason.code)
            if let staging { try? FileManager.default.removeItem(at: staging) }
            context.cancelRequest(withError: failure("无法完整接收分享，原文件仍保留在微信中，请重试或在原助理中导入。"))
        }
    }

    nonisolated private func inboxRoot() throws -> URL {
        let group = Bundle.main.object(forInfoDictionaryKey: "IywShareAppGroup") as? String ?? ""
        if !group.isEmpty {
            guard let container = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: group) else {
                throw failure("无法访问原助理的共享容器。")
            }
            return container.appendingPathComponent("share-inbox")
        }
        guard let user = getpwuid(getuid()), let home = user.pointee.pw_dir else {
            throw failure("无法定位当前用户目录。")
        }
        return URL(fileURLWithPath: String(cString: home))
            .appendingPathComponent("Library/Application Support/app.iywclaw/share-inbox")
    }

    nonisolated private func copyProvider(_ provider: NSItemProvider, directory: URL, index: Int) async throws -> ShareItem {
        let types = provider.registeredTypeIdentifiers
        if let type = types.first(where: { $0 != UTType.fileURL.identifier && UTType($0)?.conforms(to: .data) == true }) {
            return try await withCheckedThrowingContinuation { continuation in
                provider.loadFileRepresentation(forTypeIdentifier: type) { url, error in
                    do {
                        if let error { throw error }
                        guard let url else { throw self.failure("系统没有返回文件。") }
                        // 系统会在回调结束后删除临时文件，复制必须在此处完成。
                        continuation.resume(returning: try self.copyFile(url, provider: provider,
                            directory: directory, index: index, coordinated: false))
                    } catch { continuation.resume(throwing: error) }
                }
            }
        }
        guard provider.canLoadObject(ofClass: NSURL.self) else { throw failure("分享中没有文件表示。") }
        return try await withCheckedThrowingContinuation { continuation in
            _ = provider.loadObject(ofClass: NSURL.self) { object, error in
                do {
                    if let error { throw error }
                    guard let url = object as? URL else { throw self.failure("系统没有返回文件。") }
                    continuation.resume(returning: try self.copyFile(url, provider: provider,
                        directory: directory, index: index, coordinated: true))
                } catch { continuation.resume(throwing: error) }
            }
        }
    }

    nonisolated private func copyFile(_ source: URL, provider: NSItemProvider, directory: URL,
                          index: Int, coordinated: Bool) throws -> ShareItem {
        let scoped = source.startAccessingSecurityScopedResource()
        defer { if scoped { source.stopAccessingSecurityScopedResource() } }
        let name = sharedFileName(provider, source: source)
        let relative = String(format: "files/%04d-%@", index, name)
        let destination = directory.appendingPathComponent(relative)
        let copy = { (url: URL) throws in
            let values = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
            guard values.isRegularFile == true, values.isSymbolicLink != true,
                  let size = values.fileSize, size > 0, Int64(size) <= maxFileBytes else {
                throw self.failure("文件为空、不是普通文件或超过 512 MiB。")
            }
            try FileManager.default.copyItem(at: url, to: destination)
        }
        if coordinated {
            var coordinatorError: NSError?
            var copyError: Error?
            NSFileCoordinator().coordinate(readingItemAt: source, options: [.withoutChanges],
                error: &coordinatorError) { url in do { try copy(url) } catch { copyError = error } }
            if let coordinatorError { throw coordinatorError }
            if let copyError { throw copyError }
        } else { try copy(source) }
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: destination.path)
        let handle = try FileHandle(forWritingTo: destination)
        try handle.synchronize()
        try handle.close()
        let size = Int64(try destination.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0)
        guard size > 0, size <= maxFileBytes else { throw failure("接收文件大小无效。") }
        return ShareItem(name: name, relativePath: relative, size: size)
    }

    nonisolated private func commit(_ batch: ShareBatch, directory: URL, root: URL) throws {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        try encoder.encode(batch).write(to: directory.appendingPathComponent("manifest.json"), options: .atomic)
        let handle = try FileHandle(forWritingTo: directory.appendingPathComponent("manifest.json"))
        try handle.synchronize()
        try handle.close()
        let ready = root.appendingPathComponent("Ready")
        try FileManager.default.createDirectory(at: ready, withIntermediateDirectories: true)
        try FileManager.default.moveItem(at: directory, to: ready.appendingPathComponent(batch.id))
    }

    nonisolated private func cleanName(_ raw: String) -> String {
        let invalid = CharacterSet.controlCharacters.union(CharacterSet(charactersIn: "/\\:<>\"|?*"))
        let name = String(String.UnicodeScalarView(raw.unicodeScalars.filter { !invalid.contains($0) }))
            .trimmingCharacters(in: CharacterSet(charactersIn: " ."))
        return name.isEmpty ? "file" : String(name.prefix(120))
    }

    nonisolated private func failure(_ text: String) -> NSError {
        NSError(domain: "app.iywclaw.share", code: 1, userInfo: [NSLocalizedDescriptionKey: text])
    }

    nonisolated private func sharedFileName(_ provider: NSItemProvider, source: URL) -> String {
        let raw = provider.suggestedName.flatMap { $0.isEmpty ? nil : $0 } ?? source.lastPathComponent
        var name = cleanName(raw)
        if (name as NSString).pathExtension.isEmpty {
            let types = provider.registeredTypeIdentifiers.compactMap { UTType($0) }
            let suffix = types.first(where: { $0.conforms(to: .zip) })?.preferredFilenameExtension
                ?? types.first(where: { $0.conforms(to: .data) })?.preferredFilenameExtension
                ?? (source.pathExtension.isEmpty ? nil : source.pathExtension)
            if let suffix { name += "." + suffix }
        }
        return name
    }

    private func launchMain() {
        let app = Bundle.main.bundleURL.deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
        guard app.pathExtension == "app" else { return }
        let configuration = NSWorkspace.OpenConfiguration()
        configuration.arguments = ["--from-native-share"]
        NSWorkspace.shared.openApplication(at: app, configuration: configuration) { _, error in
            if let error = error as NSError? {
                NSLog("原助理分享唤醒失败 domain=%@ code=%ld", error.domain, error.code)
            }
        }
    }
}
