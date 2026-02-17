import Foundation
import CoreMediaIO

// Entry point for the Camera Extension process.
// The system launches this as a separate process managed by the SystemExtensions framework.

let providerSource = CameraProviderSource(clientQueue: nil)
CMIOExtensionProvider.startService(provider: providerSource.provider)

// Keep the extension running
CFRunLoopRun()
