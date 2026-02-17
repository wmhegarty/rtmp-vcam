//! Raw FFI bindings to Apple frameworks: CoreMedia, VideoToolbox, CoreVideo, IOSurface.
//!
//! These are stable C APIs — we bind them directly rather than going through objc2.

#![allow(non_snake_case, non_upper_case_globals, dead_code)]

use std::ffi::c_void;
use std::os::raw::c_int;

// ── Opaque types ──

pub type CFAllocatorRef = *const c_void;
pub type CFDictionaryRef = *const c_void;
pub type CFMutableDictionaryRef = *mut c_void;
pub type CFStringRef = *const c_void;
pub type CFNumberRef = *const c_void;
pub type CFBooleanRef = *const c_void;
pub type CFTypeRef = *const c_void;

pub type CMFormatDescriptionRef = *mut c_void;
pub type CMVideoFormatDescriptionRef = CMFormatDescriptionRef;
pub type CMSampleBufferRef = *mut c_void;
pub type CMBlockBufferRef = *mut c_void;

pub type VTDecompressionSessionRef = *mut c_void;
pub type VTDecompressionOutputCallbackRecord = DecompressionOutputCallbackRecord;

pub type CVPixelBufferRef = *mut c_void;
pub type CVImageBufferRef = CVPixelBufferRef;

pub type IOSurfaceRef = *mut c_void;
pub type IOSurfaceID = u32;

pub type OSStatus = i32;

// ── CoreFoundation constants ──

pub const kCFAllocatorDefault: CFAllocatorRef = std::ptr::null();
pub const kCFBooleanTrue: CFBooleanRef = unsafe { &_kCFBooleanTrue as *const _ as CFBooleanRef };

pub const kCFNumberSInt32Type: isize = 3;

// ── CMTime ──

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct CMTime {
    pub value: i64,
    pub timescale: i32,
    pub flags: u32,
    pub epoch: i64,
}

impl CMTime {
    pub fn make(value: i64, timescale: i32) -> Self {
        CMTime {
            value,
            timescale,
            flags: 1, // kCMTimeFlags_Valid
            epoch: 0,
        }
    }

    pub fn invalid() -> Self {
        CMTime {
            value: 0,
            timescale: 0,
            flags: 0,
            epoch: 0,
        }
    }
}

// ── Decompression callback ──

pub type VTDecompressionOutputCallback = unsafe extern "C" fn(
    decompressionOutputRefCon: *mut c_void,
    sourceFrameRefCon: *mut c_void,
    status: OSStatus,
    infoFlags: u32,
    imageBuffer: CVImageBufferRef,
    presentationTimeStamp: CMTime,
    presentationDuration: CMTime,
);

#[repr(C)]
pub struct DecompressionOutputCallbackRecord {
    pub decompressionOutputCallback: VTDecompressionOutputCallback,
    pub decompressionOutputRefCon: *mut c_void,
}

// ── CoreFoundation ──

extern "C" {
    static _kCFBooleanTrue: u8;

    pub fn CFRetain(cf: CFTypeRef) -> CFTypeRef;
    pub fn CFRelease(cf: CFTypeRef);

    pub fn CFDictionaryCreateMutable(
        allocator: CFAllocatorRef,
        capacity: isize,
        keyCallBacks: *const c_void,
        valueCallBacks: *const c_void,
    ) -> CFMutableDictionaryRef;

    pub fn CFDictionarySetValue(
        theDict: CFMutableDictionaryRef,
        key: *const c_void,
        value: *const c_void,
    );

    pub fn CFNumberCreate(
        allocator: CFAllocatorRef,
        theType: isize,
        valuePtr: *const c_void,
    ) -> CFNumberRef;

    // Standard CF dictionary callbacks
    pub static kCFTypeDictionaryKeyCallBacks: [u8; 0];
    pub static kCFTypeDictionaryValueCallBacks: [u8; 0];
}

// ── CoreMedia ──

extern "C" {
    pub fn CMVideoFormatDescriptionCreateFromH264ParameterSets(
        allocator: CFAllocatorRef,
        parameterSetCount: usize,
        parameterSetPointers: *const *const u8,
        parameterSetSizes: *const usize,
        nalUnitHeaderLength: c_int,
        formatDescriptionOut: *mut CMVideoFormatDescriptionRef,
    ) -> OSStatus;

    pub fn CMSampleBufferCreateReady(
        allocator: CFAllocatorRef,
        dataBuffer: CMBlockBufferRef,
        formatDescription: CMFormatDescriptionRef,
        numSamples: isize,
        numSampleTimingEntries: isize,
        sampleTimingArray: *const CMSampleTimingInfo,
        numSampleSizeEntries: isize,
        sampleSizeArray: *const usize,
        sampleBufferOut: *mut CMSampleBufferRef,
    ) -> OSStatus;

    pub fn CMBlockBufferCreateWithMemoryBlock(
        allocator: CFAllocatorRef,
        memoryBlock: *const c_void,
        blockLength: usize,
        blockAllocator: CFAllocatorRef,
        customBlockSource: *const c_void,
        offsetToData: usize,
        dataLength: usize,
        flags: u32,
        blockBufferOut: *mut CMBlockBufferRef,
    ) -> OSStatus;

    pub fn CMBlockBufferReplaceDataBytes(
        sourceBytes: *const c_void,
        destinationBuffer: CMBlockBufferRef,
        offsetIntoDestination: usize,
        dataLength: usize,
    ) -> OSStatus;
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct CMSampleTimingInfo {
    pub duration: CMTime,
    pub presentationTimeStamp: CMTime,
    pub decodeTimeStamp: CMTime,
}

// ── VideoToolbox ──

extern "C" {
    pub fn VTDecompressionSessionCreate(
        allocator: CFAllocatorRef,
        videoFormatDescription: CMVideoFormatDescriptionRef,
        videoDecoderSpecification: CFDictionaryRef,
        destinationImageBufferAttributes: CFDictionaryRef,
        outputCallback: *const DecompressionOutputCallbackRecord,
        decompressionSessionOut: *mut VTDecompressionSessionRef,
    ) -> OSStatus;

    pub fn VTDecompressionSessionDecodeFrame(
        session: VTDecompressionSessionRef,
        sampleBuffer: CMSampleBufferRef,
        decodeFlags: u32,
        sourceFrameRefCon: *mut c_void,
        infoFlagsOut: *mut u32,
    ) -> OSStatus;

    pub fn VTDecompressionSessionWaitForAsynchronousFrames(
        session: VTDecompressionSessionRef,
    ) -> OSStatus;

    pub fn VTDecompressionSessionInvalidate(session: VTDecompressionSessionRef);

    // Pixel buffer attributes keys
    pub static kCVPixelBufferPixelFormatTypeKey: CFStringRef;
    pub static kCVPixelBufferIOSurfacePropertiesKey: CFStringRef;
    pub static kCVPixelBufferWidthKey: CFStringRef;
    pub static kCVPixelBufferHeightKey: CFStringRef;
}

// ── CoreVideo ──

/// kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange = '420v' = 0x34323076
pub const kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange: u32 = 0x34323076;

pub type CVReturn = i32;
pub const kCVReturnSuccess: CVReturn = 0;

/// CVPixelBufferLockFlags
pub const kCVPixelBufferLock_ReadOnly: u64 = 0x00000001;

extern "C" {
    pub fn CVPixelBufferGetIOSurface(pixelBuffer: CVPixelBufferRef) -> IOSurfaceRef;
    pub fn CVPixelBufferGetWidth(pixelBuffer: CVPixelBufferRef) -> usize;
    pub fn CVPixelBufferGetHeight(pixelBuffer: CVPixelBufferRef) -> usize;
    pub fn CVPixelBufferLockBaseAddress(
        pixelBuffer: CVPixelBufferRef,
        lockFlags: u64,
    ) -> CVReturn;
    pub fn CVPixelBufferUnlockBaseAddress(
        pixelBuffer: CVPixelBufferRef,
        lockFlags: u64,
    ) -> CVReturn;
    pub fn CVPixelBufferGetBaseAddressOfPlane(
        pixelBuffer: CVPixelBufferRef,
        planeIndex: usize,
    ) -> *const u8;
    pub fn CVPixelBufferGetBytesPerRowOfPlane(
        pixelBuffer: CVPixelBufferRef,
        planeIndex: usize,
    ) -> usize;
    pub fn CVPixelBufferGetHeightOfPlane(
        pixelBuffer: CVPixelBufferRef,
        planeIndex: usize,
    ) -> usize;
}

// ── IOSurface ──

extern "C" {
    pub fn IOSurfaceGetID(buffer: IOSurfaceRef) -> IOSurfaceID;
    pub fn IOSurfaceLookup(csid: IOSurfaceID) -> IOSurfaceRef;
}

// ── Link directives ──

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {}

#[link(name = "CoreMedia", kind = "framework")]
extern "C" {}

#[link(name = "VideoToolbox", kind = "framework")]
extern "C" {}

#[link(name = "CoreVideo", kind = "framework")]
extern "C" {}

#[link(name = "IOSurface", kind = "framework")]
extern "C" {}
