import CoreMediaIO
import Foundation
import os.log

// FourCC 'virt' = kIOAudioDeviceTransportTypeVirtual
private let kTransportTypeVirtual: UInt32 = 0x76697274

private let logger = Logger(subsystem: "com.rtmpvcam.host.camera-extension", category: "Device")

/// Virtual camera device source.
class CameraDeviceSource: NSObject, CMIOExtensionDeviceSource {

    override init() {
        super.init()
        logger.info("CameraDeviceSource initializing")
    }

    var availableProperties: Set<CMIOExtensionProperty> {
        return [.deviceTransportType, .deviceModel]
    }

    func deviceProperties(forProperties properties: Set<CMIOExtensionProperty>) throws
        -> CMIOExtensionDeviceProperties
    {
        let deviceProperties = CMIOExtensionDeviceProperties(dictionary: [:])
        if properties.contains(.deviceTransportType) {
            deviceProperties.transportType = Int(kTransportTypeVirtual)
        }
        if properties.contains(.deviceModel) {
            deviceProperties.model = "RTMP Virtual Camera"
        }
        return deviceProperties
    }

    func setDeviceProperties(_ deviceProperties: CMIOExtensionDeviceProperties) throws {
        // Read-only device
    }
}
