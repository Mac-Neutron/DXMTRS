//! Metal's GPU capture: the shared manager, and starting and stopping a capture.
//!
//! The descriptor is the binding's `MTLCaptureDescriptor`. The manager and the output URL are local
//! re-declarations, because the C's sends differ from the bindings' in their memory or their
//! arguments: the shared manager's +0 result goes out raw, the start call passes `error:nil` and
//! never reads a result, both manager sends take the handle as a raw receiver (a nil manager is
//! sent to, as the C sends it), and `initFileURLWithPath:` takes the nil that an output string
//! which is not UTF-8 converts to (it raises, as the C's does).

use core::ffi::{c_char, c_void};
use core::ptr;
use core::ptr::NonNull;

use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, NSObject};
use objc2::{AnyThread, extern_class, extern_methods};
use objc2_foundation::{NSError, NSString, NSURL, NSUTF8StringEncoding};
use objc2_metal::{MTLCaptureDescriptor, MTLCaptureDestination};
use winemetal_wire::{
    UnixcallGenericObjNoret, UnixcallGenericObjRet, UnixcallMtlcapturemanagerStartcapture,
    WMTCaptureInfo,
};

use crate::ffi::{AbortOnPanic, STATUS_SUCCESS, mem_cstr, mem_ref, params};
use crate::float_cast::u64_to_usize;
use crate::handle;

extern_class!(
    /// `MTLCaptureManager`, re-declared for the sends the C makes raw or to nil.
    #[unsafe(super(NSObject))]
    #[name = "MTLCaptureManager"]
    struct CaptureManager;
);

impl CaptureManager {
    extern_methods!(
        /// The shared manager, `+sharedCaptureManager`.
        ///
        /// +0: written straight from the send, so the raw pointer goes out as given.
        ///
        /// # Safety
        ///
        /// The class is loaded (Metal is linked).
        #[unsafe(method(sharedCaptureManager))]
        #[unsafe(method_family = none)]
        unsafe fn shared_capture_manager() -> *mut AnyObject;

        /// `-startCaptureWithDescriptor:error:`, with `error` nil.
        ///
        /// The C sends it with `error:nil` and never reads the result, so none is returned.
        ///
        /// # Safety
        ///
        /// `this` is null (a nil manager, which takes the send and does nothing) or a live
        /// `MTLCaptureManager`; `error` is null or a valid out-pointer.
        #[unsafe(method(startCaptureWithDescriptor:error:))]
        #[unsafe(method_family = none)]
        unsafe fn start_capture(
            this: *mut Self,
            descriptor: &MTLCaptureDescriptor,
            error: *mut *mut NSError,
        ) -> bool;

        /// `-stopCapture`.
        ///
        /// # Safety
        ///
        /// `this` is null (a nil manager, which takes the send and does nothing) or a live
        /// `MTLCaptureManager`.
        #[unsafe(method(stopCapture))]
        #[unsafe(method_family = none)]
        unsafe fn stop_capture(this: *mut Self);
    );
}

extern_class!(
    /// `NSURL`, re-declared for `initFileURLWithPath:` with a nil path.
    ///
    /// The C passes the string conversion's nil on, and Foundation raises on it.
    #[unsafe(super(NSObject))]
    #[name = "NSURL"]
    struct FileUrl;
);

impl FileUrl {
    extern_methods!(
        /// `-initFileURLWithPath:`, with `path` nil allowed.
        #[unsafe(method(initFileURLWithPath:))]
        #[unsafe(method_family = init)]
        fn init_file_url_with_path(
            this: Allocated<Self>,
            path: Option<&NSString>,
        ) -> Option<Retained<Self>>;
    );
}

/// A manager handle as the raw receiver of its sends: null for 0, the nil the C sends to.
const fn receiver(h: u64) -> *mut CaptureManager {
    ptr::with_exposed_provenance_mut(u64_to_usize(h))
}

/// `_MTLCaptureManager_sharedCaptureManager`.
///
/// # Safety
///
/// `obj` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLCaptureManager_sharedCaptureManager")]
pub unsafe extern "C-unwind" fn mtl_capture_manager_shared_capture_manager(
    obj: *mut c_void,
) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract: `obj` is this slot's params block.
    let params = unsafe { params::<UnixcallGenericObjRet>(obj) };
    // SAFETY: a class method with no arguments, on a class Metal provides.
    let manager = unsafe { CaptureManager::shared_capture_manager() };
    params.ret = handle::ret_passthrough(manager);
    STATUS_SUCCESS
}

/// `_MTLCaptureManager_startCapture`. The C never writes the params' `ret`, and neither does this.
///
/// # Safety
///
/// `obj` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLCaptureManager_startCapture")]
pub unsafe extern "C-unwind" fn mtl_capture_manager_start_capture(obj: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract: `obj` is this slot's params block.
    let params = unsafe { params::<UnixcallMtlcapturemanagerStartcapture>(obj) };
    // SAFETY: the caller's contract. The C reads `info` straight through the pointer and faults on
    // null (not cased); here a null `info` writes nothing and starts nothing.
    let Some(info) = (unsafe { mem_ref::<WMTCaptureInfo>(params.info) }) else {
        return STATUS_SUCCESS;
    };
    let desc = MTLCaptureDescriptor::init(MTLCaptureDescriptor::alloc());
    desc.setDestination(MTLCaptureDestination(isize::from(info.destination)));
    // SAFETY: the caller's contract: the capture object is a live handle, or 0 for nil.
    let capture_object = unsafe { handle::obj::<AnyObject>(info.capture_object) };
    // SAFETY: the object is live or nil, as above.
    unsafe { desc.setCaptureObject(capture_object) };
    // The output string, converted as the C converts it. A null string faults in the C's
    // conversion (not cased): here it gives no string, so the descriptor gets no URL. A string
    // that is not UTF-8 converts to nil, and the URL init then gets that nil.
    // SAFETY: the caller's contract: the output string is NUL-terminated.
    let path_str = unsafe { mem_cstr(info.output_url) }.map(|path| {
        // SAFETY: the NUL-terminated string the caller gave, which the conversion only reads.
        unsafe {
            NSString::initWithCString_encoding(
                NSString::alloc(),
                NonNull::from(path).cast::<c_char>(),
                NSUTF8StringEncoding,
            )
        }
    });
    let url = path_str
        .as_ref()
        .and_then(|path| FileUrl::init_file_url_with_path(FileUrl::alloc(), path.as_deref()));
    let url_ref = url.as_deref().map(|url| {
        // SAFETY: the same NSURL object, under the binding's declaration of its class.
        unsafe { &*ptr::from_ref(url).cast::<NSURL>() }
    });
    desc.setOutputURL(url_ref);
    // SAFETY: the manager is a live MTLCaptureManager or 0 (nil), as the caller's contract; the
    // error out-pointer is nil, as the C passes it.
    unsafe {
        CaptureManager::start_capture(receiver(params.capture_manager), &desc, ptr::null_mut())
    };
    STATUS_SUCCESS
}

/// `_MTLCaptureManager_stopCapture`.
///
/// # Safety
///
/// `obj` is the params block the caller built for this slot.
#[unsafe(export_name = "_MTLCaptureManager_stopCapture")]
pub unsafe extern "C-unwind" fn mtl_capture_manager_stop_capture(obj: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract: `obj` is this slot's params block.
    let params = unsafe { params::<UnixcallGenericObjNoret>(obj) };
    // SAFETY: the manager is a live MTLCaptureManager or 0 (nil), as the caller's contract.
    unsafe { CaptureManager::stop_capture(receiver(params.handle)) };
    STATUS_SUCCESS
}
