use std::ffi::c_void;

use tracing::debug;

use crate::ffi;

/// Wraps a CMVideoFormatDescription created from H.264 SPS/PPS parameter sets.
pub struct FormatDescription {
    inner: ffi::CMVideoFormatDescriptionRef,
}

impl FormatDescription {
    /// Create a CMVideoFormatDescription from H.264 SPS and PPS NAL units.
    pub fn from_h264_parameter_sets(
        sps_list: &[Vec<u8>],
        pps_list: &[Vec<u8>],
        nalu_length_size: u8,
    ) -> Result<Self, i32> {
        // Collect all parameter set pointers and sizes
        let mut pointers: Vec<*const u8> = Vec::with_capacity(sps_list.len() + pps_list.len());
        let mut sizes: Vec<usize> = Vec::with_capacity(sps_list.len() + pps_list.len());

        for sps in sps_list {
            pointers.push(sps.as_ptr());
            sizes.push(sps.len());
        }
        for pps in pps_list {
            pointers.push(pps.as_ptr());
            sizes.push(pps.len());
        }

        let mut format_desc: ffi::CMVideoFormatDescriptionRef = std::ptr::null_mut();

        let status = unsafe {
            ffi::CMVideoFormatDescriptionCreateFromH264ParameterSets(
                ffi::kCFAllocatorDefault,
                pointers.len(),
                pointers.as_ptr(),
                sizes.as_ptr(),
                nalu_length_size as i32,
                &mut format_desc,
            )
        };

        if status != 0 {
            tracing::error!(status, "CMVideoFormatDescriptionCreateFromH264ParameterSets failed");
            return Err(status);
        }

        debug!("created CMVideoFormatDescription from {} parameter sets", pointers.len());
        Ok(FormatDescription { inner: format_desc })
    }

    pub fn as_ref(&self) -> ffi::CMVideoFormatDescriptionRef {
        self.inner
    }
}

impl Drop for FormatDescription {
    fn drop(&mut self) {
        if !self.inner.is_null() {
            unsafe { ffi::CFRelease(self.inner as *const c_void) };
        }
    }
}

// SAFETY: CMVideoFormatDescription is a CF type that is thread-safe for read access.
unsafe impl Send for FormatDescription {}
unsafe impl Sync for FormatDescription {}
