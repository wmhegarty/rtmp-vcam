import CoreMediaIO
import Foundation
import os.log

private let logger = Logger(subsystem: "com.rtmpvcam.host.camera-extension", category: "Provider")

/// Top-level provider source for the Camera Extension.
/// Registers one virtual camera device.
class CameraProviderSource: NSObject, CMIOExtensionProviderSource {
    private(set) var provider: CMIOExtensionProvider!
    private let deviceSource: CameraDeviceSource
    private let _device: CMIOExtensionDevice
    private let streamSource: CameraStreamSource
    private let _stream: CMIOExtensionStream

    init(clientQueue: DispatchQueue?) {
        deviceSource = CameraDeviceSource()
        streamSource = CameraStreamSource()

        _device = CMIOExtensionDevice(
            localizedName: "RTMP Virtual Camera",
            deviceID: UUID(uuidString: "A1B2C3D4-E5F6-7890-ABCD-EF1234567890")!,
            legacyDeviceID: nil,
            source: deviceSource
        )

        _stream = CMIOExtensionStream(
            localizedName: "RTMP Video",
            streamID: UUID(uuidString: "B2C3D4E5-F6A7-8901-BCDE-F12345678901")!,
            direction: .source,
            clockType: .hostTime,
            source: streamSource
        )

        super.init()
        logger.info("CameraProviderSource initializing")

        // Give the stream source a reference to its CMIOExtensionStream
        streamSource.extensionStream = _stream

        do {
            try _device.addStream(_stream)
            logger.info("Stream added to device")
        } catch {
            logger.error("Failed to add stream: \(error.localizedDescription)")
        }

        // Create provider with self as source (must be after super.init)
        provider = CMIOExtensionProvider(source: self, clientQueue: clientQueue)

        // Explicitly register the device with the provider
        do {
            try provider.addDevice(_device)
            logger.info("Device added to provider successfully")
        } catch {
            logger.error("Failed to add device to provider: \(error.localizedDescription)")
        }
    }

    var availableProperties: Set<CMIOExtensionProperty> {
        return [.providerManufacturer]
    }

    func providerProperties(forProperties properties: Set<CMIOExtensionProperty>) throws
        -> CMIOExtensionProviderProperties
    {
        let providerProperties = CMIOExtensionProviderProperties(dictionary: [:])
        if properties.contains(.providerManufacturer) {
            providerProperties.manufacturer = "RTMP Virtual Camera"
        }
        return providerProperties
    }

    func setProviderProperties(_ providerProperties: CMIOExtensionProviderProperties) throws {
        // Read-only provider
    }

    func connect(to client: CMIOExtensionClient) throws {
        logger.info("Client connected")
    }

    func disconnect(from client: CMIOExtensionClient) {
        logger.info("Client disconnected")
    }

    func devices() -> [CMIOExtensionDevice] {
        return [_device]
    }

    func setProvider(_ provider: CMIOExtensionProvider) {
        logger.info("setProvider called")
    }
}
