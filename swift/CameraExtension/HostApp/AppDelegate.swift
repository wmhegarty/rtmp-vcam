import Cocoa
import SystemExtensions
import os.log

private let logger = Logger(subsystem: "com.rtmpvcam.host-app", category: "AppDelegate")

class AppDelegate: NSObject, NSApplicationDelegate, OSSystemExtensionRequestDelegate {

    private var window: NSWindow?

    // Extension UI
    private var extensionStatusLabel: NSTextField?
    private var installButton: NSButton?
    private var uninstallButton: NSButton?

    // Server UI
    private var serverStatusLabel: NSTextField?
    private var portField: NSTextField?
    private var serverToggleButton: NSButton?
    private var logTextView: NSTextView?

    // Stream key UI
    private var streamKeyField: NSTextField?
    private var rtmpURLField: NSTextField?

    // Process management
    private var serverProcess: Process?
    private var isServerRunning = false
    private var intentionalStop = false
    private var crashTimestamps: [Date] = []
    private static let maxCrashRetries = 3
    private static let crashWindowSeconds: TimeInterval = 30
    private static let logMaxBytes = 50_000

    // MARK: - App Lifecycle

    func applicationDidFinishLaunching(_ notification: Notification) {
        setupWindow()
        NSApp.activate(ignoringOtherApps: true)
        logger.info("Host app launched")
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        return true
    }

    func applicationWillTerminate(_ notification: Notification) {
        stopServer()
    }

    // MARK: - UI Setup

    private func setupWindow() {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 500, height: 570),
            styleMask: [.titled, .closable, .miniaturizable],
            backing: .buffered,
            defer: false
        )
        window.title = "RTMP Virtual Camera"
        window.center()

        let contentView = NSView(frame: window.contentView!.bounds)
        contentView.autoresizingMask = [.width, .height]

        var y: CGFloat = 530

        // === Camera Extension Section ===
        y = addSectionHeader("Camera Extension", to: contentView, y: y)

        let extensionStatusLabel = NSTextField(labelWithString: "Extension not installed")
        extensionStatusLabel.frame = NSRect(x: 20, y: y, width: 460, height: 20)
        extensionStatusLabel.font = .systemFont(ofSize: 13)
        contentView.addSubview(extensionStatusLabel)
        self.extensionStatusLabel = extensionStatusLabel
        y -= 36

        let installButton = NSButton(title: "Install Extension", target: self, action: #selector(installExtension))
        installButton.frame = NSRect(x: 20, y: y, width: 140, height: 28)
        installButton.bezelStyle = .rounded
        contentView.addSubview(installButton)
        self.installButton = installButton

        let uninstallButton = NSButton(title: "Uninstall Extension", target: self, action: #selector(uninstallExtension))
        uninstallButton.frame = NSRect(x: 170, y: y, width: 150, height: 28)
        uninstallButton.bezelStyle = .rounded
        contentView.addSubview(uninstallButton)
        self.uninstallButton = uninstallButton
        y -= 16

        // Separator
        let sep = NSBox(frame: NSRect(x: 20, y: y, width: 460, height: 1))
        sep.boxType = .separator
        contentView.addSubview(sep)
        y -= 16

        // === RTMP Server Section ===
        y = addSectionHeader("RTMP Server", to: contentView, y: y)

        let serverStatusLabel = NSTextField(labelWithString: "Stopped")
        serverStatusLabel.frame = NSRect(x: 20, y: y, width: 460, height: 20)
        serverStatusLabel.font = .systemFont(ofSize: 13)
        serverStatusLabel.textColor = .secondaryLabelColor
        contentView.addSubview(serverStatusLabel)
        self.serverStatusLabel = serverStatusLabel
        y -= 36

        // Port field
        let portLabel = NSTextField(labelWithString: "Port:")
        portLabel.frame = NSRect(x: 20, y: y + 2, width: 35, height: 20)
        portLabel.font = .systemFont(ofSize: 13)
        contentView.addSubview(portLabel)

        let portField = NSTextField(string: "1935")
        portField.frame = NSRect(x: 58, y: y, width: 70, height: 24)
        portField.font = .monospacedDigitSystemFont(ofSize: 13, weight: .regular)
        contentView.addSubview(portField)
        self.portField = portField

        let serverToggleButton = NSButton(title: "Start Server", target: self, action: #selector(toggleServer))
        serverToggleButton.frame = NSRect(x: 140, y: y - 2, width: 120, height: 28)
        serverToggleButton.bezelStyle = .rounded
        contentView.addSubview(serverToggleButton)
        self.serverToggleButton = serverToggleButton
        y -= 40

        // Stream key row
        let keyLabel = NSTextField(labelWithString: "Stream Key:")
        keyLabel.frame = NSRect(x: 20, y: y + 2, width: 80, height: 20)
        keyLabel.font = .systemFont(ofSize: 13)
        contentView.addSubview(keyLabel)

        let streamKeyField = NSTextField(string: "")
        streamKeyField.frame = NSRect(x: 104, y: y, width: 200, height: 24)
        streamKeyField.font = .monospacedSystemFont(ofSize: 13, weight: .regular)
        streamKeyField.isEditable = false
        streamKeyField.isSelectable = true
        streamKeyField.placeholderString = "None (accepting all)"
        if let saved = UserDefaults.standard.string(forKey: "streamKey") {
            streamKeyField.stringValue = saved
        }
        contentView.addSubview(streamKeyField)
        self.streamKeyField = streamKeyField

        let generateButton = NSButton(title: "Generate Key", target: self, action: #selector(generateStreamKey))
        generateButton.frame = NSRect(x: 312, y: y - 2, width: 110, height: 28)
        generateButton.bezelStyle = .rounded
        contentView.addSubview(generateButton)

        let clearButton = NSButton(title: "Clear", target: self, action: #selector(clearStreamKey))
        clearButton.frame = NSRect(x: 428, y: y - 2, width: 52, height: 28)
        clearButton.bezelStyle = .rounded
        contentView.addSubview(clearButton)
        y -= 32

        // RTMP URL display
        let urlLabel = NSTextField(labelWithString: "RTMP URL:")
        urlLabel.frame = NSRect(x: 20, y: y + 2, width: 75, height: 20)
        urlLabel.font = .systemFont(ofSize: 13)
        contentView.addSubview(urlLabel)

        let rtmpURLField = NSTextField(string: "")
        rtmpURLField.frame = NSRect(x: 104, y: y, width: 376, height: 24)
        rtmpURLField.font = .monospacedSystemFont(ofSize: 11, weight: .regular)
        rtmpURLField.isEditable = false
        rtmpURLField.isSelectable = true
        rtmpURLField.textColor = .secondaryLabelColor
        contentView.addSubview(rtmpURLField)
        self.rtmpURLField = rtmpURLField
        updateRTMPURL()
        y -= 40

        // Log view
        let logLabel = NSTextField(labelWithString: "Server Log:")
        logLabel.frame = NSRect(x: 20, y: y, width: 100, height: 18)
        logLabel.font = .systemFont(ofSize: 11)
        logLabel.textColor = .secondaryLabelColor
        contentView.addSubview(logLabel)
        y -= 4

        let scrollView = NSScrollView(frame: NSRect(x: 20, y: 10, width: 460, height: y - 10))
        scrollView.hasVerticalScroller = true
        scrollView.autoresizingMask = [.width, .height]
        scrollView.borderType = .bezelBorder

        let logTextView = NSTextView(frame: scrollView.contentView.bounds)
        logTextView.isEditable = false
        logTextView.isSelectable = true
        logTextView.autoresizingMask = [.width]
        logTextView.font = .monospacedSystemFont(ofSize: 11, weight: .regular)
        logTextView.backgroundColor = NSColor(white: 0.1, alpha: 1.0)
        logTextView.textColor = NSColor(red: 0.3, green: 0.9, blue: 0.3, alpha: 1.0)
        logTextView.textContainerInset = NSSize(width: 4, height: 4)
        scrollView.documentView = logTextView
        contentView.addSubview(scrollView)
        self.logTextView = logTextView

        window.contentView = contentView
        window.makeKeyAndOrderFront(nil)
        self.window = window
    }

    private func addSectionHeader(_ title: String, to view: NSView, y: CGFloat) -> CGFloat {
        let label = NSTextField(labelWithString: title)
        label.frame = NSRect(x: 20, y: y, width: 460, height: 20)
        label.font = .boldSystemFont(ofSize: 14)
        view.addSubview(label)
        return y - 26
    }

    // MARK: - Extension Management

    @objc private func installExtension() {
        logger.info("Requesting extension installation")
        extensionStatusLabel?.stringValue = "Installing..."

        let request = OSSystemExtensionRequest.activationRequest(
            forExtensionWithIdentifier: "com.rtmpvcam.host.camera-extension",
            queue: .main
        )
        request.delegate = self
        OSSystemExtensionManager.shared.submitRequest(request)
    }

    @objc private func uninstallExtension() {
        logger.info("Requesting extension uninstallation")
        extensionStatusLabel?.stringValue = "Uninstalling..."

        let request = OSSystemExtensionRequest.deactivationRequest(
            forExtensionWithIdentifier: "com.rtmpvcam.host.camera-extension",
            queue: .main
        )
        request.delegate = self
        OSSystemExtensionManager.shared.submitRequest(request)
    }

    // MARK: - OSSystemExtensionRequestDelegate

    func request(
        _ request: OSSystemExtensionRequest,
        actionForReplacingExtension existing: OSSystemExtensionProperties,
        withExtension ext: OSSystemExtensionProperties
    ) -> OSSystemExtensionRequest.ReplacementAction {
        logger.info("Replacing existing extension")
        return .replace
    }

    func requestNeedsUserApproval(_ request: OSSystemExtensionRequest) {
        logger.info("Extension needs user approval")
        extensionStatusLabel?.stringValue = "Waiting for approval in System Settings..."
    }

    func request(
        _ request: OSSystemExtensionRequest,
        didFinishWithResult result: OSSystemExtensionRequest.Result
    ) {
        switch result {
        case .completed:
            logger.info("Extension request completed successfully")
            extensionStatusLabel?.stringValue = "Extension installed and active"
        case .willCompleteAfterReboot:
            logger.info("Extension will complete after reboot")
            extensionStatusLabel?.stringValue = "Extension will be active after reboot"
        @unknown default:
            logger.info("Extension request finished with unknown result: \(String(describing: result))")
            extensionStatusLabel?.stringValue = "Extension request completed"
        }
    }

    func request(_ request: OSSystemExtensionRequest, didFailWithError error: Error) {
        logger.error("Extension request failed: \(error.localizedDescription)")
        extensionStatusLabel?.stringValue = "Error: \(error.localizedDescription)"
    }

    // MARK: - Stream Key

    @objc private func generateStreamKey() {
        let chars = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
        let key = String((0..<16).map { _ in chars.randomElement()! })
        streamKeyField?.stringValue = key
        UserDefaults.standard.set(key, forKey: "streamKey")
        updateRTMPURL()
        logger.info("Generated new stream key")
    }

    @objc private func clearStreamKey() {
        streamKeyField?.stringValue = ""
        UserDefaults.standard.removeObject(forKey: "streamKey")
        updateRTMPURL()
        logger.info("Cleared stream key")
    }

    private func updateRTMPURL() {
        let port = portField?.stringValue ?? "1935"
        let key = streamKeyField?.stringValue ?? ""
        if key.isEmpty {
            rtmpURLField?.stringValue = "rtmp://localhost:\(port)/live/<any>"
        } else {
            rtmpURLField?.stringValue = "rtmp://localhost:\(port)/live/\(key)"
        }
    }

    // MARK: - Server Management

    @objc private func toggleServer() {
        if isServerRunning {
            stopServer()
        } else {
            startServer()
        }
    }

    private func startServer() {
        guard let binaryURL = Bundle.main.url(forAuxiliaryExecutable: "rtmp-vcam-server") else {
            appendLog("ERROR: rtmp-vcam-server not found in app bundle.\n")
            appendLog("Build with 'make build-all' to embed the Rust binary.\n")
            serverStatusLabel?.stringValue = "Error: binary not found"
            serverStatusLabel?.textColor = .systemRed
            return
        }

        let port = portField?.stringValue ?? "1935"
        guard let portNum = UInt16(port), portNum > 0 else {
            appendLog("ERROR: Invalid port '\(port)'\n")
            return
        }

        appendLog("Starting server on port \(port)...\n")

        let process = Process()
        process.executableURL = binaryURL
        var args = ["--port", port, "--verbose"]
        let streamKey = streamKeyField?.stringValue ?? ""
        if !streamKey.isEmpty {
            args += ["--stream-key", streamKey]
        }
        process.arguments = args

        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = pipe

        // Read output asynchronously
        pipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            guard !data.isEmpty, let text = String(data: data, encoding: .utf8) else { return }
            DispatchQueue.main.async {
                self?.appendLog(text)
            }
        }

        process.terminationHandler = { [weak self] proc in
            DispatchQueue.main.async {
                guard let self = self else { return }
                pipe.fileHandleForReading.readabilityHandler = nil
                self.isServerRunning = false
                self.updateServerUI()

                let status = proc.terminationStatus

                if self.intentionalStop {
                    self.intentionalStop = false
                    self.appendLog("Server stopped.\n")
                    self.serverStatusLabel?.stringValue = "Stopped"
                    self.serverStatusLabel?.textColor = .secondaryLabelColor
                } else if status != 0 {
                    self.appendLog("Server exited unexpectedly (status \(status))\n")
                    self.attemptRestart()
                } else {
                    self.appendLog("Server stopped.\n")
                    self.serverStatusLabel?.stringValue = "Stopped"
                    self.serverStatusLabel?.textColor = .secondaryLabelColor
                }
            }
        }

        do {
            try process.run()
            serverProcess = process
            isServerRunning = true
            updateServerUI()
            serverStatusLabel?.stringValue = "Running on port \(port) (PID \(process.processIdentifier))"
            serverStatusLabel?.textColor = .systemGreen
            logger.info("Server started on port \(port), PID \(process.processIdentifier)")
        } catch {
            appendLog("Failed to start server: \(error.localizedDescription)\n")
            serverStatusLabel?.stringValue = "Error: \(error.localizedDescription)"
            serverStatusLabel?.textColor = .systemRed
        }
    }

    private func stopServer() {
        guard let process = serverProcess, process.isRunning else {
            isServerRunning = false
            updateServerUI()
            return
        }

        intentionalStop = true
        appendLog("Stopping server...\n")
        process.terminate() // SIGTERM

        // Grace period: SIGKILL after 3 seconds if still running
        DispatchQueue.global().asyncAfter(deadline: .now() + 3) { [weak self] in
            if process.isRunning {
                process.interrupt() // SIGINT as escalation
                DispatchQueue.global().asyncAfter(deadline: .now() + 1) {
                    if process.isRunning {
                        kill(process.processIdentifier, SIGKILL)
                        DispatchQueue.main.async {
                            self?.appendLog("Server force-killed.\n")
                        }
                    }
                }
            }
        }

        serverProcess = nil
        isServerRunning = false
        updateServerUI()
        serverStatusLabel?.stringValue = "Stopped"
        serverStatusLabel?.textColor = .secondaryLabelColor
    }

    private func attemptRestart() {
        let now = Date()
        crashTimestamps = crashTimestamps.filter {
            now.timeIntervalSince($0) < Self.crashWindowSeconds
        }

        if crashTimestamps.count >= Self.maxCrashRetries {
            appendLog("Too many crashes (\(Self.maxCrashRetries) in \(Int(Self.crashWindowSeconds))s). Not restarting.\n")
            serverStatusLabel?.stringValue = "Crashed — too many restarts"
            serverStatusLabel?.textColor = .systemRed
            return
        }

        crashTimestamps.append(now)
        let attempt = crashTimestamps.count
        appendLog("Auto-restarting (attempt \(attempt)/\(Self.maxCrashRetries))...\n")

        DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [weak self] in
            self?.startServer()
        }
    }

    private func updateServerUI() {
        serverToggleButton?.title = isServerRunning ? "Stop Server" : "Start Server"
        portField?.isEditable = !isServerRunning
        portField?.isEnabled = !isServerRunning
    }


    // MARK: - Log View

    private func appendLog(_ text: String) {
        guard let textView = logTextView else { return }

        let storage = textView.textStorage!
        let attrs: [NSAttributedString.Key: Any] = [
            .foregroundColor: NSColor(red: 0.3, green: 0.9, blue: 0.3, alpha: 1.0),
            .font: NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)
        ]
        storage.append(NSAttributedString(string: text, attributes: attrs))

        // Trim if too large
        if storage.length > Self.logMaxBytes {
            let excess = storage.length - Self.logMaxBytes
            storage.deleteCharacters(in: NSRange(location: 0, length: excess))
        }

        // Auto-scroll to bottom
        textView.scrollToEndOfDocument(nil)
    }
}
