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
- Xcode 15+ (for building from source)
- Rust toolchain (`rustup`)
- An Apple Developer account (code signing required for system extensions)

## Quick Start (from DMG)

1. Download the latest `.dmg` from [Releases](https://github.com/wmhegarty/rtmp-vcam/releases)
2. Drag `RTMPVirtualCamera.app` to `/Applications`
3. Launch the app
4. Click **Install Extension** and approve in System Settings → Privacy & Security
5. Click **Start Server**
6. Send video:
   ```bash
   ffmpeg -f lavfi -i testsrc=duration=3600:size=1920x1080:rate=30 \
     -pix_fmt yuv420p -c:v libx264 -b:v 2M -f flv rtmp://localhost/live/test
   ```
7. Open FaceTime/Zoom → select **RTMP Virtual Camera**

## Build from Source

```bash
# Build everything (Rust + Swift)
make build-all

# Install to /Applications
make install

# Run tests
make test
```

The Rust RTMP server binary is embedded inside the app bundle and managed from the UI — no need to run it separately.

## Usage

1. **Launch** `/Applications/RTMPVirtualCamera.app`
2. **Install the camera extension** — click "Install Extension" and approve in System Settings → Privacy & Security
3. **Start the RTMP server** — click "Start Server" (default port 1935)
4. **(Optional) Generate a stream key** — click "Generate Key" to require authentication. The RTMP URL with the key is shown in the app for easy copying.
5. **Send video** from any RTMP source:

```bash
# ffmpeg test pattern
ffmpeg -f lavfi -i testsrc=duration=3600:size=1920x1080:rate=30 \
  -pix_fmt yuv420p -c:v libx264 -b:v 2M -f flv rtmp://localhost/live/YOUR_STREAM_KEY

# Or configure OBS/MeldStudio with:
#   Server: rtmp://localhost/live
#   Stream Key: <key from the app>
```

6. **Select the camera** — open FaceTime, Zoom, or any video app and choose "RTMP Virtual Camera"

## Stream Key Authentication

The server supports optional stream key authentication. When a key is configured, only RTMP clients that publish with the correct stream key are accepted — others are disconnected immediately.

**From the app UI:**
1. Click "Generate Key" to create a random 16-character key
2. The key persists across app restarts (stored in UserDefaults)
3. The full RTMP URL with the key is shown for easy copy/paste
4. Click "Clear" to disable authentication (accept all connections)

**From the CLI:**
```bash
rtmp-vcam-app --stream-key MY_SECRET_KEY
```

When no stream key is configured, the server accepts all connections (backwards-compatible).

## CLI Usage

The server can also be run standalone (without the host app):

```
rtmp-vcam-app [OPTIONS]

Options:
  -p, --port <PORT>          RTMP listen port (default: 1935)
  -k, --stream-key <KEY>     Require stream key for publishing
  -v, --verbose              Enable debug logging
  -h, --help                 Show this help
```

## Troubleshooting

**Camera doesn't appear in apps**
- Make sure the extension is installed: check System Settings → Privacy & Security → Camera
- Try restarting the app that should see the camera
- Check extension logs: `log stream --predicate 'subsystem == "com.rtmpvcam.host.camera-extension"'`

**"Address already in use" error**
- Another process is using port 1935. Find it with `lsof -i :1935` and kill it, or use a different port.

**Video is garbled or not showing**
- Ensure your source uses H.264 with YUV 4:2:0: add `-pix_fmt yuv420p` to your ffmpeg command
- High 4:4:4 Predictive profile is not supported by VideoToolbox

**Stream key rejected**
- Check that your RTMP URL matches the key shown in the app: `rtmp://localhost:<port>/live/<key>`
- In OBS/MeldStudio, the stream key goes in the "Stream Key" field, not the server URL

**Server won't stop / orphaned process**
- The server auto-exits if the host app dies. If a process is stuck: `lsof -i :1935` to find it, then `kill <PID>`

## Project Structure

```
rtmp-vcam/
├── Cargo.toml                    # Rust workspace
├── Makefile                      # Build orchestration
├── scripts/
│   ├── make-dmg.sh               # DMG packaging
│   └── release.sh                # Build + tag + GitHub release
├── crates/
│   ├── rtmp-server/              # RTMP protocol + TCP server
│   ├── video-pipeline/           # VideoToolbox H.264 decode (raw C FFI)
│   └── rtmp-vcam-app/            # Main binary (wires RTMP → decode → IPC)
└── swift/
    └── CameraExtension/          # Xcode project
        ├── HostApp/              # App UI (extension + server management)
        └── Extension/            # CoreMediaIO camera extension
```

## Notes

- FaceTime mirrors the camera preview (like a real webcam). The remote viewer sees it correctly.
- The camera extension runs in a sandboxed process — IPC uses file-backed mmap under `/Library/Application Support/` since POSIX shared memory and IOSurface are blocked by the sandbox.
- The server process auto-exits if the host app is killed, preventing orphaned processes.

## License

[MIT](LICENSE) — free to use, modify, and distribute with attribution.
