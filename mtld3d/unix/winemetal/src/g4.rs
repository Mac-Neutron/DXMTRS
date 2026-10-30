//! Libraries, functions, pipeline states and binary archives.
//!
//! Slots 25 and 26 (a library from metallib data, a function by name), 144 and 157 (a library
//! from source), 98 (a function with constant values), 29, 34, 35, 131 and 156 (compute, render,
//! mesh, tile and vertex-described pipelines) and 112 and 113 (a binary archive, serialised).
//! Every `new*` result is +1 and so is every error, which the C retained (`ret_error`).
//!
//! Where the C sends nil it still sends: an argument that is nil is sent as nil through a local
//! re-declaration taking an `Option`, and a nil receiver answers the zeros of a message to nil.
//! Where the C faults on a null pointer (a null params block member it dereferences) the handler
//! sends nothing and writes nothing; slot 98's constant without data is the one exception, which
//! reads as zeros where Metal reads it.

use core::ffi::{c_char, c_void};
use core::ptr::{self, NonNull};

use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, NSObject, ProtocolObject};
use objc2::{AnyThread, Message, extern_class, extern_methods};
use objc2_foundation::{NSArray, NSError, NSString, NSUInteger, NSURL};
use objc2_metal::{
    MTLBinaryArchive, MTLBinaryArchiveDescriptor, MTLBlendFactor, MTLBlendOperation,
    MTLColorWriteMask, MTLCompileOptions, MTLComputePipelineDescriptor, MTLComputePipelineState,
    MTLDataType, MTLDevice, MTLFunction, MTLFunctionConstantValues, MTLLanguageVersion, MTLLibrary,
    MTLMathMode, MTLMeshRenderPipelineDescriptor, MTLMutability, MTLPipelineBufferDescriptorArray,
    MTLPipelineOption, MTLPrimitiveTopologyClass, MTLRenderPipelineColorAttachmentDescriptorArray,
    MTLRenderPipelineDescriptor, MTLRenderPipelineState, MTLTessellationFactorStepFunction,
    MTLTessellationPartitionMode, MTLTileRenderPipelineDescriptor, MTLVertexAttributeDescriptor,
    MTLVertexBufferLayoutDescriptor, MTLVertexDescriptor, MTLVertexFormat, MTLVertexStepFunction,
    MTLWinding,
};
use winemetal_wire::{
    MemPtr, UnixcallGenericObjUint64ObjRet, UnixcallMtlbinaryarchiveSerialize,
    UnixcallMtldeviceNewbinaryarchive, UnixcallMtldeviceNewcomputepso, UnixcallMtldeviceNewlibrary,
    UnixcallMtldeviceNewlibrarywithsource, UnixcallMtldeviceNewlibrarywithsourceOptions,
    UnixcallMtldeviceNewmeshrenderpso, UnixcallMtldeviceNewrenderpso,
    UnixcallMtldeviceNewrenderpsoVertex, UnixcallMtllibraryNewfunctionWithConstants,
    WMTColorAttachmentBlendInfo, WMTCompileOptions, WMTComputePipelineInfo, WMTFunctionConstant,
    WMTMeshRenderPipelineInfo, WMTRenderPipelineInfo, WMTTileRenderPipelineInfo,
    WMTVertexDescriptorInfo,
};

use crate::ffi::{AbortOnPanic, STATUS_SUCCESS, mem_cstr, mem_ref, mem_slice, params};
use crate::float_cast::u64_to_usize;
use crate::format::pixel_format;
use crate::handle::{obj, ret_owned};

/// `NSUTF8StringEncoding`.
const UTF8: NSUInteger = 4;

/// `NSASCIIStringEncoding`.
const ASCII: NSUInteger = 1;

/// How many buffer slots a pipeline descriptor's immutability masks cover.
const BUFFER_SLOTS: usize = 31;

/// How many zeros slot 98 gives a constant without data.
///
/// Twice the 32 bytes of the largest value Metal reads (macOS 27: types 0-159 tried with a guard
/// page after the value, and all 16-bit types with a null pointer, which faults for 48 of them).
const ZERO_VALUE_BYTES: usize = 64;

extern_class!(
    /// `NSURL`, re-declared to send `-initFileURLWithPath:` a nil path, which raises.
    #[unsafe(super(NSObject))]
    #[name = "NSURL"]
    struct RawUrl;
);

impl RawUrl {
    extern_methods!(
        /// `-initFileURLWithPath:`, with the path the C's string decode gave: nil raises.
        #[unsafe(method(initFileURLWithPath:))]
        #[unsafe(method_family = init)]
        fn init_file_url_with_path(this: Allocated<Self>, path: Option<&NSString>) -> Retained<Self>;
    );
}

extern_class!(
    /// `MTLRenderPipelineDescriptor`, re-declared for its private logic-operation setters.
    #[unsafe(super(NSObject))]
    #[name = "MTLRenderPipelineDescriptor"]
    struct PrivateRenderDescriptor;
);

impl PrivateRenderDescriptor {
    extern_methods!(
        /// `-setLogicOperationEnabled:`.
        #[unsafe(method(setLogicOperationEnabled:))]
        #[unsafe(method_family = none)]
        fn set_logic_operation_enabled(&self, enabled: bool);

        /// `-setLogicOperation:`.
        #[unsafe(method(setLogicOperation:))]
        #[unsafe(method_family = none)]
        fn set_logic_operation(&self, operation: NSUInteger);
    );
}

extern_class!(
    /// `MTLMeshRenderPipelineDescriptor`, re-declared for its private logic-operation setters.
    #[unsafe(super(NSObject))]
    #[name = "MTLMeshRenderPipelineDescriptor"]
    struct PrivateMeshDescriptor;
);

impl PrivateMeshDescriptor {
    extern_methods!(
        /// `-setLogicOperationEnabled:`.
        #[unsafe(method(setLogicOperationEnabled:))]
        #[unsafe(method_family = none)]
        fn set_logic_operation_enabled(&self, enabled: bool);

        /// `-setLogicOperation:`.
        #[unsafe(method(setLogicOperation:))]
        #[unsafe(method_family = none)]
        fn set_logic_operation(&self, operation: NSUInteger);
    );
}

extern_class!(
    /// `MTLTileRenderPipelineDescriptor`, re-declared to send `-setTileFunction:` a nil function.
    #[unsafe(super(NSObject))]
    #[name = "MTLTileRenderPipelineDescriptor"]
    struct RawTileDescriptor;
);

impl RawTileDescriptor {
    extern_methods!(
        /// `-setTileFunction:`, with the function the guest's handle names: nil for 0.
        #[unsafe(method(setTileFunction:))]
        #[unsafe(method_family = none)]
        fn set_tile_function(&self, function: Option<&ProtocolObject<dyn MTLFunction>>);
    );
}

/// The device's sends with a nil-able argument and the C's raw `NSError **`.
trait RawDevice {
    /// `-newLibraryWithData:error:`, the data sent as it is (nil included).
    fn new_library_with_data(
        &self,
        data: Option<&AnyObject>,
        error: &mut *mut NSError,
    ) -> Option<Retained<ProtocolObject<dyn MTLLibrary>>>;

    /// `-newLibraryWithSource:options:error:`, the source sent as it is (nil included).
    fn new_library_with_source(
        &self,
        source: Option<&NSString>,
        options: &MTLCompileOptions,
        error: &mut *mut NSError,
    ) -> Option<Retained<ProtocolObject<dyn MTLLibrary>>>;

    /// `-newComputePipelineStateWithDescriptor:options:reflection:error:`, no reflection.
    fn new_compute_pipeline_state(
        &self,
        descriptor: &MTLComputePipelineDescriptor,
        options: MTLPipelineOption,
        reflection: *mut *mut AnyObject,
        error: &mut *mut NSError,
    ) -> Option<Retained<ProtocolObject<dyn MTLComputePipelineState>>>;

    /// `-newRenderPipelineStateWithDescriptor:options:reflection:error:`, no reflection.
    fn new_render_pipeline_state(
        &self,
        descriptor: &MTLRenderPipelineDescriptor,
        options: MTLPipelineOption,
        reflection: *mut *mut AnyObject,
        error: &mut *mut NSError,
    ) -> Option<Retained<ProtocolObject<dyn MTLRenderPipelineState>>>;

    /// `-newRenderPipelineStateWithMeshDescriptor:options:reflection:error:`, no reflection.
    fn new_mesh_render_pipeline_state(
        &self,
        descriptor: &MTLMeshRenderPipelineDescriptor,
        options: MTLPipelineOption,
        reflection: *mut *mut AnyObject,
        error: &mut *mut NSError,
    ) -> Option<Retained<ProtocolObject<dyn MTLRenderPipelineState>>>;

    /// `-newRenderPipelineStateWithTileDescriptor:options:reflection:error:`, no reflection.
    fn new_tile_render_pipeline_state(
        &self,
        descriptor: &MTLTileRenderPipelineDescriptor,
        options: MTLPipelineOption,
        reflection: *mut *mut AnyObject,
        error: &mut *mut NSError,
    ) -> Option<Retained<ProtocolObject<dyn MTLRenderPipelineState>>>;

    /// `-newBinaryArchiveWithDescriptor:error:`.
    fn new_binary_archive(
        &self,
        descriptor: &MTLBinaryArchiveDescriptor,
        error: &mut *mut NSError,
    ) -> Option<Retained<ProtocolObject<dyn MTLBinaryArchive>>>;
}

impl RawDevice for ProtocolObject<dyn MTLDevice> {
    extern_methods!(
        #[unsafe(method(newLibraryWithData:error:))]
        #[unsafe(method_family = new)]
        fn new_library_with_data(
            &self,
            data: Option<&AnyObject>,
            error: &mut *mut NSError,
        ) -> Option<Retained<ProtocolObject<dyn MTLLibrary>>>;

        #[unsafe(method(newLibraryWithSource:options:error:))]
        #[unsafe(method_family = new)]
        fn new_library_with_source(
            &self,
            source: Option<&NSString>,
            options: &MTLCompileOptions,
            error: &mut *mut NSError,
        ) -> Option<Retained<ProtocolObject<dyn MTLLibrary>>>;

        #[unsafe(method(newComputePipelineStateWithDescriptor:options:reflection:error:))]
        #[unsafe(method_family = new)]
        fn new_compute_pipeline_state(
            &self,
            descriptor: &MTLComputePipelineDescriptor,
            options: MTLPipelineOption,
            reflection: *mut *mut AnyObject,
            error: &mut *mut NSError,
        ) -> Option<Retained<ProtocolObject<dyn MTLComputePipelineState>>>;

        #[unsafe(method(newRenderPipelineStateWithDescriptor:options:reflection:error:))]
        #[unsafe(method_family = new)]
        fn new_render_pipeline_state(
            &self,
            descriptor: &MTLRenderPipelineDescriptor,
            options: MTLPipelineOption,
            reflection: *mut *mut AnyObject,
            error: &mut *mut NSError,
        ) -> Option<Retained<ProtocolObject<dyn MTLRenderPipelineState>>>;

        #[unsafe(method(newRenderPipelineStateWithMeshDescriptor:options:reflection:error:))]
        #[unsafe(method_family = new)]
        fn new_mesh_render_pipeline_state(
            &self,
            descriptor: &MTLMeshRenderPipelineDescriptor,
            options: MTLPipelineOption,
            reflection: *mut *mut AnyObject,
            error: &mut *mut NSError,
        ) -> Option<Retained<ProtocolObject<dyn MTLRenderPipelineState>>>;

        #[unsafe(method(newRenderPipelineStateWithTileDescriptor:options:reflection:error:))]
        #[unsafe(method_family = new)]
        fn new_tile_render_pipeline_state(
            &self,
            descriptor: &MTLTileRenderPipelineDescriptor,
            options: MTLPipelineOption,
            reflection: *mut *mut AnyObject,
            error: &mut *mut NSError,
        ) -> Option<Retained<ProtocolObject<dyn MTLRenderPipelineState>>>;

        #[unsafe(method(newBinaryArchiveWithDescriptor:error:))]
        #[unsafe(method_family = new)]
        fn new_binary_archive(
            &self,
            descriptor: &MTLBinaryArchiveDescriptor,
            error: &mut *mut NSError,
        ) -> Option<Retained<ProtocolObject<dyn MTLBinaryArchive>>>;
    );
}

/// The library's sends with a nil-able name and the C's raw `NSError **`.
trait RawLibrary {
    /// `-newFunctionWithName:`, the name sent as it is (nil included).
    fn new_function(&self, name: Option<&NSString>) -> Option<Retained<ProtocolObject<dyn MTLFunction>>>;

    /// `-newFunctionWithName:constantValues:error:`, the name sent as it is (nil included).
    fn new_function_with_constants(
        &self,
        name: Option<&NSString>,
        values: &MTLFunctionConstantValues,
        error: &mut *mut NSError,
    ) -> Option<Retained<ProtocolObject<dyn MTLFunction>>>;
}

impl RawLibrary for ProtocolObject<dyn MTLLibrary> {
    extern_methods!(
        #[unsafe(method(newFunctionWithName:))]
        #[unsafe(method_family = new)]
        fn new_function(&self, name: Option<&NSString>) -> Option<Retained<ProtocolObject<dyn MTLFunction>>>;

        #[unsafe(method(newFunctionWithName:constantValues:error:))]
        #[unsafe(method_family = new)]
        fn new_function_with_constants(
            &self,
            name: Option<&NSString>,
            values: &MTLFunctionConstantValues,
            error: &mut *mut NSError,
        ) -> Option<Retained<ProtocolObject<dyn MTLFunction>>>;
    );
}

/// The archive's sends with the C's raw `NSError **`, whose `BOOL` the C ignored.
trait RawArchive {
    /// `-addComputePipelineFunctionsWithDescriptor:error:`.
    fn add_compute(&self, descriptor: &MTLComputePipelineDescriptor, error: &mut *mut NSError) -> bool;

    /// `-addRenderPipelineFunctionsWithDescriptor:error:`.
    fn add_render(&self, descriptor: &MTLRenderPipelineDescriptor, error: &mut *mut NSError) -> bool;

    /// `-addMeshRenderPipelineFunctionsWithDescriptor:error:`.
    fn add_mesh(&self, descriptor: &MTLMeshRenderPipelineDescriptor, error: &mut *mut NSError) -> bool;

    /// `-addTileRenderPipelineFunctionsWithDescriptor:error:`.
    fn add_tile(&self, descriptor: &MTLTileRenderPipelineDescriptor, error: &mut *mut NSError) -> bool;

    /// `-serializeToURL:error:`.
    fn serialize_to_url(&self, url: &NSURL, error: &mut *mut NSError) -> bool;
}

impl RawArchive for ProtocolObject<dyn MTLBinaryArchive> {
    extern_methods!(
        #[unsafe(method(addComputePipelineFunctionsWithDescriptor:error:))]
        #[unsafe(method_family = none)]
        fn add_compute(&self, descriptor: &MTLComputePipelineDescriptor, error: &mut *mut NSError) -> bool;

        #[unsafe(method(addRenderPipelineFunctionsWithDescriptor:error:))]
        #[unsafe(method_family = none)]
        fn add_render(&self, descriptor: &MTLRenderPipelineDescriptor, error: &mut *mut NSError) -> bool;

        #[unsafe(method(addMeshRenderPipelineFunctionsWithDescriptor:error:))]
        #[unsafe(method_family = none)]
        fn add_mesh(&self, descriptor: &MTLMeshRenderPipelineDescriptor, error: &mut *mut NSError) -> bool;

        #[unsafe(method(addTileRenderPipelineFunctionsWithDescriptor:error:))]
        #[unsafe(method_family = none)]
        fn add_tile(&self, descriptor: &MTLTileRenderPipelineDescriptor, error: &mut *mut NSError) -> bool;

        #[unsafe(method(serializeToURL:error:))]
        #[unsafe(method_family = none)]
        fn serialize_to_url(&self, url: &NSURL, error: &mut *mut NSError) -> bool;
    );
}

/// The device a handle names, `None` for 0.
fn device<'a>(h: u64) -> Option<&'a ProtocolObject<dyn MTLDevice>> {
    // SAFETY: a device handle the caller holds for the call; any other class raises in the send.
    unsafe { obj(h) }
}

/// The library a handle names, `None` for 0.
fn library<'a>(h: u64) -> Option<&'a ProtocolObject<dyn MTLLibrary>> {
    // SAFETY: a library handle the caller holds for the call; any other class raises in the send.
    unsafe { obj(h) }
}

/// The function a handle names, `None` for 0.
fn function<'a>(h: u64) -> Option<&'a ProtocolObject<dyn MTLFunction>> {
    // SAFETY: a function handle the caller holds for the call.
    unsafe { obj(h) }
}

/// The binary archive a handle names, `None` for 0.
fn archive<'a>(h: u64) -> Option<&'a ProtocolObject<dyn MTLBinaryArchive>> {
    // SAFETY: an archive handle the caller holds for the call; any other class raises in the send.
    unsafe { obj(h) }
}

/// The object a handle names as it is, `None` for 0 (metallib data, sent to Metal untyped).
const fn any_object<'a>(h: u64) -> Option<&'a AnyObject> {
    // SAFETY: a handle the caller holds for the call.
    unsafe { obj(h) }
}

/// A BOOL the C took from a `uint8_t`: any non-zero byte is true.
const fn flag(v: u8) -> bool {
    v != 0
}

/// The pipeline option a `fail_on_binary_archive_miss` byte asks for.
const fn pipeline_option(fail_on_miss: u8) -> MTLPipelineOption {
    if flag(fail_on_miss) {
        MTLPipelineOption::FailOnBinaryArchiveMiss
    } else {
        MTLPipelineOption::None
    }
}

/// The buffer slots an immutability mask marks: bits 0 to 30 (bit 31 is not a slot).
fn immutable_slots(mask: u32) -> impl Iterator<Item = usize> {
    (0..BUFFER_SLOTS).filter(move |&i| (mask >> i) & 1 != 0)
}

/// Marks the buffer slots of `mask` immutable.
fn set_immutable(buffers: &MTLPipelineBufferDescriptorArray, mask: u32) {
    for i in immutable_slots(mask) {
        // SAFETY: `i` is below 31, the count of buffer slots a pipeline descriptor has.
        let buffer = unsafe { buffers.objectAtIndexedSubscript(i) };
        buffer.setMutability(MTLMutability::Immutable);
    }
}

/// Sets the eight color attachments' format, blending, write mask, operations and factors.
fn set_color_attachments(
    attachments: &MTLRenderPipelineColorAttachmentDescriptorArray,
    colors: &[WMTColorAttachmentBlendInfo; 8],
) {
    for (i, c) in colors.iter().enumerate() {
        // SAFETY: `i` is below 8, the count of color attachments.
        let a = unsafe { attachments.objectAtIndexedSubscript(i) };
        a.setPixelFormat(pixel_format(c.pixel_format));
        a.setBlendingEnabled(flag(c.blending_enabled));
        a.setWriteMask(MTLColorWriteMask(usize::from(c.write_mask)));
        a.setAlphaBlendOperation(MTLBlendOperation(usize::from(c.alpha_blend_operation)));
        a.setRgbBlendOperation(MTLBlendOperation(usize::from(c.rgb_blend_operation)));
        a.setSourceRGBBlendFactor(MTLBlendFactor(usize::from(c.src_rgb_blend_factor)));
        a.setSourceAlphaBlendFactor(MTLBlendFactor(usize::from(c.src_alpha_blend_factor)));
        a.setDestinationRGBBlendFactor(MTLBlendFactor(usize::from(c.dst_rgb_blend_factor)));
        a.setDestinationAlphaBlendFactor(MTLBlendFactor(usize::from(c.dst_alpha_blend_factor)));
    }
}

/// The array of binary archives to look pipelines up in, `None` when the C sets none.
///
/// `[NSArray arrayWithObjects:count:]` over the guest's list as it is: a nil entry raises.
fn lookup_archives(
    count: u8,
    list: MemPtr,
) -> Option<Retained<NSArray<ProtocolObject<dyn MTLBinaryArchive>>>> {
    if count == 0 {
        return None;
    }
    // SAFETY: the guest's list holds `count` handles.
    let handles = unsafe { mem_slice::<u64>(list, u64::from(count)) }?;
    let first = NonNull::from(handles).cast::<NonNull<ProtocolObject<dyn MTLBinaryArchive>>>();
    // SAFETY: `first` points to `count` handles, read as the C read them; a nil one raises.
    Some(unsafe { NSArray::arrayWithObjects_count(first, usize::from(count)) })
}

/// Runs a send that takes the C's `NSError **`; the error comes back +1, as `[err retain]`.
fn with_error<T: Message>(
    send: impl FnOnce(&mut *mut NSError) -> Option<Retained<T>>,
) -> (Option<Retained<T>>, Option<Retained<NSError>>) {
    let mut error: *mut NSError = ptr::null_mut();
    let object = send(&mut error);
    // SAFETY: `error` is null or the autoreleased error the send set.
    (object, unsafe { Retained::retain(error) })
}

/// Adds the functions of a pipeline to the serialisation archive, whatever the result.
fn add_functions(
    handle: u64,
    add: impl FnOnce(&ProtocolObject<dyn MTLBinaryArchive>, &mut *mut NSError) -> bool,
) {
    if let Some(archive) = archive(handle) {
        let mut error: *mut NSError = ptr::null_mut();
        add(archive, &mut error);
    }
}

/// A string from the guest's bytes in `encoding`, nil (`None`) when they do not decode.
fn string_from_bytes(bytes: &[u8], encoding: NSUInteger) -> Option<Retained<NSString>> {
    let first = NonNull::from(bytes).cast::<c_void>();
    // SAFETY: `first` points to `bytes.len()` readable bytes (a dangling pointer for none).
    unsafe { NSString::initWithBytes_length_encoding(NSString::alloc(), first, bytes.len(), encoding) }
}

/// A string from a NUL-terminated C string as UTF-8, nil (`None`) when it does not decode.
fn string_from_c_str(s: &core::ffi::CStr) -> Option<Retained<NSString>> {
    let first = NonNull::from(s.to_bytes_with_nul()).cast::<c_char>();
    // SAFETY: `first` points to a NUL-terminated string.
    unsafe { NSString::initWithCString_encoding(NSString::alloc(), first, UTF8) }
}

/// The file URL of `path`, which raises for nil, as `-[NSURL initFileURLWithPath:]` does.
fn file_url(path: Option<&NSString>) -> Retained<NSURL> {
    let url = RawUrl::init_file_url_with_path(RawUrl::alloc(), path);
    // SAFETY: the same class under two declarations of it.
    unsafe { Retained::cast_unchecked::<NSURL>(url) }
}

/// Slot 25: a library from metallib data.
///
/// # Safety
///
/// `args` points to the slot's `UnixcallMtldeviceNewlibrary`.
#[unsafe(export_name = "_MTLDevice_newLibrary")]
pub unsafe extern "C-unwind" fn mtl_device_new_library(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtldeviceNewlibrary>(args) };
    let (library, error) = with_error(|error| {
        device(p.device)?.new_library_with_data(any_object(p.data), error)
    });
    p.ret_library = ret_owned(library);
    p.ret_error = ret_owned(error);
    STATUS_SUCCESS
}

/// Slot 26: a function of a library by name.
///
/// # Safety
///
/// `args` points to the slot's `UnixcallGenericObjUint64ObjRet`.
#[unsafe(export_name = "_MTLLibrary_newFunction")]
pub unsafe extern "C-unwind" fn mtl_library_new_function(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjUint64ObjRet>(args) };
    // SAFETY: the guest's name is NUL-terminated; null is the C's fault.
    let Some(name) = (unsafe { mem_cstr(MemPtr { ptr: p.arg }) }) else {
        return STATUS_SUCCESS;
    };
    let name = string_from_c_str(name);
    p.ret = ret_owned(library(p.handle).and_then(|l| l.new_function(name.as_deref())));
    STATUS_SUCCESS
}

/// Slot 29: a compute pipeline state.
///
/// # Safety
///
/// `args` points to the slot's `UnixcallMtldeviceNewcomputepso`.
#[unsafe(export_name = "_MTLDevice_newComputePipelineState")]
pub unsafe extern "C-unwind" fn mtl_device_new_compute_pipeline_state(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtldeviceNewcomputepso>(args) };
    // SAFETY: the guest's info block, null is the C's fault.
    let Some(info) = (unsafe { mem_ref::<WMTComputePipelineInfo>(p.info) }) else {
        return STATUS_SUCCESS;
    };
    let descriptor = MTLComputePipelineDescriptor::new();
    descriptor.setComputeFunction(function(info.compute_function));
    // SAFETY: a flag.
    unsafe {
        descriptor.setThreadGroupSizeIsMultipleOfThreadExecutionWidth(flag(info.tgsize_is_multiple_of_sgwidth));
    }
    descriptor.setSupportIndirectCommandBuffers(flag(info.support_indirect_command_buffers));
    set_immutable(&descriptor.buffers(), info.immutable_buffers);
    if let Some(archives) = lookup_archives(info.num_binary_archives_for_lookup, info.binary_archives_for_lookup) {
        descriptor.setBinaryArchives(Some(&archives));
    }
    let options = pipeline_option(info.fail_on_binary_archive_miss);
    let (pso, error) = with_error(|error| {
        device(p.device)?.new_compute_pipeline_state(&descriptor, options, ptr::null_mut(), error)
    });
    p.ret_pso = ret_owned(pso);
    let failed = error.is_some();
    p.ret_error = ret_owned(error);
    if !failed {
        add_functions(info.binary_archive_for_serialization, |a, e| a.add_compute(&descriptor, e));
    }
    STATUS_SUCCESS
}

/// The render pipeline descriptor slots 34 and 156 share.
fn render_pipeline_descriptor(info: &WMTRenderPipelineInfo) -> Retained<MTLRenderPipelineDescriptor> {
    let descriptor = MTLRenderPipelineDescriptor::new();
    set_color_attachments(&descriptor.colorAttachments(), &info.colors);
    set_immutable(&descriptor.fragmentBuffers(), info.immutable_fragment_buffers);
    set_immutable(&descriptor.vertexBuffers(), info.immutable_vertex_buffers);
    {
        // SAFETY: the same class under two declarations of it.
        let private = unsafe { &*ptr::from_ref(&*descriptor).cast::<PrivateRenderDescriptor>() };
        private.set_logic_operation_enabled(flag(info.logic_operation_enabled));
        private.set_logic_operation(usize::from(info.logic_operation));
    }
    descriptor.setDepthAttachmentPixelFormat(pixel_format(info.depth_pixel_format));
    descriptor.setStencilAttachmentPixelFormat(pixel_format(info.stencil_pixel_format));
    descriptor.setAlphaToCoverageEnabled(flag(info.alpha_to_coverage_enabled));
    descriptor.setRasterizationEnabled(flag(info.rasterization_enabled));
    descriptor.setRasterSampleCount(usize::from(info.raster_sample_count));
    // SAFETY: Metal checks the topology when it makes a pipeline.
    unsafe {
        descriptor.setInputPrimitiveTopology(MTLPrimitiveTopologyClass(info.input_primitive_topology as usize));
    }
    // SAFETY: as above.
    unsafe {
        descriptor.setTessellationPartitionMode(MTLTessellationPartitionMode(usize::from(
            info.tessellation_partition_mode,
        )));
    }
    descriptor.setTessellationFactorStepFunction(MTLTessellationFactorStepFunction(usize::from(
        info.tessellation_factor_step,
    )));
    descriptor.setTessellationOutputWindingOrder(MTLWinding(usize::from(
        info.tessellation_output_winding_order,
    )));
    // SAFETY: as above.
    unsafe { descriptor.setMaxTessellationFactor(usize::from(info.max_tessellation_factor)) };
    descriptor.setVertexFunction(function(info.vertex_function));
    descriptor.setFragmentFunction(function(info.fragment_function));
    descriptor.setSupportIndirectCommandBuffers(flag(info.support_indirect_command_buffers));
    if let Some(archives) = lookup_archives(info.num_binary_archives_for_lookup, info.binary_archives_for_lookup) {
        descriptor.setBinaryArchives(Some(&archives));
    }
    descriptor
}

/// Slots 34 and 156: the pipeline of `descriptor` and its error, then the serialisation archive.
///
/// The pipeline and the error are written to the two fields before the add, as the C did, so an
/// add that raises leaves them written.
fn render_pipeline_state(
    device_handle: u64,
    info: &WMTRenderPipelineInfo,
    descriptor: &MTLRenderPipelineDescriptor,
    ret_pso: &mut u64,
    ret_error: &mut u64,
) {
    let options = pipeline_option(info.fail_on_binary_archive_miss);
    let (pso, error) = with_error(|error| {
        device(device_handle)?.new_render_pipeline_state(descriptor, options, ptr::null_mut(), error)
    });
    *ret_pso = ret_owned(pso);
    let failed = error.is_some();
    *ret_error = ret_owned(error);
    if !failed {
        add_functions(info.binary_archive_for_serialization, |a, e| a.add_render(descriptor, e));
    }
}

/// Slot 34: a render pipeline state.
///
/// # Safety
///
/// `args` points to the slot's `UnixcallMtldeviceNewrenderpso`.
#[unsafe(export_name = "_MTLDevice_newRenderPipelineState")]
pub unsafe extern "C-unwind" fn mtl_device_new_render_pipeline_state(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtldeviceNewrenderpso>(args) };
    // SAFETY: the guest's info block, null is the C's fault.
    let Some(info) = (unsafe { mem_ref::<WMTRenderPipelineInfo>(p.info) }) else {
        return STATUS_SUCCESS;
    };
    let descriptor = render_pipeline_descriptor(info);
    render_pipeline_state(p.device, info, &descriptor, &mut p.ret_pso, &mut p.ret_error);
    STATUS_SUCCESS
}

/// The vertex descriptor of slot 156.
///
/// An attribute without a format and a layout without a stride are left unset.
fn vertex_descriptor(info: &WMTVertexDescriptorInfo) -> Retained<MTLVertexDescriptor> {
    let vertex = MTLVertexDescriptor::vertexDescriptor();
    let attributes = vertex.attributes();
    let layouts = vertex.layouts();
    for (i, (attribute, layout)) in info.attributes.iter().zip(&info.layouts).enumerate() {
        if attribute.format != 0 {
            // SAFETY: `i` is below 31, the count of vertex attributes.
            let a: Retained<MTLVertexAttributeDescriptor> = unsafe { attributes.objectAtIndexedSubscript(i) };
            a.setFormat(MTLVertexFormat(attribute.format as usize));
            // SAFETY: Metal checks the offset when it makes a pipeline.
            unsafe { a.setOffset(attribute.offset as usize) };
            // SAFETY: as above.
            unsafe { a.setBufferIndex(attribute.buffer_index as usize) };
        }
        if layout.stride != 0 {
            // SAFETY: `i` is below 31, the count of vertex buffer layouts.
            let l: Retained<MTLVertexBufferLayoutDescriptor> = unsafe { layouts.objectAtIndexedSubscript(i) };
            // SAFETY: Metal checks the stride when it makes a pipeline.
            unsafe { l.setStride(layout.stride as usize) };
            l.setStepFunction(MTLVertexStepFunction(layout.step_function as usize));
            // SAFETY: as above.
            unsafe { l.setStepRate(layout.step_rate as usize) };
        }
    }
    vertex
}

/// Slot 156: slot 34's pipeline with a vertex descriptor.
///
/// # Safety
///
/// `args` points to the slot's `UnixcallMtldeviceNewrenderpsoVertex`.
#[unsafe(export_name = "_MTLDevice_newRenderPipelineStateWithVertexDescriptor")]
pub unsafe extern "C-unwind" fn mtl_device_new_render_pipeline_state_with_vertex_descriptor(
    args: *mut c_void,
) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtldeviceNewrenderpsoVertex>(args) };
    // SAFETY: the guest's info block, null is the C's fault.
    let Some(info) = (unsafe { mem_ref::<WMTRenderPipelineInfo>(p.info) }) else {
        return STATUS_SUCCESS;
    };
    let descriptor = render_pipeline_descriptor(info);
    // SAFETY: the guest's vertex block, null is the C's fault.
    let Some(vertex) = (unsafe { mem_ref::<WMTVertexDescriptorInfo>(p.vertex) }) else {
        return STATUS_SUCCESS;
    };
    descriptor.setVertexDescriptor(Some(&vertex_descriptor(vertex)));
    render_pipeline_state(p.device, info, &descriptor, &mut p.ret_pso, &mut p.ret_error);
    STATUS_SUCCESS
}

/// Slot 35: a mesh render pipeline state.
///
/// # Safety
///
/// `args` points to the slot's `UnixcallMtldeviceNewmeshrenderpso`.
#[unsafe(export_name = "_MTLDevice_newMeshRenderPipelineState")]
pub unsafe extern "C-unwind" fn mtl_device_new_mesh_render_pipeline_state(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtldeviceNewmeshrenderpso>(args) };
    // SAFETY: the guest's info block, null is the C's fault.
    let Some(info) = (unsafe { mem_ref::<WMTMeshRenderPipelineInfo>(p.info) }) else {
        return STATUS_SUCCESS;
    };
    let descriptor = MTLMeshRenderPipelineDescriptor::new();
    set_color_attachments(&descriptor.colorAttachments(), &info.colors);
    set_immutable(&descriptor.fragmentBuffers(), info.immutable_fragment_buffers);
    set_immutable(&descriptor.meshBuffers(), info.immutable_mesh_buffers);
    set_immutable(&descriptor.objectBuffers(), info.immutable_object_buffers);
    {
        // SAFETY: the same class under two declarations of it.
        let private = unsafe { &*ptr::from_ref(&*descriptor).cast::<PrivateMeshDescriptor>() };
        private.set_logic_operation_enabled(flag(info.logic_operation_enabled));
        private.set_logic_operation(usize::from(info.logic_operation));
    }
    descriptor.setDepthAttachmentPixelFormat(pixel_format(info.depth_pixel_format));
    descriptor.setStencilAttachmentPixelFormat(pixel_format(info.stencil_pixel_format));
    descriptor.setAlphaToCoverageEnabled(flag(info.alpha_to_coverage_enabled));
    descriptor.setRasterizationEnabled(flag(info.rasterization_enabled));
    // SAFETY: Metal checks the count when it makes a pipeline.
    unsafe { descriptor.setRasterSampleCount(usize::from(info.raster_sample_count)) };
    // SAFETY: a function handle the caller holds for the call.
    unsafe { descriptor.setObjectFunction(function(info.object_function)) };
    // SAFETY: as above.
    unsafe { descriptor.setMeshFunction(function(info.mesh_function)) };
    // SAFETY: as above.
    unsafe { descriptor.setFragmentFunction(function(info.fragment_function)) };
    descriptor.setPayloadMemoryLength(usize::from(info.payload_memory_length));
    descriptor
        .setMeshThreadgroupSizeIsMultipleOfThreadExecutionWidth(flag(info.mesh_tgsize_is_multiple_of_sgwidth));
    descriptor
        .setObjectThreadgroupSizeIsMultipleOfThreadExecutionWidth(flag(info.object_tgsize_is_multiple_of_sgwidth));
    descriptor.setSupportIndirectCommandBuffers(flag(info.support_indirect_command_buffers));
    if let Some(archives) = lookup_archives(info.num_binary_archives_for_lookup, info.binary_archives_for_lookup) {
        descriptor.setBinaryArchives(Some(&archives));
    }
    let options = pipeline_option(info.fail_on_binary_archive_miss);
    let (pso, error) = with_error(|error| {
        device(p.device)?.new_mesh_render_pipeline_state(&descriptor, options, ptr::null_mut(), error)
    });
    p.ret_pso = ret_owned(pso);
    let failed = error.is_some();
    p.ret_error = ret_owned(error);
    if !failed {
        add_functions(info.binary_archive_for_serialization, |a, e| a.add_mesh(&descriptor, e));
    }
    STATUS_SUCCESS
}

/// Slot 131: a tile render pipeline state.
///
/// # Safety
///
/// `args` points to the slot's `UnixcallMtldeviceNewrenderpso`.
#[unsafe(export_name = "_MTLDevice_newTileRenderPipelineState")]
pub unsafe extern "C-unwind" fn mtl_device_new_tile_render_pipeline_state(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtldeviceNewrenderpso>(args) };
    // SAFETY: the guest's info block, null is the C's fault.
    let Some(info) = (unsafe { mem_ref::<WMTTileRenderPipelineInfo>(p.info) }) else {
        return STATUS_SUCCESS;
    };
    let descriptor = MTLTileRenderPipelineDescriptor::new();
    let attachments = descriptor.colorAttachments();
    for (i, format) in info.color_formats.iter().enumerate() {
        // SAFETY: `i` is below 8, the count of color attachments.
        let a = unsafe { attachments.objectAtIndexedSubscript(i) };
        a.setPixelFormat(pixel_format(*format));
    }
    set_immutable(&descriptor.tileBuffers(), info.immutable_tile_buffers);
    // SAFETY: Metal checks the count when it makes a pipeline.
    unsafe { descriptor.setRasterSampleCount(usize::from(info.raster_sample_count)) };
    descriptor.setThreadgroupSizeMatchesTileSize(flag(info.tgsize_matches_tile_size));
    {
        // SAFETY: the same class under two declarations of it.
        let raw = unsafe { &*ptr::from_ref(&*descriptor).cast::<RawTileDescriptor>() };
        raw.set_tile_function(function(info.tile_function));
    }
    if let Some(archives) = lookup_archives(info.num_binary_archives_for_lookup, info.binary_archives_for_lookup) {
        descriptor.setBinaryArchives(Some(&archives));
    }
    let options = pipeline_option(info.fail_on_binary_archive_miss);
    let (pso, error) = with_error(|error| {
        device(p.device)?.new_tile_render_pipeline_state(&descriptor, options, ptr::null_mut(), error)
    });
    p.ret_pso = ret_owned(pso);
    let failed = error.is_some();
    p.ret_error = ret_owned(error);
    if !failed {
        add_functions(info.binary_archive_for_serialization, |a, e| a.add_tile(&descriptor, e));
    }
    STATUS_SUCCESS
}

/// Slot 98: a function of a library with constant values.
///
/// # Safety
///
/// `args` points to the slot's `UnixcallMtllibraryNewfunctionWithConstants`.
#[unsafe(export_name = "_MTLLibrary_newFunctionWithConstants")]
pub unsafe extern "C-unwind" fn mtl_library_new_function_with_constants(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtllibraryNewfunctionWithConstants>(args) };
    // SAFETY: the guest's name is NUL-terminated; null is the C's fault.
    let Some(name) = (unsafe { mem_cstr(p.name) }) else {
        return STATUS_SUCCESS;
    };
    // SAFETY: the guest's table holds `num_constants` entries; null with a count is the C's fault.
    let Some(constants) = (unsafe { mem_slice::<WMTFunctionConstant>(p.constants, p.num_constants) }) else {
        return STATUS_SUCCESS;
    };
    let name = string_from_c_str(name);
    let values = MTLFunctionConstantValues::new();
    // A constant without data goes to Metal as the C sent it. For most types Metal never reads the
    // value: it records the type and the function then fails, with the error the C answered. For
    // the 48 scalar, vector and long types (3-6, 16-19, 29-56, 81-88, 121-124) it reads up to 32
    // bytes, where the C faulted: those read zeros, so no guest value crashes the host.
    let zeros = [0_u8; ZERO_VALUE_BYTES];
    for constant in constants {
        let given = core::ptr::with_exposed_provenance_mut::<c_void>(u64_to_usize(constant.data.ptr));
        let data = NonNull::new(given).unwrap_or_else(|| NonNull::from(&zeros).cast());
        // SAFETY: `data` is the guest's value for the type, or zeros as large as any value Metal
        // reads; Metal checks the index.
        unsafe {
            values.setConstantValue_type_atIndex(
                data,
                MTLDataType(usize::from(constant.r#type)),
                usize::from(constant.index),
            );
        }
    }
    let (function, error) = with_error(|error| {
        library(p.library)?.new_function_with_constants(name.as_deref(), &values, error)
    });
    p.ret = ret_owned(function);
    p.ret_error = ret_owned(error);
    STATUS_SUCCESS
}

/// Slot 112: a binary archive, empty or over a file.
///
/// # Safety
///
/// `args` points to the slot's `UnixcallMtldeviceNewbinaryarchive`.
#[unsafe(export_name = "_MTLDevice_newBinaryArchive")]
pub unsafe extern "C-unwind" fn mtl_device_new_binary_archive(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtldeviceNewbinaryarchive>(args) };
    let descriptor = MTLBinaryArchiveDescriptor::new();
    // SAFETY: the guest's path is NUL-terminated.
    if let Some(path) = unsafe { mem_cstr(p.url) } {
        let url = file_url(string_from_c_str(path).as_deref());
        descriptor.setUrl(Some(&url));
    }
    let (archive, error) = with_error(|error| device(p.device)?.new_binary_archive(&descriptor, error));
    p.ret_archive = ret_owned(archive);
    p.ret_error = ret_owned(error);
    STATUS_SUCCESS
}

/// Slot 113: serialises a binary archive to a file.
///
/// # Safety
///
/// `args` points to the slot's `UnixcallMtlbinaryarchiveSerialize`.
#[unsafe(export_name = "_MTLBinaryArchive_serialize")]
pub unsafe extern "C-unwind" fn mtl_binary_archive_serialize(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtlbinaryarchiveSerialize>(args) };
    // SAFETY: the guest's path is NUL-terminated; null is the C's fault.
    let Some(path) = (unsafe { mem_cstr(p.url) }) else {
        return STATUS_SUCCESS;
    };
    let url = file_url(string_from_c_str(path).as_deref());
    let mut error: *mut NSError = ptr::null_mut();
    if let Some(archive) = archive(p.archive) {
        archive.serialize_to_url(&url, &mut error);
    }
    // SAFETY: `error` is null or the autoreleased error the send set.
    p.ret_error = ret_owned(unsafe { Retained::retain(error) });
    STATUS_SUCCESS
}

/// Slot 144: a library from source, as MSL 3.1.
///
/// # Safety
///
/// `args` points to the slot's `UnixcallMtldeviceNewlibrarywithsource`.
#[unsafe(export_name = "_MTLDevice_newLibraryWithSource")]
pub unsafe extern "C-unwind" fn mtl_device_new_library_with_source(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtldeviceNewlibrarywithsource>(args) };
    // SAFETY: the guest's source holds `length` bytes; null with a length is the C's fault.
    let Some(bytes) = (unsafe { mem_slice::<u8>(p.source, p.length) }) else {
        return STATUS_SUCCESS;
    };
    let source = string_from_bytes(bytes, ASCII);
    let options = MTLCompileOptions::new();
    options.setLanguageVersion(MTLLanguageVersion::Version3_1);
    let (library, error) = with_error(|error| {
        device(p.device)?.new_library_with_source(source.as_deref(), &options, error)
    });
    p.ret_library = ret_owned(library);
    p.ret_error = ret_owned(error);
    STATUS_SUCCESS
}

/// Slot 157: slot 144 with compile options.
///
/// # Safety
///
/// `args` points to the slot's `UnixcallMtldeviceNewlibrarywithsourceOptions`.
#[unsafe(export_name = "_MTLDevice_newLibraryWithSourceOptions")]
pub unsafe extern "C-unwind" fn mtl_device_new_library_with_source_options(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtldeviceNewlibrarywithsourceOptions>(args) };
    // SAFETY: the guest's options block, null is the C's fault.
    let Some(info) = (unsafe { mem_ref::<WMTCompileOptions>(p.options) }) else {
        return STATUS_SUCCESS;
    };
    // SAFETY: the guest's source holds `length` bytes; null with a length is the C's fault.
    let Some(bytes) = (unsafe { mem_slice::<u8>(p.source, p.length) }) else {
        return STATUS_SUCCESS;
    };
    let source = string_from_bytes(bytes, ASCII);
    let options = MTLCompileOptions::new();
    if info.language_version != 0 {
        options.setLanguageVersion(MTLLanguageVersion(u64_to_usize(info.language_version)));
    }
    options.setPreserveInvariance(flag(info.preserve_invariance));
    options.setMathMode(MTLMathMode(isize::from(info.math_mode)));
    let (library, error) = with_error(|error| {
        device(p.device)?.new_library_with_source(source.as_deref(), &options, error)
    });
    p.ret_library = ret_owned(library);
    p.ret_error = ret_owned(error);
    STATUS_SUCCESS
}

#[cfg(test)]
mod tests;
