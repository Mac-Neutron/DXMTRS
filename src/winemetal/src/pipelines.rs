//! Libraries, functions, pipeline states and binary archives.

use core::ffi::c_char;

use winemetal_wire::{
    UnixcallGenericObjUint64ObjRet, UnixcallMtlbinaryarchiveSerialize,
    UnixcallMtldeviceNewbinaryarchive, UnixcallMtldeviceNewcomputepso, UnixcallMtldeviceNewlibrary,
    UnixcallMtldeviceNewlibrarywithsource, UnixcallMtldeviceNewlibrarywithsourceOptions,
    UnixcallMtldeviceNewmeshrenderpso, UnixcallMtldeviceNewrenderpso,
    UnixcallMtldeviceNewrenderpsoVertex, UnixcallMtllibraryNewfunctionWithConstants,
    WMTCompileOptions, WMTComputePipelineInfo, WMTFunctionConstant, WMTMeshRenderPipelineInfo,
    WMTRenderPipelineInfo, WMTTileRenderPipelineInfo, WMTVertexDescriptorInfo, slot,
};

use crate::transport::{addr, call, mem_ptr, write_if};

exports! {
    fn MTLDevice_newLibrary(device: u64, data: u64, err_out: *mut u64) -> u64 {
        let p = call(slot::MTL_DEVICE_NEW_LIBRARY, |p: &mut UnixcallMtldeviceNewlibrary| {
            p.device = device;
            p.data = data;
        });
        // SAFETY: null or valid for a write (the function's contract).
        unsafe { write_if(err_out, p.ret_error) };
        p.ret_library
    }

    fn MTLLibrary_newFunction(library: u64, name: *const c_char) -> u64 {
        call(slot::MTL_LIBRARY_NEW_FUNCTION, |p: &mut UnixcallGenericObjUint64ObjRet| {
            p.handle = library;
            p.arg = addr(name);
        })
        .ret
    }

    fn MTLDevice_newComputePipelineState(device: u64, info: *const WMTComputePipelineInfo, err_out: *mut u64) -> u64 {
        let p = call(slot::MTL_DEVICE_NEW_COMPUTE_PIPELINE_STATE, |p: &mut UnixcallMtldeviceNewcomputepso| {
            p.device = device;
            p.info = mem_ptr(info);
        });
        // SAFETY: null or valid for a write (the function's contract).
        unsafe { write_if(err_out, p.ret_error) };
        p.ret_pso
    }

    fn MTLDevice_newRenderPipelineState(device: u64, info: *const WMTRenderPipelineInfo, err_out: *mut u64) -> u64 {
        let p = call(slot::MTL_DEVICE_NEW_RENDER_PIPELINE_STATE, |p: &mut UnixcallMtldeviceNewrenderpso| {
            p.device = device;
            p.info = mem_ptr(info);
        });
        // SAFETY: null or valid for a write (the function's contract).
        unsafe { write_if(err_out, p.ret_error) };
        p.ret_pso
    }

    fn MTLDevice_newMeshRenderPipelineState(
        device: u64,
        info: *const WMTMeshRenderPipelineInfo,
        err_out: *mut u64,
    ) -> u64 {
        let p = call(slot::MTL_DEVICE_NEW_MESH_RENDER_PIPELINE_STATE, |p: &mut UnixcallMtldeviceNewmeshrenderpso| {
            p.device = device;
            p.info = mem_ptr(info);
        });
        // SAFETY: null or valid for a write (the function's contract).
        unsafe { write_if(err_out, p.ret_error) };
        p.ret_pso
    }

    fn MTLLibrary_newFunctionWithConstants(
        library: u64,
        name: *const c_char,
        constants: *const WMTFunctionConstant,
        num_constants: u32,
        err_out: *mut u64,
    ) -> u64 {
        let p = call(
            slot::MTL_LIBRARY_NEW_FUNCTION_WITH_CONSTANTS,
            |p: &mut UnixcallMtllibraryNewfunctionWithConstants| {
                p.library = library;
                p.name = mem_ptr(name);
                p.constants = mem_ptr(constants);
                p.num_constants = u64::from(num_constants);
            },
        );
        // SAFETY: null or valid for a write (the function's contract).
        unsafe { write_if(err_out, p.ret_error) };
        p.ret
    }

    fn MTLDevice_newBinaryArchive(device: u64, url: *const c_char, err_out: *mut u64) -> u64 {
        let p = call(slot::MTL_DEVICE_NEW_BINARY_ARCHIVE, |p: &mut UnixcallMtldeviceNewbinaryarchive| {
            p.device = device;
            p.url = mem_ptr(url);
        });
        // SAFETY: null or valid for a write (the function's contract).
        unsafe { write_if(err_out, p.ret_error) };
        p.ret_archive
    }

    fn MTLBinaryArchive_serialize(archive: u64, url: *const c_char, err_out: *mut u64) {
        let p = call(slot::MTL_BINARY_ARCHIVE_SERIALIZE, |p: &mut UnixcallMtlbinaryarchiveSerialize| {
            p.archive = archive;
            p.url = mem_ptr(url);
        });
        // SAFETY: null or valid for a write (the function's contract).
        unsafe { write_if(err_out, p.ret_error) };
    }

    fn MTLDevice_newTileRenderPipelineState(
        device: u64,
        info: *const WMTTileRenderPipelineInfo,
        err_out: *mut u64,
    ) -> u64 {
        let p = call(slot::MTL_DEVICE_NEW_TILE_RENDER_PIPELINE_STATE, |p: &mut UnixcallMtldeviceNewrenderpso| {
            p.device = device;
            p.info = mem_ptr(info);
        });
        // SAFETY: null or valid for a write (the function's contract).
        unsafe { write_if(err_out, p.ret_error) };
        p.ret_pso
    }

    fn MTLDevice_newLibraryWithSource(device: u64, source: *const c_char, length: u64, err_out: *mut u64) -> u64 {
        let p = call(slot::MTL_DEVICE_NEW_LIBRARY_WITH_SOURCE, |p: &mut UnixcallMtldeviceNewlibrarywithsource| {
            p.device = device;
            p.source = mem_ptr(source);
            p.length = length;
        });
        // SAFETY: null or valid for a write (the function's contract).
        unsafe { write_if(err_out, p.ret_error) };
        p.ret_library
    }

    fn MTLDevice_newLibraryWithSourceOptions(
        device: u64,
        source: *const c_char,
        length: u64,
        options: *const WMTCompileOptions,
        err_out: *mut u64,
    ) -> u64 {
        let p = call(
            slot::MTL_DEVICE_NEW_LIBRARY_WITH_SOURCE_OPTIONS,
            |p: &mut UnixcallMtldeviceNewlibrarywithsourceOptions| {
                p.device = device;
                p.source = mem_ptr(source);
                p.length = length;
                p.options = mem_ptr(options);
            },
        );
        // SAFETY: null or valid for a write (the function's contract).
        unsafe { write_if(err_out, p.ret_error) };
        p.ret_library
    }

    fn MTLDevice_newRenderPipelineStateWithVertexDescriptor(
        device: u64,
        info: *const WMTRenderPipelineInfo,
        vertex: *const WMTVertexDescriptorInfo,
        err_out: *mut u64,
    ) -> u64 {
        let p = call(
            slot::MTL_DEVICE_NEW_RENDER_PIPELINE_STATE_WITH_VERTEX_DESCRIPTOR,
            |p: &mut UnixcallMtldeviceNewrenderpsoVertex| {
                p.device = device;
                p.info = mem_ptr(info);
                p.vertex = mem_ptr(vertex);
            },
        );
        // SAFETY: null or valid for a write (the function's contract).
        unsafe { write_if(err_out, p.ret_error) };
        p.ret_pso
    }
}
