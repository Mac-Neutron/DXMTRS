use objc2::{
    extern_class, extern_methods,
    rc::{Weak, autoreleasepool},
    runtime::{NSObject, NSObjectProtocol},
};
use objc2_foundation::{NSInteger, NSString};

use super::*;

extern_class!(
    /// `NSError`, re-declared for a +0 result returned as a raw pointer.
    #[unsafe(super(NSObject))]
    #[name = "NSError"]
    struct RawError;
);

impl RawError {
    extern_methods!(
        /// `+errorWithDomain:code:userInfo:`, autoreleased, as a raw pointer.
        #[unsafe(method(errorWithDomain:code:userInfo:))]
        #[unsafe(method_family = none)]
        fn error(
            domain: &NSString,
            code: NSInteger,
            user_info: Option<&AnyObject>,
        ) -> *mut NSObject;
    );
}

#[test]
fn missing_class_is_none() {
    assert!(lookup_class(c"WMProbeNoSuchClass").is_none());
}

#[test]
fn passthrough_keeps_counts() {
    let weak = autoreleasepool(|_| {
        let p = RawError::error(&NSString::from_str("winemetal"), 1, None);
        // SAFETY: an autoreleased object, alive until the pool drains.
        let error = unsafe { &*p };
        assert_eq!(error.retainCount(), 1);
        assert_eq!(ret_passthrough(p.cast()), p.expose_provenance() as u64);
        assert_eq!(error.retainCount(), 1);
        Weak::from(error)
    });
    // Only the pool held it: no retain was taken that the pass-through leaves behind.
    assert!(weak.load().is_none());
}
