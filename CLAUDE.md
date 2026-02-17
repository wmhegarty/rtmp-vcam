# RTMP Virtual Camera for macOS

## Overview
A Rust + Swift hybrid app that receives RTMP video streams (e.g., from MeldStudio) and exposes them as a macOS virtual camera via Apple's CoreMediaIO Camera Extension framework.

## Architecture
Three-layer design:
1. **rtmp-server** (Rust) — RTMP protocol handling, TCP server, FLV demux
2. **video-pipeline** (Rust) — VideoToolbox H.264 decode, IOSurface output
3. **Camera Extension** (Swift) — CoreMediaIO virtual camera exposed to system

IPC: IOSurface for video frames (cross-process GPU memory), POSIX shared memory for control (ring buffer of IOSurfaceIDs at `/rtmp_vcam_ring`).

## Project Structure
```
rtmp-vcam/
├── Cargo.toml                    # Workspace root
├── Makefile                      # Build orchestration
├── crates/
│   ├── rtmp-server/              # RTMP protocol + TCP server
│   │   └── src/
│   │       ├── lib.rs            # Public API
│   │       ├── server.rs         # Tokio TCP listener + accept loop
│   │       ├── session.rs        # rml_rtmp ServerSession wrapper
│   │       ├── handshake.rs      # RTMP handshake state machine
│   │       └── flv.rs            # FLV demux → H.264 NAL units + SPS/PPS
│   ├── video-pipeline/           # VideoToolbox decode + IOSurface
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── ffi.rs            # Raw FFI to Apple frameworks
│   │       ├── decoder.rs        # VTDecompressionSession H.264 decode
│   │       ├── format.rs         # CMFormatDescription from SPS/PPS
│   │       └── surface_pool.rs   # Atomic ring buffer of IOSurfaceIDs
│   └── rtmp-vcam-app/            # Main binary
│       └── src/
│           ├── main.rs           # CLI entry, wires RTMP → decoder → IPC
│           └── ipc.rs            # POSIX shared memory writer
└── swift/
    └── CameraExtension/          # Swift Camera Extension (Xcode project)
        ├── Extension/
        │   ├── main.swift        # Extension entry point
        │   ├── Provider.swift    # CMIOExtensionProviderSource
        │   ├── Device.swift      # CMIOExtensionDeviceSource
        │   ├── Stream.swift      # Frame delivery via IOSurface → CMSampleBuffer
        │   └── shm_shim.h        # C shim for shm_open (Swift can't call variadic C)
        ├── HostApp/
        │   └── AppDelegate.swift # Install/uninstall system extension UI
        └── Entitlements/
            ├── HostApp.entitlements
            └── Extension.entitlements
```

## Bundle Identifiers
- Host app: `com.rtmpvcam.host`
- Camera Extension: `com.rtmpvcam.host.camera-extension`

## Build Commands
```bash
make build-all          # Build everything (Rust + Swift)
make build-rust         # Build Rust crates only
make build-swift        # Build Swift extension only
cargo build             # Rust debug build
cargo build --release   # Rust release build
cargo test              # Run all Rust tests
make run                # Build and run (default port 1935)
```

## Test Commands
```bash
# Run Rust tests
cargo test

# Manual RTMP test (requires ffmpeg)
cargo run --release -- --port 1935
ffmpeg -re -i test.mp4 -c:v libx264 -f flv rtmp://localhost/live/test

# Check virtual camera visibility
# Open FaceTime or System Settings → Privacy & Security → Camera
```

## Key Conventions
- Rust async runtime: `tokio` (multi-threaded)
- Apple framework FFI: Direct C FFI (`extern "C"` blocks in `video-pipeline/src/ffi.rs`), not objc2 crates
- Swift code is minimal — only Camera Extension boilerplate (~400 LOC)
- All logging via `tracing` crate (Rust) and `os.log` (Swift)
- IPC: POSIX shared memory ring buffer at `/rtmp_vcam_ring`
- RTMP protocol parsing: `rml_rtmp` crate

## Dependencies
### Rust
- `rml_rtmp` — RTMP protocol parsing
- `tokio` — Async TCP server
- `bytes` — Byte buffer handling
- `libc` — POSIX shared memory (shm_open, mmap)
- `tracing`, `tracing-subscriber` — Structured logging
- Raw FFI to: CoreFoundation, CoreMedia, VideoToolbox, CoreVideo, IOSurface

### Swift
- CoreMediaIO (CMIOExtension)
- SystemExtensions (OSSystemExtensionRequest)
- IOSurface, CoreMedia, CoreVideo

## CLI Usage
```
rtmp-vcam-app [OPTIONS]

Options:
  -p, --port <PORT>    RTMP listen port (default: 1935)
  -v, --verbose        Enable debug logging
  -h, --help           Show help
```

## Common Tasks

### Install Camera Extension
1. Build: `make build-all`
2. Move RTMPVirtualCamera.app to /Applications
3. Launch the app and click "Install Extension"
4. Approve in System Settings → Privacy & Security

### Debug Camera Extension
```bash
log stream --predicate 'subsystem == "com.rtmpvcam.host.camera-extension"'
```

### Uninstall Camera Extension
Launch the host app and click "Uninstall Extension", or:
```bash
systemextensionsctl uninstall <team-id> com.rtmpvcam.host.camera-extension
```

### End-to-end Test
1. Start Rust server: `cargo run --release`
2. Push RTMP from ffmpeg or MeldStudio to `rtmp://localhost/live/test`
3. Open FaceTime/Zoom → select "RTMP Virtual Camera"
