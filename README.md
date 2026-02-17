# RTMP Virtual Camera for macOS

Receive RTMP video streams (e.g., from OBS, MeldStudio, or ffmpeg) and expose them as a native macOS virtual camera. Any app that uses a webcam — FaceTime, Zoom, Google Meet, QuickTime — can select "RTMP Virtual Camera" as a camera source.

## How It Works

```
RTMP Source ──► Rust Server ──► H.264 Decode ──► Shared Memory ──► Camera Extension ──► macOS Apps
 (ffmpeg,       (port 1935)    (VideoToolbox)   (mmap NV12)       (CoreMediaIO)       (FaceTime,
  OBS, etc.)                                                                            Zoom, etc.)
```

Three-layer architecture:

1. **rtmp-server** (Rust) — RTMP protocol handling, TCP server, FLV demux
2. **video-pipeline** (Rust) — VideoToolbox H.264 hardware decoding, raw NV12 pixel output
3. **Camera Extension** (Swift) — CoreMediaIO system extension that reads frames from shared memory and exposes them as a virtual camera

IPC uses a double-buffered memory-mapped file at `/Library/Application Support/RTMPVirtualCamera/rtmp_vcam_ring` (~6.2MB: 64-byte header + 2× 1920×1080 NV12 frames).

## Requirements

- macOS 12.3+
- Xcode 15+ (for building)
- Rust toolchain (`rustup`)
- An Apple Developer account (code signing required for system extensions)

## Build & Install

```bash
# Build everything (Rust + Swift)
make build-all

# Install to /Applications
make install
```

The Rust RTMP server binary is embedded inside the app bundle and managed from the UI — no need to run it separately.

## Usage

1. **Launch** `/Applications/RTMPVirtualCamera.app`
2. **Install the camera extension** — click "Install Extension" and approve in System Settings → Privacy & Security
3. **Start the RTMP server** — click "Start Server" (default port 1935)
4. **Send video** from any RTMP source:

```bash
# ffmpeg test pattern
ffmpeg -f lavfi -i testsrc=duration=3600:size=1920x1080:rate=30 \
  -pix_fmt yuv420p -c:v libx264 -b:v 2M -f flv rtmp://localhost/live/test

# Or configure OBS/MeldStudio with:
#   Server: rtmp://localhost/live
#   Stream Key: test
```

5. **Select the camera** — open FaceTime, Zoom, or any video app and choose "RTMP Virtual Camera"

## Project Structure

```
rtmp-vcam/
├── Cargo.toml                    # Rust workspace
├── Makefile                      # Build orchestration
├── crates/
│   ├── rtmp-server/              # RTMP protocol + TCP server
│   ├── video-pipeline/           # VideoToolbox H.264 decode (raw C FFI)
│   └── rtmp-vcam-app/            # Main binary (wires RTMP → decode → IPC)
└── swift/
    └── CameraExtension/          # Xcode project
        ├── HostApp/              # App UI (extension + server management)
        └── Extension/            # CoreMediaIO camera extension
```

## CLI Usage

The server can also be run standalone:

```
rtmp-vcam-app [OPTIONS]

Options:
  -p, --port <PORT>    RTMP listen port (default: 1935)
  -v, --verbose        Enable debug logging
  -h, --help           Show this help
```

## Notes

- The H.264 stream must use YUV 4:2:0 (`-pix_fmt yuv420p`). High 4:4:4 Predictive profile is not supported by VideoToolbox.
- FaceTime mirrors the camera preview (like a real webcam). The remote viewer sees it correctly.
- The camera extension runs in a sandboxed process — IPC uses file-backed mmap under `/Library/Application Support/` since POSIX shared memory and IOSurface are blocked by the sandbox.

## License

[MIT](LICENSE) — free to use, modify, and distribute with attribution.
