import CoreMediaIO
import CoreVideo
import Foundation
import os.log

private let logger = Logger(subsystem: "com.rtmpvcam.host.camera-extension", category: "Stream")

/// Shared memory layout for IPC with the Rust process.
/// Must match the Rust side exactly (video_pipeline::decoder constants).
///
/// Header (64 bytes):
///   [0..8)    write_index (u64, little-endian, atomic)
///   [8..12)   width (u32)
///   [12..16)  height (u32)
///   [16..64)  reserved
///
/// Frame data (double-buffered):
///   [64 .. 64+MAX_FRAME_SIZE)                   frame buffer 0
///   [64+MAX_FRAME_SIZE .. 64+2*MAX_FRAME_SIZE)  frame buffer 1
///
/// Each frame is NV12: Y plane (width*height) + UV plane (width*height/2)
private let kHeaderSize = 64
private let kMaxWidth = 1920
private let kMaxHeight = 1080
private let kMaxFrameSize = kMaxWidth * kMaxHeight * 3 / 2  // NV12
private let kShmSize = kHeaderSize + 2 * kMaxFrameSize       // ~6.2MB

/// Ring buffer file path — must match the Rust side.
/// The cmioextension sandbox allows: (allow file-read* (subpath "/Library"))
private let kRingFilePath = "/Library/Application Support/RTMPVirtualCamera/rtmp_vcam_ring"

/// Virtual camera stream source.
/// Reads raw NV12 pixel data from shared memory and delivers frames to the system.
class CameraStreamSource: NSObject, CMIOExtensionStreamSource {
    var extensionStream: CMIOExtensionStream?

    private let width: Int32 = 1920
    private let height: Int32 = 1080
    private let frameRate: Float64 = 30.0

    private var timer: DispatchSourceTimer?
    private var shmPointer: UnsafeMutableRawPointer?
    private var shmFd: Int32 = -1
    private var isStreaming = false

    // Sequence number for frame timing
    private var sequenceNumber: UInt64 = 0
    // Track last write_index to detect new frames
    private var lastWriteIndex: UInt64 = 0

    override init() {
        super.init()
    }

    deinit {
        stopStreaming()
    }

    // MARK: - CMIOExtensionStreamSource

    var formats: [CMIOExtensionStreamFormat] {
        let format = CMIOExtensionStreamFormat(
            formatDescription: CameraStreamSource.createFormatDescription(
                width: width, height: height),
            maxFrameDuration: CMTime(value: 1, timescale: CMTimeScale(frameRate)),
            minFrameDuration: CMTime(value: 1, timescale: CMTimeScale(frameRate)),
            validFrameDurations: nil
        )
        return [format]
    }

    var availableProperties: Set<CMIOExtensionProperty> {
        return [
            .streamActiveFormatIndex,
            .streamFrameDuration,
        ]
    }

    func streamProperties(forProperties properties: Set<CMIOExtensionProperty>) throws
        -> CMIOExtensionStreamProperties
    {
        let streamProperties = CMIOExtensionStreamProperties(dictionary: [:])
        if properties.contains(.streamActiveFormatIndex) {
            streamProperties.activeFormatIndex = 0
        }
        if properties.contains(.streamFrameDuration) {
            streamProperties.frameDuration = CMTime(
                value: 1, timescale: CMTimeScale(frameRate))
        }
        return streamProperties
    }

    func setStreamProperties(_ streamProperties: CMIOExtensionStreamProperties) throws {
        // Read-only stream
    }

    func authorizedToStartStream(for client: CMIOExtensionClient) -> Bool {
        return true
    }

    func startStream() throws {
        logger.info("Starting stream")
        guard !isStreaming else { return }
        isStreaming = true
        sequenceNumber = 0
        lastWriteIndex = 0

        openSharedMemory()
        startFrameTimer()
    }

    func stopStream() throws {
        logger.info("Stopping stream")
        stopStreaming()
    }

    // MARK: - Shared Memory IPC

    private var shmRetryCount: UInt64 = 0

    private func openSharedMemory() {
        let path = kRingFilePath

        let fd = open(path, O_RDONLY)
        guard fd >= 0 else {
            shmRetryCount += 1
            if shmRetryCount == 1 || shmRetryCount % 300 == 0 {
                let errNo = errno
                logger.warning("Could not open ring file '\(path, privacy: .public)' errno=\(errNo, privacy: .public) — Rust process may not be running (attempt \(self.shmRetryCount))")
            }
            return
        }

        let ptr = mmap(nil, kShmSize, PROT_READ, MAP_SHARED, fd, 0)
        guard ptr != MAP_FAILED else {
            logger.error("mmap failed for ring file")
            close(fd)
            return
        }

        shmFd = fd
        shmPointer = ptr
        logger.info("Frame buffer mapped successfully from '\(path)' size=\(kShmSize)")
    }

    private func closeSharedMemory() {
        if let ptr = shmPointer {
            munmap(ptr, kShmSize)
            shmPointer = nil
        }
        if shmFd >= 0 {
            close(shmFd)
            shmFd = -1
        }
    }

    /// Read the latest frame data from shared memory into a CVPixelBuffer.
    private func readLatestFrame() -> CVPixelBuffer? {
        guard let ptr = shmPointer else { return nil }

        // Read write_index atomically
        let writeIndex = ptr.load(fromByteOffset: 0, as: UInt64.self)
        guard writeIndex > 0 else { return nil }

        // Read dimensions from header
        let frameWidth = Int(ptr.load(fromByteOffset: 8, as: UInt32.self))
        let frameHeight = Int(ptr.load(fromByteOffset: 12, as: UInt32.self))

        guard frameWidth > 0, frameHeight > 0,
              frameWidth <= kMaxWidth, frameHeight <= kMaxHeight else { return nil }

        // Determine which double-buffer slot to read from
        // Reader reads from the most recently completed slot
        let slot = Int((writeIndex - 1) % 2)
        let frameOffset = kHeaderSize + slot * kMaxFrameSize
        let frameSize = frameWidth * frameHeight * 3 / 2

        // Create a CVPixelBuffer and copy data into it
        var pixelBuffer: CVPixelBuffer?
        let attrs: [String: Any] = [
            kCVPixelBufferIOSurfacePropertiesKey as String: [:] as [String: Any],
        ]
        let status = CVPixelBufferCreate(
            kCFAllocatorDefault,
            frameWidth,
            frameHeight,
            kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
            attrs as CFDictionary,
            &pixelBuffer
        )

        guard status == kCVReturnSuccess, let pixelBuffer = pixelBuffer else {
            return nil
        }

        CVPixelBufferLockBaseAddress(pixelBuffer, [])

        let srcBase = ptr.advanced(by: frameOffset)

        // Copy Y plane
        if let yDst = CVPixelBufferGetBaseAddressOfPlane(pixelBuffer, 0) {
            let yDstStride = CVPixelBufferGetBytesPerRowOfPlane(pixelBuffer, 0)
            let yHeight = CVPixelBufferGetHeightOfPlane(pixelBuffer, 0)
            if yDstStride == frameWidth {
                // Fast path
                memcpy(yDst, srcBase, frameWidth * yHeight)
            } else {
                // Row by row
                for row in 0..<yHeight {
                    memcpy(
                        yDst.advanced(by: row * yDstStride),
                        srcBase.advanced(by: row * frameWidth),
                        frameWidth
                    )
                }
            }
        }

        // Copy UV plane
        let uvSrcOffset = frameWidth * frameHeight
        if let uvDst = CVPixelBufferGetBaseAddressOfPlane(pixelBuffer, 1) {
            let uvDstStride = CVPixelBufferGetBytesPerRowOfPlane(pixelBuffer, 1)
            let uvHeight = CVPixelBufferGetHeightOfPlane(pixelBuffer, 1)
            if uvDstStride == frameWidth {
                memcpy(uvDst, srcBase.advanced(by: uvSrcOffset), frameWidth * uvHeight)
            } else {
                for row in 0..<uvHeight {
                    memcpy(
                        uvDst.advanced(by: row * uvDstStride),
                        srcBase.advanced(by: uvSrcOffset + row * frameWidth),
                        frameWidth
                    )
                }
            }
        }

        CVPixelBufferUnlockBaseAddress(pixelBuffer, [])

        return pixelBuffer
    }

    // MARK: - Frame Delivery

    private func startFrameTimer() {
        let interval = 1.0 / frameRate
        let timer = DispatchSource.makeTimerSource(queue: DispatchQueue.global(qos: .userInteractive))
        timer.schedule(
            deadline: .now(),
            repeating: interval,
            leeway: .milliseconds(1)
        )
        timer.setEventHandler { [weak self] in
            self?.deliverFrame()
        }
        timer.resume()
        self.timer = timer
    }

    private func stopStreaming() {
        isStreaming = false
        timer?.cancel()
        timer = nil
        closeSharedMemory()
    }

    private func deliverFrame() {
        guard let stream = extensionStream else { return }

        // Retry opening shared memory if not yet connected
        if shmPointer == nil {
            openSharedMemory()
        }

        // Try to get a frame from shared memory
        if let pixelBuffer = readLatestFrame() {
            sendPixelBuffer(pixelBuffer, to: stream)
        } else {
            // No RTMP data yet — deliver a black frame as placeholder
            deliverBlackFrame(stream: stream)
        }
    }

    private func deliverBlackFrame(stream: CMIOExtensionStream) {
        // Create a black NV12 pixel buffer
        var pixelBuffer: CVPixelBuffer?
        let attrs: [String: Any] = [
            kCVPixelBufferIOSurfacePropertiesKey as String: [:] as [String: Any],
        ]
        let status = CVPixelBufferCreate(
            kCFAllocatorDefault,
            Int(width),
            Int(height),
            kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
            attrs as CFDictionary,
            &pixelBuffer
        )

        guard status == kCVReturnSuccess, let pixelBuffer = pixelBuffer else {
            return
        }

        // Fill with black (Y=0, UV=128)
        CVPixelBufferLockBaseAddress(pixelBuffer, [])

        // Y plane
        if let yBase = CVPixelBufferGetBaseAddressOfPlane(pixelBuffer, 0) {
            let yHeight = CVPixelBufferGetHeightOfPlane(pixelBuffer, 0)
            let yBytesPerRow = CVPixelBufferGetBytesPerRowOfPlane(pixelBuffer, 0)
            memset(yBase, 0, yHeight * yBytesPerRow)
        }

        // UV plane (128 = neutral chroma)
        if let uvBase = CVPixelBufferGetBaseAddressOfPlane(pixelBuffer, 1) {
            let uvHeight = CVPixelBufferGetHeightOfPlane(pixelBuffer, 1)
            let uvBytesPerRow = CVPixelBufferGetBytesPerRowOfPlane(pixelBuffer, 1)
            memset(uvBase, 128, uvHeight * uvBytesPerRow)
        }

        CVPixelBufferUnlockBaseAddress(pixelBuffer, [])

        sendPixelBuffer(pixelBuffer, to: stream)
    }

    private func sendPixelBuffer(_ pixelBuffer: CVPixelBuffer, to stream: CMIOExtensionStream) {
        // Create timing
        let now = CMClockGetTime(CMClockGetHostTimeClock())
        let duration = CMTime(value: 1, timescale: CMTimeScale(frameRate))

        // Create format description
        var formatDesc: CMFormatDescription?
        CMVideoFormatDescriptionCreateForImageBuffer(
            allocator: kCFAllocatorDefault,
            imageBuffer: pixelBuffer,
            formatDescriptionOut: &formatDesc
        )

        guard let formatDesc = formatDesc else { return }

        // Create sample buffer
        var timingInfo = CMSampleTimingInfo(
            duration: duration,
            presentationTimeStamp: now,
            decodeTimeStamp: .invalid
        )

        var sampleBuffer: CMSampleBuffer?
        CMSampleBufferCreateForImageBuffer(
            allocator: kCFAllocatorDefault,
            imageBuffer: pixelBuffer,
            dataReady: true,
            makeDataReadyCallback: nil,
            refcon: nil,
            formatDescription: formatDesc,
            sampleTiming: &timingInfo,
            sampleBufferOut: &sampleBuffer
        )

        guard let sampleBuffer = sampleBuffer else { return }

        sequenceNumber += 1

        do {
            try stream.send(
                sampleBuffer,
                discontinuity: sequenceNumber == 1 ? [.sampleDropped] : [],
                hostTimeInNanoseconds: UInt64(now.seconds * 1_000_000_000)
            )
        } catch {
            if sequenceNumber % 300 == 0 {
                // Log every ~10s to avoid spam
                logger.error("Failed to send frame: \(error.localizedDescription)")
            }
        }
    }

    // MARK: - Helpers

    private static func createFormatDescription(width: Int32, height: Int32)
        -> CMFormatDescription
    {
        var formatDesc: CMFormatDescription?
        CMVideoFormatDescriptionCreate(
            allocator: kCFAllocatorDefault,
            codecType: kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
            width: width,
            height: height,
            extensions: nil,
            formatDescriptionOut: &formatDesc
        )
        return formatDesc!
    }
}
