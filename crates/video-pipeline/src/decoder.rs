use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};

use tracing::{debug, error, trace, warn};

use crate::ffi;
use crate::format::FormatDescription;

/// Shared frame buffer layout constants.
/// Must match the Swift extension side.
pub const FRAME_HEADER_SIZE: usize = 64;
pub const MAX_WIDTH: usize = 1920;
pub const MAX_HEIGHT: usize = 1080;
pub const MAX_FRAME_SIZE: usize = MAX_WIDTH * MAX_HEIGHT * 3 / 2; // NV12
pub const FRAME_SHM_SIZE: usize = FRAME_HEADER_SIZE + 2 * MAX_FRAME_SIZE; // double-buffered

/// H.264 hardware decoder using Apple VideoToolbox.
///
/// Decodes H.264 NAL units into CVPixelBuffers and copies pixel data
/// to a shared memory region for the Camera Extension to read.
pub struct H264Decoder {
    session: ffi::VTDecompressionSessionRef,
    format_desc: FormatDescription,
    _ctx: *mut CallbackContext, // prevent premature free
}

/// Context passed to the VT decompression callback.
struct CallbackContext {
    shm_ptr: *mut u8,
}

// SAFETY: shm_ptr points to a memory-mapped region that is valid for the lifetime of the decoder.
unsafe impl Send for CallbackContext {}
unsafe impl Sync for CallbackContext {}

impl H264Decoder {
    /// Create a new decoder from SPS/PPS parameter sets.
    ///
    /// `shm_ptr` must point to a shared memory region of at least `FRAME_SHM_SIZE` bytes,
    /// valid for the lifetime of the decoder.
    pub fn new(
        sps_list: &[Vec<u8>],
        pps_list: &[Vec<u8>],
        nalu_length_size: u8,
        shm_ptr: *mut u8,
    ) -> Result<Self, String> {
        let format_desc =
            FormatDescription::from_h264_parameter_sets(sps_list, pps_list, nalu_length_size)
                .map_err(|s| format!("failed to create format description: OSStatus {s}"))?;

        // Build destination image buffer attributes
        let dest_attrs = unsafe { create_destination_attributes() };

        // Build callback
        let ctx = Box::new(CallbackContext { shm_ptr });
        let ctx_ptr = Box::into_raw(ctx);

        let callback = ffi::DecompressionOutputCallbackRecord {
            decompressionOutputCallback: decompression_callback,
            decompressionOutputRefCon: ctx_ptr as *mut c_void,
        };

        let mut session: ffi::VTDecompressionSessionRef = std::ptr::null_mut();
        let status = unsafe {
            ffi::VTDecompressionSessionCreate(
                ffi::kCFAllocatorDefault,
                format_desc.as_ref(),
                std::ptr::null(),       // videoDecoderSpecification
                dest_attrs,             // destinationImageBufferAttributes
                &callback,
                &mut session,
            )
        };

        // Clean up dest_attrs
        if !dest_attrs.is_null() {
            unsafe { ffi::CFRelease(dest_attrs as *const c_void) };
        }

        if status != 0 {
            // Clean up the leaked context
            unsafe { drop(Box::from_raw(ctx_ptr)) };
            return Err(format!(
                "VTDecompressionSessionCreate failed: OSStatus {status}"
            ));
        }

        debug!("VTDecompressionSession created");
        Ok(H264Decoder {
            session,
            format_desc,
            _ctx: ctx_ptr,
        })
    }

    /// Decode AVCC-framed video data containing one or more NAL units.
    /// Data must be in AVCC format: [4-byte len][NAL1][4-byte len][NAL2]...
    pub fn decode_avcc(&mut self, avcc_data: &[u8], timestamp_ms: u32) -> Result<(), String> {
        // Create CMBlockBuffer — let CoreMedia allocate and own the memory,
        // then copy our data in, to avoid memory ownership issues.
        let mut block_buffer: ffi::CMBlockBufferRef = std::ptr::null_mut();
        let status = unsafe {
            ffi::CMBlockBufferCreateWithMemoryBlock(
                ffi::kCFAllocatorDefault,
                std::ptr::null(),           // NULL = CoreMedia allocates
                avcc_data.len(),
                ffi::kCFAllocatorDefault,   // allocator for the block
                std::ptr::null(),           // no custom block source
                0,                          // offset
                avcc_data.len(),
                0,                          // flags
                &mut block_buffer,
            )
        };
        if status != 0 {
            return Err(format!("CMBlockBufferCreateWithMemoryBlock failed: {status}"));
        }

        // Copy AVCC data into the CoreMedia-owned block
        let status = unsafe {
            ffi::CMBlockBufferReplaceDataBytes(
                avcc_data.as_ptr() as *const c_void,
                block_buffer,
                0,
                avcc_data.len(),
            )
        };
        if status != 0 {
            unsafe { ffi::CFRelease(block_buffer as *const c_void) };
            return Err(format!("CMBlockBufferReplaceDataBytes failed: {status}"));
        }

        // Create CMSampleBuffer
        let timing = ffi::CMSampleTimingInfo {
            duration: ffi::CMTime::make(1, 30), // 1/30s
            presentationTimeStamp: ffi::CMTime::make(timestamp_ms as i64, 1000),
            decodeTimeStamp: ffi::CMTime::invalid(),
        };
        let sample_size = avcc_data.len();

        let mut sample_buffer: ffi::CMSampleBufferRef = std::ptr::null_mut();
        let status = unsafe {
            ffi::CMSampleBufferCreateReady(
                ffi::kCFAllocatorDefault,
                block_buffer,
                self.format_desc.as_ref(),
                1,     // numSamples
                1,     // numSampleTimingEntries
                &timing,
                1,     // numSampleSizeEntries
                &sample_size,
                &mut sample_buffer,
            )
        };

        // Release block buffer (sample buffer retains it)
        unsafe { ffi::CFRelease(block_buffer as *const c_void) };

        if status != 0 {
            return Err(format!("CMSampleBufferCreateReady failed: {status}"));
        }

        // Decode
        let mut info_flags: u32 = 0;
        let status = unsafe {
            ffi::VTDecompressionSessionDecodeFrame(
                self.session,
                sample_buffer,
                0, // decodeFlags: synchronous
                std::ptr::null_mut(), // sourceFrameRefCon
                &mut info_flags,
            )
        };

        // Release sample buffer
        unsafe { ffi::CFRelease(sample_buffer as *const c_void) };

        if status != 0 {
            // -8969 = kVTVideoDecoderBadDataErr (common for incomplete frames)
            if status == -8969 {
                trace!(status, "decode frame returned bad data (may be expected for partial frames)");
            } else {
                warn!(status, "VTDecompressionSessionDecodeFrame failed");
            }
            return Err(format!("VTDecompressionSessionDecodeFrame failed: {status}"));
        }

        trace!(timestamp_ms, "decoded frame");
        Ok(())
    }

    /// Flush the decoder — wait for all pending frames.
    pub fn flush(&self) -> Result<(), String> {
        let status = unsafe {
            ffi::VTDecompressionSessionWaitForAsynchronousFrames(self.session)
        };
        if status != 0 {
            return Err(format!("WaitForAsynchronousFrames failed: {status}"));
        }
        Ok(())
    }
}

impl Drop for H264Decoder {
    fn drop(&mut self) {
        if !self.session.is_null() {
            unsafe {
                ffi::VTDecompressionSessionInvalidate(self.session);
                ffi::CFRelease(self.session as *const c_void);
            }
        }
        // Clean up callback context
        if !self._ctx.is_null() {
            unsafe { drop(Box::from_raw(self._ctx)) };
        }
    }
}

// SAFETY: VTDecompressionSession is internally thread-safe for decode calls.
unsafe impl Send for H264Decoder {}

/// Create destination pixel buffer attributes dictionary.
///
/// Requests IOSurface-backed NV12 pixel buffers.
unsafe fn create_destination_attributes() -> ffi::CFDictionaryRef {
    let dict = ffi::CFDictionaryCreateMutable(
        ffi::kCFAllocatorDefault,
        4,
        &ffi::kCFTypeDictionaryKeyCallBacks as *const _ as *const c_void,
        &ffi::kCFTypeDictionaryValueCallBacks as *const _ as *const c_void,
    );

    // Pixel format: NV12 (420v)
    let pixel_format = ffi::kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange as i32;
    let pixel_format_num = ffi::CFNumberCreate(
        ffi::kCFAllocatorDefault,
        ffi::kCFNumberSInt32Type,
        &pixel_format as *const i32 as *const c_void,
    );
    ffi::CFDictionarySetValue(
        dict,
        ffi::kCVPixelBufferPixelFormatTypeKey as *const c_void,
        pixel_format_num as *const c_void,
    );
    ffi::CFRelease(pixel_format_num as *const c_void);

    // IOSurface backing (empty dictionary = yes, use IOSurface)
    let io_surface_props = ffi::CFDictionaryCreateMutable(
        ffi::kCFAllocatorDefault,
        0,
        &ffi::kCFTypeDictionaryKeyCallBacks as *const _ as *const c_void,
        &ffi::kCFTypeDictionaryValueCallBacks as *const _ as *const c_void,
    );
    ffi::CFDictionarySetValue(
        dict,
        ffi::kCVPixelBufferIOSurfacePropertiesKey as *const c_void,
        io_surface_props as *const c_void,
    );
    ffi::CFRelease(io_surface_props as *const c_void);

    dict as ffi::CFDictionaryRef
}

/// VTDecompressionSession output callback.
///
/// Called by VideoToolbox when a frame has been decoded.
/// Copies raw NV12 pixel data from the CVPixelBuffer into shared memory
/// for the Camera Extension to read.
#[allow(non_snake_case)]
unsafe extern "C" fn decompression_callback(
    decompressionOutputRefCon: *mut c_void,
    _sourceFrameRefCon: *mut c_void,
    status: ffi::OSStatus,
    _infoFlags: u32,
    imageBuffer: ffi::CVImageBufferRef,
    _presentationTimeStamp: ffi::CMTime,
    _presentationDuration: ffi::CMTime,
) {
    if status != 0 {
        error!(status, "decompression callback received error");
        return;
    }

    if imageBuffer.is_null() {
        warn!("decompression callback received null imageBuffer");
        return;
    }

    let ctx = &*(decompressionOutputRefCon as *const CallbackContext);
    let shm = ctx.shm_ptr;

    // Lock the pixel buffer for read access
    let lock_status = ffi::CVPixelBufferLockBaseAddress(
        imageBuffer,
        ffi::kCVPixelBufferLock_ReadOnly,
    );
    if lock_status != ffi::kCVReturnSuccess {
        warn!(lock_status, "CVPixelBufferLockBaseAddress failed");
        return;
    }

    let width = ffi::CVPixelBufferGetWidth(imageBuffer);
    let height = ffi::CVPixelBufferGetHeight(imageBuffer);

    // Clamp to max supported resolution
    if width > MAX_WIDTH || height > MAX_HEIGHT {
        warn!(width, height, "frame exceeds max resolution, skipping");
        ffi::CVPixelBufferUnlockBaseAddress(imageBuffer, ffi::kCVPixelBufferLock_ReadOnly);
        return;
    }

    let frame_size = width * height * 3 / 2; // NV12

    // Determine which double-buffer slot to write to
    let write_index_ptr = shm as *const AtomicU64;
    let write_idx = (*write_index_ptr).load(Ordering::Relaxed);
    let slot = (write_idx as usize) % 2;
    let frame_offset = FRAME_HEADER_SIZE + slot * MAX_FRAME_SIZE;
    let frame_dst = shm.add(frame_offset);

    // Copy Y plane
    let y_src = ffi::CVPixelBufferGetBaseAddressOfPlane(imageBuffer, 0);
    let y_stride = ffi::CVPixelBufferGetBytesPerRowOfPlane(imageBuffer, 0);
    let y_height = ffi::CVPixelBufferGetHeightOfPlane(imageBuffer, 0);

    if !y_src.is_null() {
        if y_stride == width {
            // Fast path: stride matches width, single memcpy
            std::ptr::copy_nonoverlapping(y_src, frame_dst, width * y_height);
        } else {
            // Row-by-row copy to strip padding
            for row in 0..y_height {
                std::ptr::copy_nonoverlapping(
                    y_src.add(row * y_stride),
                    frame_dst.add(row * width),
                    width,
                );
            }
        }
    }

    // Copy UV plane
    let uv_src = ffi::CVPixelBufferGetBaseAddressOfPlane(imageBuffer, 1);
    let uv_stride = ffi::CVPixelBufferGetBytesPerRowOfPlane(imageBuffer, 1);
    let uv_height = ffi::CVPixelBufferGetHeightOfPlane(imageBuffer, 1);
    let uv_dst = frame_dst.add(width * y_height);

    if !uv_src.is_null() {
        if uv_stride == width {
            std::ptr::copy_nonoverlapping(uv_src, uv_dst, width * uv_height);
        } else {
            for row in 0..uv_height {
                std::ptr::copy_nonoverlapping(
                    uv_src.add(row * uv_stride),
                    uv_dst.add(row * width),
                    width,
                );
            }
        }
    }

    // Unlock pixel buffer
    ffi::CVPixelBufferUnlockBaseAddress(imageBuffer, ffi::kCVPixelBufferLock_ReadOnly);

    // Write dimensions to header
    let width_ptr = shm.add(8) as *mut u32;
    let height_ptr = shm.add(12) as *mut u32;
    std::ptr::write_volatile(width_ptr, width as u32);
    std::ptr::write_volatile(height_ptr, height as u32);

    // Increment write_index (atomic, Release ordering) — signals reader that a new frame is ready
    (*write_index_ptr).fetch_add(1, Ordering::Release);

    trace!(width, height, frame_size, slot, "copied frame to shm");
}
