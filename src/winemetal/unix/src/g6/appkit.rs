//! `NSScreen`, `NSView` and `NSWindow` getters, re-declared readable on any thread.
//!
//! Slots 96, 97 and 101 read them on the calling thread, as the C did, with no hop to the main
//! thread (CONVENTIONS, winemetal's display reads). This states what the C does, not that the
//! getters are thread-safe.

use objc2::{AnyThread, extern_class, extern_methods, rc::Retained, runtime::NSObject};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString};

extern_class!(
    /// `NSScreen`.
    #[unsafe(super(NSObject))]
    #[thread_kind = AnyThread]
    #[name = "NSScreen"]
    pub struct Screen;
);

impl Screen {
    extern_methods!(
        /// `+screens`.
        #[unsafe(method(screens))]
        #[unsafe(method_family = none)]
        pub fn screens() -> Retained<NSArray<Self>>;

        /// `-deviceDescription`, its values typed as numbers.
        ///
        /// winemetal reads its `NSScreenNumber` only.
        #[unsafe(method(deviceDescription))]
        #[unsafe(method_family = none)]
        pub fn device_description(&self) -> Retained<NSDictionary<NSString, NSNumber>>;

        /// `-maximumExtendedDynamicRangeColorComponentValue`.
        #[unsafe(method(maximumExtendedDynamicRangeColorComponentValue))]
        #[unsafe(method_family = none)]
        pub fn max_edr(&self) -> CGFloat;

        /// `-maximumPotentialExtendedDynamicRangeColorComponentValue`.
        #[unsafe(method(maximumPotentialExtendedDynamicRangeColorComponentValue))]
        #[unsafe(method_family = none)]
        pub fn max_potential_edr(&self) -> CGFloat;

        /// `-maximumReferenceExtendedDynamicRangeColorComponentValue`.
        #[unsafe(method(maximumReferenceExtendedDynamicRangeColorComponentValue))]
        #[unsafe(method_family = none)]
        pub fn max_reference_edr(&self) -> CGFloat;
    );
}

extern_class!(
    /// `NSView`.
    #[unsafe(super(NSObject))]
    #[thread_kind = AnyThread]
    #[name = "NSView"]
    pub struct View;
);

impl View {
    extern_methods!(
        /// `-window`.
        #[unsafe(method(window))]
        #[unsafe(method_family = none)]
        pub fn window(&self) -> Option<Retained<Window>>;
    );
}

extern_class!(
    /// `NSWindow`.
    #[unsafe(super(NSObject))]
    #[thread_kind = AnyThread]
    #[name = "NSWindow"]
    pub struct Window;
);

impl Window {
    extern_methods!(
        /// `-screen`.
        #[unsafe(method(screen))]
        #[unsafe(method_family = none)]
        pub fn screen(&self) -> Option<Retained<Screen>>;
    );
}
