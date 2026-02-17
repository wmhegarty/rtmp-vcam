use std::ffi::CString;
use std::io;
use std::path::PathBuf;
use std::ptr;

use tracing::info;

use video_pipeline::FRAME_SHM_SIZE;

/// Ring buffer file path — must be accessible to both the Rust process (as user)
/// and the sandboxed CMIO extension (as _cmiodalassistants).
/// The cmioextension sandbox allows: (allow file-read* (subpath "/Library"))
/// so we use /Library/Application Support/RTMPVirtualCamera/.
const RING_FILE_PATH: &str = "/Library/Application Support/RTMPVirtualCamera/rtmp_vcam_ring";

/// File-backed mmap shared memory for publishing decoded NV12 frames
/// to the Swift Camera Extension.
///
/// Layout (see video_pipeline::decoder for constants):
///   Header (64 bytes):
///     [0..8)   write_index (u64, atomic)
///     [8..12)  width (u32)
///     [12..16) height (u32)
///     [16..64) reserved
///   Frame data (double-buffered):
///     [64 .. 64+MAX_FRAME_SIZE)              frame buffer 0
///     [64+MAX_FRAME_SIZE .. 64+2*MAX_FRAME_SIZE) frame buffer 1
pub struct SharedFrameBuffer {
    ptr: *mut u8,
    fd: i32,
    path: PathBuf,
}

// SAFETY: The shared memory region uses atomic operations for synchronization.
unsafe impl Send for SharedFrameBuffer {}
unsafe impl Sync for SharedFrameBuffer {}

impl SharedFrameBuffer {
    /// Create and map the shared frame buffer file.
    pub fn create() -> io::Result<Self> {
        let ring_path = PathBuf::from(RING_FILE_PATH);

        let c_path = CString::new(RING_FILE_PATH)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains null"))?;

        unsafe {
            let fd = libc::open(
                c_path.as_ptr(),
                libc::O_CREAT | libc::O_RDWR,
                0o644,
            );
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }

            // Set size for double-buffered frame data
            if libc::ftruncate(fd, FRAME_SHM_SIZE as libc::off_t) != 0 {
                let err = io::Error::last_os_error();
                libc::close(fd);
                return Err(err);
            }

            // Map into our address space
            let ptr = libc::mmap(
                ptr::null_mut(),
                FRAME_SHM_SIZE,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            );
            if ptr == libc::MAP_FAILED {
                let err = io::Error::last_os_error();
                libc::close(fd);
                return Err(err);
            }

            // Zero-initialize header (frame data doesn't need zeroing)
            ptr::write_bytes(ptr as *mut u8, 0, video_pipeline::FRAME_HEADER_SIZE);

            info!(
                path = %ring_path.display(),
                size = FRAME_SHM_SIZE,
                "frame buffer created"
            );
            Ok(SharedFrameBuffer {
                ptr: ptr as *mut u8,
                fd,
                path: ring_path,
            })
        }
    }

    /// Get the raw pointer to the shared memory region.
    /// The decoder callback writes directly to this pointer.
    pub fn ptr(&self) -> *mut u8 {
        self.ptr
    }
}

impl Drop for SharedFrameBuffer {
    fn drop(&mut self) {
        unsafe {
            if !self.ptr.is_null() {
                libc::munmap(self.ptr as *mut libc::c_void, FRAME_SHM_SIZE);
            }
            if self.fd >= 0 {
                libc::close(self.fd);
            }
        }
        info!("frame buffer closed: {}", self.path.display());
    }
}
