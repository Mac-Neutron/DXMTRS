use core::ffi::CStr;

use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol};
use objc2_metal::MTLCreateSystemDefaultDevice;

use winemetal_wire::{WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_PRIMITIVE_TOPOLOGY_CLASS_UNSPECIFIED};

use super::*;

/// What a handler leaves in an output it does not write (the probe's fill).
const FILL: u64 = 0xA5A5_A5A5_A5A5_A5A5;

/// The address of an object as a handle.
fn handle<T>(o: &T) -> u64 {
    core::ptr::from_ref(o).expose_provenance() as u64
}

/// A +1 handle taken back, so the test releases it.
fn take(h: u64) -> Option<Retained<NSObject>> {
    // SAFETY: a +1 object a handler gave back, or null.
    unsafe { Retained::from_raw(core::ptr::with_exposed_provenance_mut::<NSObject>(u64_to_usize(h))) }
}

/// An object's retain count.
fn retain_count(h: u64) -> usize {
    take(h).map_or(0, |o| {
        let n = o.retainCount();
        core::mem::forget(o);
        n
    })
}

/// A pointer to a guest-memory value.
fn mem<T>(v: &T) -> MemPtr {
    MemPtr { ptr: core::ptr::from_ref(v).expose_provenance() as u64 }
}

const SOURCE: &CStr = c"#include <metal_stdlib>\nusing namespace metal;\nkernel void k(device uint *b [[buffer(0)]], uint i [[thread_position_in_grid]]) { b[i] = i; }\n";

/// A Metal device.
type Device = Retained<ProtocolObject<dyn MTLDevice>>;

/// A library of `SOURCE` through slot 144, on the system device.
fn library_of_source() -> Option<(Device, Retained<NSObject>)> {
    let device = MTLCreateSystemDefaultDevice()?;
    let mut p = UnixcallMtldeviceNewlibrarywithsource {
        device: handle(&*device),
        source: mem(&SOURCE.to_bytes()[0]),
        length: SOURCE.to_bytes().len() as u64,
        ret_error: FILL,
        ret_library: FILL,
    };
    // SAFETY: a live params block.
    assert_eq!(unsafe { mtl_device_new_library_with_source(core::ptr::from_mut(&mut p).cast()) }, 0);
    assert_eq!(p.ret_error, 0);
    Some((device, take(p.ret_library)?))
}

#[test]
fn immutable_slots_are_bits_0_to_30() {
    assert_eq!(immutable_slots(0).count(), 0);
    assert_eq!(immutable_slots(0x8000_0000).count(), 0);
    assert_eq!(immutable_slots(0xffff_ffff).count(), 31);
    assert_eq!(immutable_slots(0x4000_0081).collect::<Vec<_>>(), [0, 7, 30]);
}

#[test]
fn any_non_zero_byte_is_a_flag() {
    assert!(!flag(0));
    assert!(flag(1));
    assert!(flag(2));
    assert!(flag(255));
    assert_eq!(pipeline_option(0), MTLPipelineOption::None);
    assert_eq!(pipeline_option(7), MTLPipelineOption::FailOnBinaryArchiveMiss);
}

#[test]
fn source_is_ascii_and_names_are_utf8() {
    // ASCII takes every byte, UTF-8 refuses 0xff: the encodings the C asked for.
    assert!(string_from_bytes(b"a\xffb", ASCII).is_some());
    assert!(string_from_bytes(b"a\xffb", UTF8).is_none());
    assert_eq!(string_from_bytes(b"", ASCII).map(|s| s.len()), Some(0));
    assert!(string_from_c_str(c"fine").is_some());
    assert!(string_from_c_str(c"\xff").is_none());
}

#[test]
fn lookup_archives_need_a_count_and_a_list() {
    let none = MemPtr { ptr: 0 };
    assert!(lookup_archives(0, none).is_none());
    assert!(lookup_archives(1, none).is_none());
    let empty: [u64; 0] = [];
    assert!(lookup_archives(0, mem(&empty)).is_none());
}

#[test]
fn nil_device_writes_a_nil_library_and_error() {
    let mut p = UnixcallMtldeviceNewlibrary { device: 0, data: 0, ret_error: FILL, ret_library: FILL };
    // SAFETY: a live params block.
    assert_eq!(unsafe { mtl_device_new_library(core::ptr::from_mut(&mut p).cast()) }, 0);
    assert_eq!((p.ret_library, p.ret_error), (0, 0));
    let mut s = UnixcallMtldeviceNewlibrarywithsource {
        device: 0,
        source: mem(&SOURCE.to_bytes()[0]),
        length: SOURCE.to_bytes().len() as u64,
        ret_error: FILL,
        ret_library: FILL,
    };
    // SAFETY: a live params block.
    assert_eq!(unsafe { mtl_device_new_library_with_source(core::ptr::from_mut(&mut s).cast()) }, 0);
    assert_eq!((s.ret_library, s.ret_error), (0, 0));
}

#[test]
fn a_function_is_made_by_name() {
    let Some((_device, library)) = library_of_source() else { return };
    for (name, found) in [(c"k", true), (c"nope", false), (c"", false)] {
        let mut p = UnixcallGenericObjUint64ObjRet {
            handle: handle(&*library),
            arg: name.as_ptr().expose_provenance() as u64,
            ret: FILL,
        };
        // SAFETY: a live params block.
        assert_eq!(unsafe { mtl_library_new_function(core::ptr::from_mut(&mut p).cast()) }, 0);
        assert_eq!(p.ret != 0, found, "{name:?}");
        drop(take(p.ret));
    }
}

#[test]
fn a_null_name_writes_nothing() {
    let Some((_device, library)) = library_of_source() else { return };
    let mut p = UnixcallGenericObjUint64ObjRet { handle: handle(&*library), arg: 0, ret: FILL };
    // SAFETY: a live params block.
    assert_eq!(unsafe { mtl_library_new_function(core::ptr::from_mut(&mut p).cast()) }, 0);
    assert_eq!(p.ret, FILL);
}

#[test]
fn a_syntax_error_gives_an_error_the_handler_retained() {
    let Some((device, _library)) = library_of_source() else { return };
    let bad = c"kernel void broken( { }\n";
    let mut p = UnixcallMtldeviceNewlibrarywithsource {
        device: handle(&*device),
        source: mem(&bad.to_bytes()[0]),
        length: bad.to_bytes().len() as u64,
        ret_error: FILL,
        ret_library: FILL,
    };
    // SAFETY: a live params block.
    assert_eq!(unsafe { mtl_device_new_library_with_source(core::ptr::from_mut(&mut p).cast()) }, 0);
    assert_eq!(p.ret_library, 0);
    // Metal's autoreleased error and the handler's retain.
    assert!(retain_count(p.ret_error) >= 2);
    drop(take(p.ret_error));
}

#[test]
fn an_empty_archive_cannot_be_serialized() {
    let Some(device) = MTLCreateSystemDefaultDevice() else { return };
    let mut a = UnixcallMtldeviceNewbinaryarchive {
        device: handle(&*device),
        url: MemPtr { ptr: 0 },
        ret_archive: FILL,
        ret_error: FILL,
    };
    // SAFETY: a live params block.
    assert_eq!(unsafe { mtl_device_new_binary_archive(core::ptr::from_mut(&mut a).cast()) }, 0);
    assert_eq!(a.ret_error, 0);
    assert_eq!(retain_count(a.ret_archive), 1);
    let path = c"/winemetal-g4-unit/no such folder/archive.bin";
    let mut s = UnixcallMtlbinaryarchiveSerialize { archive: a.ret_archive, url: mem(&path.to_bytes()[0]), ret_error: FILL };
    // SAFETY: a live params block.
    assert_eq!(unsafe { mtl_binary_archive_serialize(core::ptr::from_mut(&mut s).cast()) }, 0);
    assert_ne!(s.ret_error, 0);
    assert_ne!(s.ret_error, FILL);
    drop(take(s.ret_error));
    drop(take(a.ret_archive));
}

#[test]
fn null_info_blocks_write_nothing() {
    let mut c = UnixcallMtldeviceNewcomputepso { device: 0, info: MemPtr { ptr: 0 }, ret_error: FILL, ret_pso: FILL };
    // SAFETY: a live params block.
    assert_eq!(unsafe { mtl_device_new_compute_pipeline_state(core::ptr::from_mut(&mut c).cast()) }, 0);
    assert_eq!((c.ret_pso, c.ret_error), (FILL, FILL));
    let mut o = UnixcallMtldeviceNewlibrarywithsourceOptions {
        device: 0,
        source: MemPtr { ptr: 0 },
        length: 0,
        options: MemPtr { ptr: 0 },
        ret_error: FILL,
        ret_library: FILL,
    };
    // SAFETY: a live params block.
    assert_eq!(unsafe { mtl_device_new_library_with_source_options(core::ptr::from_mut(&mut o).cast()) }, 0);
    assert_eq!((o.ret_library, o.ret_error), (FILL, FILL));
}

#[test]
fn constants_with_a_null_table_and_a_count_write_nothing() {
    let Some((_device, library)) = library_of_source() else { return };
    let mut p = UnixcallMtllibraryNewfunctionWithConstants {
        library: handle(&*library),
        name: mem(&c"k".to_bytes()[0]),
        constants: MemPtr { ptr: 0 },
        num_constants: 3,
        ret: FILL,
        ret_error: FILL,
    };
    // SAFETY: a live params block.
    assert_eq!(unsafe { mtl_library_new_function_with_constants(core::ptr::from_mut(&mut p).cast()) }, 0);
    assert_eq!((p.ret, p.ret_error), (FILL, FILL));
}

#[test]
fn no_constants_still_asks_metal() {
    let Some((_device, library)) = library_of_source() else { return };
    let mut p = UnixcallMtllibraryNewfunctionWithConstants {
        library: handle(&*library),
        name: mem(&c"k".to_bytes()[0]),
        constants: MemPtr { ptr: 0 },
        num_constants: 0,
        ret: FILL,
        ret_error: FILL,
    };
    // SAFETY: a live params block.
    assert_eq!(unsafe { mtl_library_new_function_with_constants(core::ptr::from_mut(&mut p).cast()) }, 0);
    // A function without constants is made; the pair was written, not left as the fill.
    assert_ne!(p.ret, FILL);
    assert_ne!(p.ret_error, FILL);
    drop(take(p.ret));
    drop(take(p.ret_error));
}

const PIPELINE_SOURCE: &CStr = c"#include <metal_stdlib>
using namespace metal;
kernel void k(device uint *b [[buffer(0)]], uint i [[thread_position_in_grid]]) { b[i] = i; }
vertex float4 v(uint i [[vertex_id]]) { return float4(0, 0, 0, 1); }
fragment half4 f() { return half4(1); }
struct VIn { float4 pos [[attribute(0)]]; float2 uv [[attribute(1)]]; };
vertex float4 vin(VIn in [[stage_in]]) { return in.pos + float4(in.uv, 0, 0); }
";

const MESH_SOURCE: &CStr = c"#include <metal_stdlib>
#include <metal_mesh>
using namespace metal;
struct MV { float4 position [[position]]; };
using MM = mesh<MV, void, 3, 1, topology::triangle>;
[[mesh]] void mshf(MM m, uint tid [[thread_index_in_threadgroup]]) {
  if (tid < 3) { MV v; v.position = float4(float(tid), 0, 0, 1); m.set_vertex(tid, v); m.set_index(tid, tid); }
  if (tid == 0) m.set_primitive_count(1);
}
fragment half4 mf() { return half4(1); }
";

const TILE_SOURCE: &CStr = c"#include <metal_stdlib>
using namespace metal;
struct TD { half4 c [[color(0)]]; };
kernel void tf(imageblock<TD, imageblock_layout_implicit> ib, ushort2 tid [[thread_position_in_threadgroup]]) {
}
";

/// A library of `source` on `device` through slot 144, `None` when Metal refuses it.
fn compile(device: u64, source: &CStr) -> Option<Retained<NSObject>> {
    let mut p = UnixcallMtldeviceNewlibrarywithsource {
        device,
        source: mem(&source.to_bytes()[0]),
        length: source.to_bytes().len() as u64,
        ret_error: FILL,
        ret_library: FILL,
    };
    // SAFETY: a live params block.
    assert_eq!(unsafe { mtl_device_new_library_with_source(core::ptr::from_mut(&mut p).cast()) }, 0);
    drop(take(p.ret_error));
    take(p.ret_library)
}

/// A function of `library` through slot 26.
fn function_in(library: &NSObject, name: &CStr) -> u64 {
    let mut p = UnixcallGenericObjUint64ObjRet {
        handle: handle(library),
        arg: name.as_ptr().expose_provenance() as u64,
        ret: FILL,
    };
    // SAFETY: a live params block.
    assert_eq!(unsafe { mtl_library_new_function(core::ptr::from_mut(&mut p).cast()) }, 0);
    assert_ne!(p.ret, 0, "{name:?}");
    p.ret
}

/// A fresh empty archive through slot 112.
fn empty_archive(device: u64) -> u64 {
    let mut a = UnixcallMtldeviceNewbinaryarchive { device, url: MemPtr { ptr: 0 }, ret_archive: FILL, ret_error: FILL };
    // SAFETY: a live params block.
    assert_eq!(unsafe { mtl_device_new_binary_archive(core::ptr::from_mut(&mut a).cast()) }, 0);
    assert_eq!(a.ret_error, 0);
    a.ret_archive
}

/// A render pipeline info over the two functions, valid for Metal.
fn render_info(vertex: u64, fragment: u64, archive: u64) -> WMTRenderPipelineInfo {
    // SAFETY: plain data, all zeroes is a valid value.
    let mut info: WMTRenderPipelineInfo = unsafe { core::mem::zeroed() };
    info.colors[0].pixel_format = WMT_PIXEL_FORMAT_BGRA8_UNORM;
    info.colors[0].write_mask = 15;
    info.rasterization_enabled = 1;
    info.raster_sample_count = 1;
    info.vertex_function = vertex;
    info.fragment_function = fragment;
    info.input_primitive_topology = WMT_PRIMITIVE_TOPOLOGY_CLASS_UNSPECIFIED;
    info.max_tessellation_factor = 16;
    info.binary_archive_for_serialization = archive;
    info
}

/// Serialises `archive` through slot 113 into a file of the temp folder, true when it wrote one.
fn serializes(archive: u64, name: &str) -> bool {
    let path = std::env::temp_dir().join(format!("winemetal-g4-unit-{}-{name}.bin", std::process::id()));
    let c_path = std::ffi::CString::new(path.to_str().unwrap_or_default()).unwrap_or_default();
    let mut s = UnixcallMtlbinaryarchiveSerialize { archive, url: mem(&c_path.to_bytes_with_nul()[0]), ret_error: FILL };
    // SAFETY: a live params block.
    assert_eq!(unsafe { mtl_binary_archive_serialize(core::ptr::from_mut(&mut s).cast()) }, 0);
    drop(take(s.ret_error));
    let written = std::fs::metadata(&path).is_ok_and(|m| m.len() > 0);
    drop(std::fs::remove_file(&path));
    written
}

#[test]
fn each_pipeline_slot_adds_its_functions_to_the_archive() {
    let Some(device) = MTLCreateSystemDefaultDevice() else { return };
    let dev = handle(&*device);
    let Some(library) = compile(dev, PIPELINE_SOURCE) else { return };
    let (kernel, vertex_fn, fragment_fn, staged_fn) = (
        function_in(&library, c"k"),
        function_in(&library, c"v"),
        function_in(&library, c"f"),
        function_in(&library, c"vin"),
    );

    let archive = empty_archive(dev);
    assert!(!serializes(archive, "empty"), "an empty archive has nothing to write");
    // SAFETY: plain data, all zeroes is a valid value.
    let mut compute: WMTComputePipelineInfo = unsafe { core::mem::zeroed() };
    compute.compute_function = kernel;
    compute.binary_archive_for_serialization = archive;
    let mut compute_call = UnixcallMtldeviceNewcomputepso { device: dev, info: mem(&compute), ret_error: FILL, ret_pso: FILL };
    // SAFETY: a live params block.
    assert_eq!(unsafe { mtl_device_new_compute_pipeline_state(core::ptr::from_mut(&mut compute_call).cast()) }, 0);
    assert_eq!(compute_call.ret_error, 0);
    assert_eq!(retain_count(compute_call.ret_pso), 1);
    assert!(serializes(archive, "compute"));

    let archive = empty_archive(dev);
    let render = render_info(vertex_fn, fragment_fn, archive);
    let mut render_call = UnixcallMtldeviceNewrenderpso { device: dev, info: mem(&render), ret_error: FILL, ret_pso: FILL };
    // SAFETY: a live params block.
    assert_eq!(unsafe { mtl_device_new_render_pipeline_state(core::ptr::from_mut(&mut render_call).cast()) }, 0);
    assert_eq!(render_call.ret_error, 0);
    assert_eq!(retain_count(render_call.ret_pso), 1);
    assert!(serializes(archive, "render"));

    let archive = empty_archive(dev);
    let render = render_info(staged_fn, fragment_fn, archive);
    // SAFETY: plain data, all zeroes is a valid value.
    let mut vertex: WMTVertexDescriptorInfo = unsafe { core::mem::zeroed() };
    vertex.attributes[0].format = 31;
    vertex.attributes[1].format = 29;
    vertex.attributes[1].offset = 16;
    vertex.layouts[0].stride = 24;
    vertex.layouts[0].step_function = 1;
    vertex.layouts[0].step_rate = 1;
    let mut vertex_call = UnixcallMtldeviceNewrenderpsoVertex {
        device: dev,
        info: mem(&render),
        vertex: mem(&vertex),
        ret_error: FILL,
        ret_pso: FILL,
    };
    // SAFETY: a live params block.
    assert_eq!(unsafe { mtl_device_new_render_pipeline_state_with_vertex_descriptor(core::ptr::from_mut(&mut vertex_call).cast()) }, 0);
    assert_eq!(vertex_call.ret_error, 0);
    assert_eq!(retain_count(vertex_call.ret_pso), 1);
    assert!(serializes(archive, "vertex"));

    if let Some(mesh_library) = compile(dev, MESH_SOURCE) {
        let archive = empty_archive(dev);
        // SAFETY: plain data, all zeroes is a valid value.
        let mut mesh: WMTMeshRenderPipelineInfo = unsafe { core::mem::zeroed() };
        mesh.colors[0].pixel_format = WMT_PIXEL_FORMAT_BGRA8_UNORM;
        mesh.colors[0].write_mask = 15;
        mesh.rasterization_enabled = 1;
        mesh.raster_sample_count = 1;
        mesh.mesh_function = function_in(&mesh_library, c"mshf");
        mesh.fragment_function = function_in(&mesh_library, c"mf");
        mesh.binary_archive_for_serialization = archive;
        let mut mesh_call = UnixcallMtldeviceNewmeshrenderpso { device: dev, info: mem(&mesh), ret_error: FILL, ret_pso: FILL };
        // SAFETY: a live params block.
        assert_eq!(unsafe { mtl_device_new_mesh_render_pipeline_state(core::ptr::from_mut(&mut mesh_call).cast()) }, 0);
        assert_eq!(mesh_call.ret_error, 0);
        assert_eq!(retain_count(mesh_call.ret_pso), 1);
        assert!(serializes(archive, "mesh"));
    }

    if let Some(tile_library) = compile(dev, TILE_SOURCE) {
        let archive = empty_archive(dev);
        // SAFETY: plain data, all zeroes is a valid value.
        let mut tile: WMTTileRenderPipelineInfo = unsafe { core::mem::zeroed() };
        tile.tile_function = function_in(&tile_library, c"tf");
        tile.color_formats[0] = WMT_PIXEL_FORMAT_BGRA8_UNORM;
        tile.raster_sample_count = 1;
        tile.binary_archive_for_serialization = archive;
        let mut tile_call = UnixcallMtldeviceNewrenderpso { device: dev, info: mem(&tile), ret_error: FILL, ret_pso: FILL };
        // SAFETY: a live params block.
        assert_eq!(unsafe { mtl_device_new_tile_render_pipeline_state(core::ptr::from_mut(&mut tile_call).cast()) }, 0);
        assert_eq!(tile_call.ret_error, 0);
        assert_eq!(retain_count(tile_call.ret_pso), 1);
        assert!(serializes(archive, "tile"));
    }
}

#[test]
fn a_nil_device_still_adds_to_the_archive() {
    let Some(device) = MTLCreateSystemDefaultDevice() else { return };
    let dev = handle(&*device);
    let Some(library) = compile(dev, PIPELINE_SOURCE) else { return };
    let archive = empty_archive(dev);
    // SAFETY: plain data, all zeroes is a valid value.
    let mut compute: WMTComputePipelineInfo = unsafe { core::mem::zeroed() };
    compute.compute_function = function_in(&library, c"k");
    compute.binary_archive_for_serialization = archive;
    let mut compute_call = UnixcallMtldeviceNewcomputepso { device: 0, info: mem(&compute), ret_error: FILL, ret_pso: FILL };
    // SAFETY: a live params block.
    assert_eq!(unsafe { mtl_device_new_compute_pipeline_state(core::ptr::from_mut(&mut compute_call).cast()) }, 0);
    assert_eq!((compute_call.ret_pso, compute_call.ret_error), (0, 0));
    assert!(serializes(archive, "nil-device"));
}
