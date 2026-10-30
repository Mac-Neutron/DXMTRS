//! Objects, strings, autorelease pools, dispatch data, device queries and the OS version.
//!
//! Group G1, slots 0 to 8, 10, 24, 27, 28, 49 to 52, 60, 61, 86, 87, 111, 114, 125, 148 and 149.
//! Each handler is exported under the C name the slot tables hold. A +0 result the C wrote
//! straight from a send (`objectAtIndex:`, `name`, `description`, `stringWithCString:encoding:`)
//! goes out through [`ret_passthrough`] of a raw-pointer re-declaration, so the reference counts
//! and the autorelease pool see what they saw with the C. A nil receiver answers the zeros the
//! C's send to nil answered.

use core::{
    ffi::{c_char, c_void},
    ptr::NonNull,
};

use block2::StackBlock;
use dispatch2::{DispatchData, DispatchRetained, dispatch_data_applier_t};
use objc2::{
    AnyThread, extern_class, extern_methods,
    ffi::{objc_release, objc_retain},
    rc::{Allocated, Retained},
    runtime::{AnyObject, NSObject, ProtocolObject},
};
use objc2_foundation::{
    NSArray, NSAutoreleasePool, NSProcessInfo, NSString, NSStringEncoding, NSUInteger,
};
use objc2_metal::{MTLCommandEncoder, MTLCopyAllDevices, MTLDevice, MTLGPUFamily};
use winemetal_wire::{
    UnixcallDispatchdataCopybytes, UnixcallGenericObjNoret, UnixcallGenericObjObjNoret,
    UnixcallGenericObjObjRet, UnixcallGenericObjRet, UnixcallGenericObjUint64Noret,
    UnixcallGenericObjUint64ObjRet, UnixcallGenericObjUint64Ret, UnixcallGenericObjUint64Uint64Ret,
    UnixcallGetOsVersion, UnixcallMtldeviceSampletimestamps, UnixcallNsstringGetcstring,
    UnixcallNsstringString,
};

use crate::{
    ffi::{AbortOnPanic, STATUS_SUCCESS, params},
    float_cast::{u64_to_u32, u64_to_usize},
    format::pixel_format,
    handle::{obj, ret_owned, ret_passthrough},
};

extern_class!(
    /// `NSObject`, re-declared for `-description` returned as a raw pointer.
    #[unsafe(super(NSObject))]
    #[name = "NSObject"]
    struct RawObject;
);

impl RawObject {
    extern_methods!(
        /// `-description`, +0 as the receiver answered it.
        #[unsafe(method(description))]
        #[unsafe(method_family = none)]
        fn description_raw(&self) -> *mut AnyObject;
    );
}

extern_class!(
    /// `NSArray`, re-declared for `-objectAtIndex:` returned as a raw pointer.
    #[unsafe(super(NSObject))]
    #[name = "NSArray"]
    struct RawArray;
);

impl RawArray {
    extern_methods!(
        /// `-objectAtIndex:`, +0 as the array answered it; raises past the end.
        #[unsafe(method(objectAtIndex:))]
        #[unsafe(method_family = none)]
        fn object_at_index_raw(&self, index: NSUInteger) -> *mut AnyObject;
    );
}

extern_class!(
    /// `NSString`, re-declared for the C-string methods taking the guest's pointers as they are.
    #[unsafe(super(NSObject))]
    #[name = "NSString"]
    struct RawString;
);

impl RawString {
    extern_methods!(
        /// `+stringWithCString:encoding:`, autoreleased, nil for bytes the encoding rejects.
        ///
        /// A null `cstr` raises.
        #[unsafe(method(stringWithCString:encoding:))]
        #[unsafe(method_family = none)]
        unsafe fn with_cstring_raw(
            cstr: *const c_char,
            encoding: NSStringEncoding,
        ) -> *mut AnyObject;

        /// `-initWithCString:encoding:`, +1, nil for bytes the encoding rejects.
        #[unsafe(method(initWithCString:encoding:))]
        #[unsafe(method_family = init)]
        unsafe fn init_with_cstring(
            this: Allocated<Self>,
            cstr: *const c_char,
            encoding: NSStringEncoding,
        ) -> Option<Retained<Self>>;

        /// `-getCString:maxLength:encoding:`, writing to the guest's buffer.
        #[unsafe(method(getCString:maxLength:encoding:))]
        #[unsafe(method_family = none)]
        unsafe fn get_cstring(
            &self,
            buffer: *mut c_char,
            max_length: NSUInteger,
            encoding: NSStringEncoding,
        ) -> bool;
    );
}

/// `MTLDevice`'s `-name` returned as a raw pointer.
trait RawDevice {
    /// `-name`, +0 as the device answered it.
    fn name_raw(&self) -> *mut AnyObject;
}

impl RawDevice for ProtocolObject<dyn MTLDevice> {
    extern_methods!(
        #[unsafe(method(name))]
        #[unsafe(method_family = none)]
        fn name_raw(&self) -> *mut AnyObject;
    );
}

/// A guest address as a pointer.
const fn raw<T>(addr: u64) -> *mut T {
    core::ptr::with_exposed_provenance_mut(u64_to_usize(addr))
}

/// The device a handle names, `None` for 0.
///
/// # Safety
///
/// A non-zero `h` is the address of a live object; one that is not a device raises when a
/// device selector reaches it, as it did for the C.
unsafe fn device<'a>(h: u64) -> Option<&'a ProtocolObject<dyn MTLDevice>> {
    // SAFETY: the caller's contract.
    unsafe { obj(h) }
}

/// Slot 0, `NSObject_retain`: retains the object the block names.
///
/// The params block is the handle itself, whatever the object's class; 0 retains nothing.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_NSObject_retain")]
pub unsafe extern "C-unwind" fn nsobject_retain(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjNoret>(args) };
    // SAFETY: a live object or 0, which `objc_retain` ignores.
    unsafe { objc_retain(raw::<AnyObject>(p.handle)) };
    STATUS_SUCCESS
}

/// Slot 1, `NSObject_release`: releases the reference the guest holds.
///
/// The params block is the handle itself. It drains a pool slot 10 made, on the thread that made
/// it, and releases Foundation objects as well as Metal's; 0 releases nothing.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_NSObject_release")]
pub unsafe extern "C-unwind" fn nsobject_release(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjNoret>(args) };
    // SAFETY: a live object the guest owns a reference to, or 0, which `objc_release` ignores.
    unsafe { objc_release(raw::<AnyObject>(p.handle)) };
    STATUS_SUCCESS
}

/// Slot 2, `NSArray_object`: the element at an index, +0.
///
/// An index past the end raises, as `objectAtIndex:` does.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_NSArray_object")]
pub unsafe extern "C-unwind" fn nsarray_object(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjUint64ObjRet>(args) };
    // SAFETY: a live array or 0.
    let array = unsafe { obj::<RawArray>(p.handle) };
    let index = u64_to_usize(p.arg);
    p.ret = array.map_or(0, |a| ret_passthrough(a.object_at_index_raw(index)));
    STATUS_SUCCESS
}

/// Slot 3, `NSArray_count`: the number of elements.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_NSArray_count")]
pub unsafe extern "C-unwind" fn nsarray_count(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjUint64Ret>(args) };
    // SAFETY: a live array or 0.
    let array = unsafe { obj::<NSArray<AnyObject>>(p.handle) };
    p.ret = array.map_or(0, |a| a.count() as u64);
    STATUS_SUCCESS
}

/// Slot 4, `MTLCopyAllDevices`: the array of Metal devices, +1.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLCopyAllDevices")]
pub unsafe extern "C-unwind" fn mtl_copy_all_devices(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjRet>(args) };
    p.ret = ret_owned(Some(MTLCopyAllDevices()));
    STATUS_SUCCESS
}

/// Slot 5, `MTLDevice_recommendedMaxWorkingSetSize`.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLDevice_recommendedMaxWorkingSetSize")]
pub unsafe extern "C-unwind" fn mtldevice_recommended_max_working_set_size(
    args: *mut c_void,
) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjUint64Ret>(args) };
    // SAFETY: a live device or 0.
    let device = unsafe { device(p.handle) };
    p.ret = device.map_or(0, MTLDevice::recommendedMaxWorkingSetSize);
    STATUS_SUCCESS
}

/// Slot 6, `MTLDevice_currentAllocatedSize`.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLDevice_currentAllocatedSize")]
pub unsafe extern "C-unwind" fn mtldevice_current_allocated_size(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjUint64Ret>(args) };
    // SAFETY: a live device or 0.
    let device = unsafe { device(p.handle) };
    p.ret = device.map_or(0, |d| d.currentAllocatedSize() as u64);
    STATUS_SUCCESS
}

/// Slot 7, `MTLDevice_name`: the device's name, +0.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLDevice_name")]
pub unsafe extern "C-unwind" fn mtldevice_name(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjObjRet>(args) };
    // SAFETY: a live device or 0.
    let device = unsafe { device(p.handle) };
    p.ret = device.map_or(0, |d| ret_passthrough(d.name_raw()));
    STATUS_SUCCESS
}

/// Slot 8, `NSString_getCString`: writes a string's bytes into the guest's buffer.
///
/// `ret` is Foundation's answer: whether the string and its terminator fit. Nothing else of the
/// buffer is touched, and a nil string writes only `ret`.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_NSString_getCString")]
pub unsafe extern "C-unwind" fn nsstring_get_cstring(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallNsstringGetcstring>(args) };
    // SAFETY: a live string or 0.
    let string = unsafe { obj::<RawString>(p.str) };
    let buffer = raw::<c_char>(p.buffer_ptr);
    let max_length = u64_to_usize(p.max_length);
    let encoding = u64_to_usize(p.encoding);
    p.ret = string.map_or(0, |s| {
        // SAFETY: the C's send of the guest's buffer and length: the guest owns `max_length`
        // writable bytes at `buffer`.
        u32::from(unsafe { s.get_cstring(buffer, max_length, encoding) })
    });
    STATUS_SUCCESS
}

/// Slot 10, `NSAutoreleasePool_alloc_init`: a new autorelease pool, +1.
///
/// The result is the `NSAutoreleasePool` object, not a push token: slot 1 drains it, on the
/// thread that made it, over any number of calls.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_NSAutoreleasePool_alloc_init")]
pub unsafe extern "C-unwind" fn nsautoreleasepool_alloc_init(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjRet>(args) };
    // SAFETY: `init` of the object `alloc` just made.
    let pool = unsafe { NSAutoreleasePool::init(NSAutoreleasePool::alloc()) };
    p.ret = ret_owned(Some(pool));
    STATUS_SUCCESS
}

/// Slot 24, `MTLDevice_minimumLinearTextureAlignmentForPixelFormat`.
///
/// The format is the low 32 bits of the argument, without the custom swizzle bits.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLDevice_minimumLinearTextureAlignmentForPixelFormat")]
pub unsafe extern "C-unwind" fn mtldevice_minimum_linear_texture_alignment(
    args: *mut c_void,
) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjUint64Uint64Ret>(args) };
    // SAFETY: a live device or 0.
    let device = unsafe { device(p.handle) };
    let format = pixel_format(u64_to_u32(p.arg));
    p.ret = device.map_or(0, |d| {
        d.minimumLinearTextureAlignmentForPixelFormat(format) as u64
    });
    STATUS_SUCCESS
}

/// Slot 27, `NSString_lengthOfBytesUsingEncoding`.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_NSString_lengthOfBytesUsingEncoding")]
pub unsafe extern "C-unwind" fn nsstring_length_of_bytes(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjUint64Uint64Ret>(args) };
    // SAFETY: a live string or 0.
    let string = unsafe { obj::<NSString>(p.handle) };
    let encoding = u64_to_usize(p.arg);
    p.ret = string.map_or(0, |s| s.lengthOfBytesUsingEncoding(encoding) as u64);
    STATUS_SUCCESS
}

/// Slot 28, `NSObject_description`: the object's description, +0.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_NSObject_description")]
pub unsafe extern "C-unwind" fn nsobject_description(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjObjRet>(args) };
    // SAFETY: a live object or 0.
    let object = unsafe { obj::<RawObject>(p.handle) };
    p.ret = object.map_or(0, |o| ret_passthrough(o.description_raw()));
    STATUS_SUCCESS
}

/// Slot 49, `MTLDevice_supportsFamily`.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLDevice_supportsFamily")]
pub unsafe extern "C-unwind" fn mtldevice_supports_family(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjUint64Uint64Ret>(args) };
    // SAFETY: a live device or 0.
    let device = unsafe { device(p.handle) };
    let family = MTLGPUFamily(u64_to_usize(p.arg).cast_signed());
    p.ret = device.map_or(0, |d| u64::from(d.supportsFamily(family)));
    STATUS_SUCCESS
}

/// Slot 50, `MTLDevice_supportsBCTextureCompression`.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLDevice_supportsBCTextureCompression")]
pub unsafe extern "C-unwind" fn mtldevice_supports_bc_texture_compression(
    args: *mut c_void,
) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjUint64Ret>(args) };
    // SAFETY: a live device or 0.
    let device = unsafe { device(p.handle) };
    p.ret = device.map_or(0, |d| u64::from(d.supportsBCTextureCompression()));
    STATUS_SUCCESS
}

/// Slot 51, `MTLDevice_supportsTextureSampleCount`.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLDevice_supportsTextureSampleCount")]
pub unsafe extern "C-unwind" fn mtldevice_supports_texture_sample_count(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjUint64Uint64Ret>(args) };
    // SAFETY: a live device or 0.
    let device = unsafe { device(p.handle) };
    let count = u64_to_usize(p.arg);
    p.ret = device.map_or(0, |d| u64::from(d.supportsTextureSampleCount(count)));
    STATUS_SUCCESS
}

/// Slot 52, `MTLDevice_hasUnifiedMemory`.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLDevice_hasUnifiedMemory")]
pub unsafe extern "C-unwind" fn mtldevice_has_unified_memory(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjUint64Ret>(args) };
    // SAFETY: a live device or 0.
    let device = unsafe { device(p.handle) };
    p.ret = device.map_or(0, |d| u64::from(d.hasUnifiedMemory()));
    STATUS_SUCCESS
}

/// Slot 60, `NSString_string`: a string from C bytes, +0 (autoreleased).
///
/// Bytes the encoding rejects answer 0, a null pointer raises, as `stringWithCString:encoding:`
/// does.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_NSString_string")]
pub unsafe extern "C-unwind" fn nsstring_string(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallNsstringString>(args) };
    let bytes = raw::<c_char>(p.buffer_ptr.ptr).cast_const();
    let encoding = u64_to_usize(p.encoding);
    // SAFETY: the C's send of the guest's bytes: a NUL-terminated string, or null, which raises.
    p.ret = ret_passthrough(unsafe { RawString::with_cstring_raw(bytes, encoding) });
    STATUS_SUCCESS
}

/// Slot 61, `NSString_alloc_init`: a string from C bytes, +1.
///
/// Bytes the encoding rejects answer 0.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_NSString_alloc_init")]
pub unsafe extern "C-unwind" fn nsstring_alloc_init(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallNsstringString>(args) };
    let bytes = raw::<c_char>(p.buffer_ptr.ptr).cast_const();
    let encoding = u64_to_usize(p.encoding);
    // SAFETY: the C's send of the guest's bytes: a NUL-terminated string.
    let string = unsafe { RawString::init_with_cstring(RawString::alloc(), bytes, encoding) };
    p.ret = ret_owned(string);
    STATUS_SUCCESS
}

/// Slot 86, `MTLCommandEncoder_setLabel`: sets the label of any object that answers it.
///
/// A 0 label sends nil, which removes the label; a 0 handle sends nothing.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLCommandEncoder_setLabel")]
pub unsafe extern "C-unwind" fn mtlcommandencoder_set_label(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjObjNoret>(args) };
    // SAFETY: a live object that answers `setLabel:`, or 0.
    let encoder = unsafe { obj::<ProtocolObject<dyn MTLCommandEncoder>>(p.handle) };
    // SAFETY: a live string or 0.
    let label = unsafe { obj::<NSString>(p.arg) };
    if let Some(encoder) = encoder {
        encoder.setLabel(label);
    }
    STATUS_SUCCESS
}

/// Slot 87, `MTLDevice_setShouldMaximizeConcurrentCompilation`.
///
/// Any non-zero argument is YES.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLDevice_setShouldMaximizeConcurrentCompilation")]
pub unsafe extern "C-unwind" fn mtldevice_set_should_maximize_concurrent_compilation(
    args: *mut c_void,
) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjUint64Noret>(args) };
    // SAFETY: a live device or 0.
    let device = unsafe { device(p.handle) };
    if let Some(device) = device {
        device.setShouldMaximizeConcurrentCompilation(p.arg != 0);
    }
    STATUS_SUCCESS
}

/// Slot 111, `WMTGetOSVersion`: the host's operating system version.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_WMTGetOSVersion")]
pub unsafe extern "C-unwind" fn wmt_get_os_version(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGetOsVersion>(args) };
    let version = NSProcessInfo::processInfo().operatingSystemVersion();
    p.ret_major = version.majorVersion.cast_unsigned() as u64;
    p.ret_minor = version.minorVersion.cast_unsigned() as u64;
    p.ret_patch = version.patchVersion.cast_unsigned() as u64;
    STATUS_SUCCESS
}

/// Slot 114, `DispatchData_alloc_init`: dispatch data holding a copy of the guest's bytes, +1.
///
/// The handle is a byte pointer and the argument its length. No bytes (a null pointer or length
/// 0) answer libdispatch's empty data.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_DispatchData_alloc_init")]
pub unsafe extern "C-unwind" fn dispatchdata_alloc_init(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjUint64ObjRet>(args) };
    let size = u64_to_usize(p.arg);
    let data = NonNull::new(raw::<c_void>(p.handle)).map_or_else(
        || core::ptr::from_ref(DispatchData::empty()).cast_mut(),
        |bytes| {
            // SAFETY: the guest's `size` readable bytes, copied (a null destructor is the default).
            let data = unsafe { DispatchData::new(bytes, size, None, core::ptr::null_mut()) };
            DispatchRetained::into_raw(data).as_ptr()
        },
    );
    // The +1 reference leaves with the address, as `dispatch_data_create`'s result did.
    p.ret = ret_passthrough(data.cast());
    STATUS_SUCCESS
}

/// Slot 125, `MTLDevice_registryID`.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLDevice_registryID")]
pub unsafe extern "C-unwind" fn mtldevice_registry_id(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjUint64Ret>(args) };
    // SAFETY: a live device or 0.
    let device = unsafe { device(p.handle) };
    p.ret = device.map_or(0, MTLDevice::registryID);
    STATUS_SUCCESS
}

/// Slot 148, `MTLDevice_sampleTimestamps`: the device's CPU and GPU timestamps.
///
/// A nil device answers 0 and 0.
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLDevice_sampleTimestamps")]
pub unsafe extern "C-unwind" fn mtldevice_sample_timestamps(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtldeviceSampletimestamps>(args) };
    // SAFETY: a live device or 0.
    let device = unsafe { device(p.device) };
    let (mut cpu, mut gpu) = (0_u64, 0_u64);
    if let Some(device) = device {
        // SAFETY: two writable timestamps.
        unsafe {
            device.sampleTimestamps_gpuTimestamp(NonNull::from(&mut cpu), NonNull::from(&mut gpu));
        }
    }
    p.cpu = cpu;
    p.gpu = gpu;
    STATUS_SUCCESS
}

/// Slot 149, `DispatchData_copyBytes`: copies dispatch data out, at most `capacity` bytes.
///
/// `ret_size` is the data's full size. Regions are copied in order, one that starts at or past
/// `capacity` stops the walk and one that crosses it is cut. A nil data, or a null buffer,
/// copies nothing (the C faulted).
///
/// # Safety
///
/// `args` is the params block the caller built for this slot.
#[unsafe(export_name = "_DispatchData_copyBytes")]
pub unsafe extern "C-unwind" fn dispatchdata_copy_bytes(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallDispatchdataCopybytes>(args) };
    // SAFETY: live dispatch data or 0.
    let Some(data) = (unsafe { obj::<DispatchData>(p.data) }) else {
        return STATUS_SUCCESS;
    };
    let out = raw::<u8>(p.buffer.ptr);
    let capacity = if out.is_null() {
        0
    } else {
        u64_to_usize(p.capacity)
    };
    // The block answers a `u8`, as `dispatch2::DispatchData::to_vec` does: `bool` is not `Encode`.
    let copy = StackBlock::new(
        move |_: NonNull<DispatchData>, offset: usize, src: NonNull<c_void>, length: usize| {
            if offset >= capacity {
                return 0_u8;
            }
            let count = length.min(capacity.saturating_sub(offset));
            // SAFETY: `src` holds `length` bytes, at least `count`; the guest's buffer holds
            // `capacity` bytes, and `offset + count <= capacity`.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    src.as_ptr().cast::<u8>(),
                    out.wrapping_add(offset),
                    count,
                );
            }
            1_u8
        },
    );
    // A `u8` and a `bool` return alike in a register.
    let applier: dispatch_data_applier_t = core::ptr::from_ref(&*copy).cast_mut().cast();
    // SAFETY: a block of the applier's shape, which dispatch runs before returning.
    unsafe { data.apply(applier) };
    p.ret_size = data.size() as u64;
    STATUS_SUCCESS
}

#[cfg(test)]
mod tests;
