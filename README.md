# RTMP Virtual Camera for macOS

Receive RTMP video streams (e.g., from OBS, MeldStudio, or ffmpeg) and expose them as a native macOS virtual camera. Any app that uses a webcam — FaceTime, Zoom, Google Meet, QuickTime — can select "RTMP Virtual Camera" as a camera source.

## Installation

1. Download the latest `.dmg` from [Releases](https://github.com/wmhegarty/rtmp-vcam/releases)
2. Open the DMG and drag **RTMPVirtualCamera** to the **Applications** folder
3. Launch **RTMPVirtualCamera** from `/Applications`

> **First launch:** macOS may show a security prompt. If so, go to **System Settings → Privacy & Security** and click **Open Anyway**.

## Setup (one-time)

1. In the app, click **Install Extension**
2. macOS will ask you to approve the system extension — go to **System Settings → Privacy & Security** and allow it
3. The camera extension is now installed and will persist across reboots

## Usage

1. Click **Start Server** (default port 1935)
2. **(Optional)** Click **Generate Key** to require a stream key for authentication
3. Point your RTMP source at the URL shown in the app:

**From OBS / MeldStudio:**
- Server: `rtmp://localhost/live`
- Stream Key: the key shown in the app (or anything if no key is set)

**From ffmpeg:**
```bash
# Test pattern
ffmpeg -f lavfi -i testsrc=duration=3600:size=1920x1080:rate=30 \
  -pix_fmt yuv420p -c:v libx264 -b:v 2M -f flv rtmp://localhost/live/YOUR_STREAM_KEY

# Stream a file
ffmpeg -re -i video.mp4 -c:v libx264 -pix_fmt yuv420p \
  -f flv rtmp://localhost/live/YOUR_STREAM_KEY
```

4. Open **FaceTime**, **Zoom**, **Google Meet**, or any video app and select **RTMP Virtual Camera** as your camera

## Uninstalling

1. Launch the app and click **Uninstall Extension**
2. Approve the removal in System Settings if prompted
3. Delete `RTMPVirtualCamera.app` from `/Applications`

---

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

## Requirements (building from source)

- macOS 12.3+
- Xcode 15+
- Rust toolchain (`rustup`)
- An Apple Developer account (code signing required for system extensions)

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
