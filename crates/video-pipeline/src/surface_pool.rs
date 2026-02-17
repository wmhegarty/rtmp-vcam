use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::ffi;

/// Ring buffer of IOSurface IDs for cross-process sharing.
///
/// The Rust decoder writes decoded surface IDs here.
/// The Swift Camera Extension reads the latest surface ID.
///
/// Retains IOSurfaceRef objects to keep them alive for cross-process lookup.
pub struct SurfaceRing {
    inner: Arc<SurfaceRingInner>,
}

struct SurfaceRingInner {
    /// Current write index (monotonically increasing, mod RING_SIZE to get slot).
    write_index: AtomicU64,
    /// Ring buffer of surface IDs.
    surface_ids: [AtomicU32; RING_SIZE],
    /// Ring buffer of timestamps (milliseconds).
    timestamps: [AtomicU64; RING_SIZE],
    /// Retained IOSurfaceRef objects — keeps surfaces alive for cross-process IOSurfaceLookup.
    retained_surfaces: Mutex<[ffi::IOSurfaceRef; RING_SIZE]>,
}

const RING_SIZE: usize = 8;

impl SurfaceRing {
    pub fn new() -> Self {
        SurfaceRing {
            inner: Arc::new(SurfaceRingInner {
                write_index: AtomicU64::new(0),
                surface_ids: std::array::from_fn(|_| AtomicU32::new(0)),
                timestamps: std::array::from_fn(|_| AtomicU64::new(0)),
                retained_surfaces: Mutex::new([std::ptr::null_mut(); RING_SIZE]),
            }),
        }
    }

    /// Push a new decoded frame's IOSurface into the ring.
    /// Retains the IOSurface to keep it alive for cross-process lookup.
    pub fn push(&self, surface_id: ffi::IOSurfaceID, timestamp_ms: u64, surface: ffi::IOSurfaceRef) {
        let idx = self.inner.write_index.load(Ordering::Relaxed) as usize % RING_SIZE;

        // Retain the new surface and release the old one
        if !surface.is_null() {
            unsafe { ffi::CFRetain(surface as *const c_void) };
        }
        {
            let mut surfaces = self.inner.retained_surfaces.lock().unwrap();
            let old = surfaces[idx];
            surfaces[idx] = surface;
            if !old.is_null() {
                unsafe { ffi::CFRelease(old as *const c_void) };
            }
        }

        self.inner.surface_ids[idx].store(surface_id, Ordering::Release);
        self.inner.timestamps[idx].store(timestamp_ms, Ordering::Release);
        self.inner.write_index.fetch_add(1, Ordering::Release);
    }

    /// Read the latest surface ID and timestamp.
    /// Returns None if no frames have been written yet.
    pub fn latest(&self) -> Option<(ffi::IOSurfaceID, u64)> {
        let write_idx = self.inner.write_index.load(Ordering::Acquire);
        if write_idx == 0 {
            return None;
        }
        let idx = (write_idx - 1) as usize % RING_SIZE;
        let surface_id = self.inner.surface_ids[idx].load(Ordering::Acquire);
        let timestamp = self.inner.timestamps[idx].load(Ordering::Acquire);
        if surface_id == 0 {
            return None;
        }
        Some((surface_id, timestamp))
    }

    /// Get the current write count (for detecting new frames).
    pub fn write_count(&self) -> u64 {
        self.inner.write_index.load(Ordering::Acquire)
    }

    pub fn clone_ref(&self) -> Self {
        SurfaceRing {
            inner: Arc::clone(&self.inner),
        }
    }
}

// SAFETY: All fields use atomics or Mutex.
unsafe impl Send for SurfaceRing {}
unsafe impl Sync for SurfaceRing {}

impl Drop for SurfaceRingInner {
    fn drop(&mut self) {
        let surfaces = self.retained_surfaces.get_mut().unwrap();
        for surface in surfaces.iter_mut() {
            if !surface.is_null() {
                unsafe { ffi::CFRelease(*surface as *const c_void) };
                *surface = std::ptr::null_mut();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_empty() {
        let ring = SurfaceRing::new();
        assert!(ring.latest().is_none());
        assert_eq!(ring.write_count(), 0);
    }

    #[test]
    fn test_ring_push_read() {
        let ring = SurfaceRing::new();
        ring.push(42, 1000, std::ptr::null_mut());
        let (id, ts) = ring.latest().unwrap();
        assert_eq!(id, 42);
        assert_eq!(ts, 1000);
        assert_eq!(ring.write_count(), 1);
    }

    #[test]
    fn test_ring_overwrap() {
        let ring = SurfaceRing::new();
        for i in 0..20u32 {
            ring.push(i + 1, (i as u64 + 1) * 33, std::ptr::null_mut());
        }
        let (id, ts) = ring.latest().unwrap();
        assert_eq!(id, 20);
        assert_eq!(ts, 20 * 33);
        assert_eq!(ring.write_count(), 20);
    }
}
