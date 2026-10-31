//! Emulated commands: clears Metal has no command for, and the indirect-draw argument marshals.
//!
//! The clears are compute dispatches of the internal library's kernels, recorded into the
//! encoder's compute pass; the marshals are draws, encoded into a live render encoder at the
//! flush, that turn a geometry or tessellation draw's arguments into the mesh draw's.
//!
//! Behaviour is the C++'s `EmulatedCommandContext` (`dxmt_command.hpp`, `dxmt_command.cpp`).
//! Written ahead for the switches of `d3d11.dll` and `d3d12.dll`, which are its first callers.

use core::{ffi::CStr, ptr::NonNull};

use winemetal_sys::{
    MTLDevice_newComputePipelineState, MTLRenderCommandEncoder_encodeCommands, Obj, Pool,
    wire::{
        MemPtr, WMT_COMPUTE_COMMAND_DISPATCH_THREADS, WMT_COMPUTE_COMMAND_SET_BUFFER,
        WMT_COMPUTE_COMMAND_SET_BYTES, WMT_COMPUTE_COMMAND_SET_PSO,
        WMT_COMPUTE_COMMAND_SET_TEXTURE, WMT_PRIMITIVE_TYPE_POINT, WMT_RENDER_COMMAND_DRAW,
        WMT_RENDER_COMMAND_SET_PSO, WMT_RENDER_COMMAND_SET_VERTEX_BUFFER, WMTComputePipelineInfo,
        WMTSize, WmtcmdBase, WmtcmdComputeDispatch, WmtcmdComputeSetbuffer, WmtcmdComputeSetbytes,
        WmtcmdComputeSetpso, WmtcmdComputeSettexture, WmtcmdRenderDraw, WmtcmdRenderSetbuffer,
        WmtcmdRenderSetpso,
    },
};

use crate::{
    encoder::{Encoder, NO_NEXT},
    library::{CUSTOM_BUFFER_ARGUMENT_INDEX_0, Library, render_pipeline, render_pipeline_info},
};

/// The compute kernels, by their index in [`EmulatedContext`]'s pipelines.
const KERNELS: [&CStr; 9] = [
    c"clear_texture_2d_uint",
    c"clear_texture_2d_array_uint",
    c"clear_texture_3d_uint",
    c"clear_texture_buffer_uint",
    c"clear_buffer_uint",
    c"clear_texture_2d_float",
    c"clear_texture_2d_array_float",
    c"clear_texture_3d_float",
    c"clear_texture_buffer_float",
];
const CLEAR_TEXTURE_2D_UINT: usize = 0;
const CLEAR_TEXTURE_2D_ARRAY_UINT: usize = 1;
const CLEAR_TEXTURE_3D_UINT: usize = 2;
const CLEAR_TEXTURE_BUFFER_UINT: usize = 3;
const CLEAR_BUFFER_UINT: usize = 4;
const CLEAR_TEXTURE_2D_FLOAT: usize = 5;
const CLEAR_TEXTURE_2D_ARRAY_FLOAT: usize = 6;
const CLEAR_TEXTURE_3D_FLOAT: usize = 7;
const CLEAR_TEXTURE_BUFFER_FLOAT: usize = 8;
/// A buffer or texel-buffer clear's threadgroup.
const LINEAR_GROUP: [u64; 3] = [32, 1, 1];
/// A texture clear's threadgroup.
const TILE_GROUP: [u64; 3] = [8, 4, 1];
/// The pipelines' handles in the tests, from here up by index.
#[cfg(test)]
const FAKE_PIPELINE: u64 = 0xe0_0000;

/// The clear kernels' pipelines and the two argument marshals' render pipelines.
pub struct EmulatedContext {
    compute: [Option<Obj>; KERNELS.len()],
    gs_marshal: Option<Obj>,
    ts_marshal: Option<Obj>,
}

impl EmulatedContext {
    /// The pipelines of `library`'s kernels on `device`.
    ///
    /// A pipeline Metal cannot make is nil, as the C++'s.
    #[must_use]
    pub fn new(device: &Obj, library: &Library) -> Self {
        let _pool = Pool::new();
        let marshal = |name: &CStr| {
            let function = library.function(name);
            let mut info = render_pipeline_info();
            info.vertex_function = function.as_ref().map_or(0, Obj::handle);
            info.rasterization_enabled = 0;
            render_pipeline(device, &info)
        };
        Self {
            compute: KERNELS.map(|name| compute_pipeline(device, library, name)),
            gs_marshal: marshal(c"gs_draw_arguments_marshal"),
            ts_marshal: marshal(c"ts_draw_arguments_marshal"),
        }
    }

    /// Writes `value` to `elements` `u32`s of `buffer` from `byte_offset`.
    pub fn clear_buffer_uint(
        &self,
        encoder: &mut Encoder,
        buffer: u64,
        byte_offset: u64,
        elements: u64,
        value: [u32; 4],
    ) {
        self.set_pso(encoder, CLEAR_BUFFER_UINT, LINEAR_GROUP);
        set_buffer(encoder, buffer, byte_offset, 0);
        set_bytes(encoder, &words(value), 1);
        // The element count's low 4 bytes, as the C++ passes its `uint64_t`.
        set_bytes(encoder, &elements.to_le_bytes()[..4], 2);
        dispatch(encoder, [elements, 1, 1]);
    }

    /// Writes `value`'s bits to `elements` `u32`s of `buffer` from `byte_offset`.
    pub fn clear_buffer_float(
        &self,
        encoder: &mut Encoder,
        buffer: u64,
        byte_offset: u64,
        elements: u64,
        value: [f32; 4],
    ) {
        self.clear_buffer_uint(
            encoder,
            buffer,
            byte_offset,
            elements,
            value.map(f32::to_bits),
        );
    }

    /// Writes `value` to `size` texels of the texel buffer `texture` from `offset`.
    pub fn clear_texture_buffer_uint(
        &self,
        encoder: &mut Encoder,
        texture: u64,
        offset: u32,
        size: u32,
        value: [u32; 4],
    ) {
        self.clear_texel_buffer(
            encoder,
            CLEAR_TEXTURE_BUFFER_UINT,
            texture,
            [offset, size],
            value,
        );
    }

    /// Writes `value` to `size` texels of the texel buffer `texture` from `offset`.
    pub fn clear_texture_buffer_float(
        &self,
        encoder: &mut Encoder,
        texture: u64,
        offset: u32,
        size: u32,
        value: [f32; 4],
    ) {
        let value = value.map(f32::to_bits);
        self.clear_texel_buffer(
            encoder,
            CLEAR_TEXTURE_BUFFER_FLOAT,
            texture,
            [offset, size],
            value,
        );
    }

    /// Clears the 2D `texture` of `[width, height]` to `value`.
    pub fn clear_texture_2d_uint(
        &self,
        encoder: &mut Encoder,
        texture: u64,
        [width, height]: [u64; 2],
        value: [u32; 4],
    ) {
        self.clear_texture(
            encoder,
            CLEAR_TEXTURE_2D_UINT,
            texture,
            [width, height, 1],
            value,
        );
    }

    /// Clears the 2D `texture` of `[width, height]` to `value`.
    pub fn clear_texture_2d_float(
        &self,
        encoder: &mut Encoder,
        texture: u64,
        [width, height]: [u64; 2],
        value: [f32; 4],
    ) {
        let value = value.map(f32::to_bits);
        self.clear_texture(
            encoder,
            CLEAR_TEXTURE_2D_FLOAT,
            texture,
            [width, height, 1],
            value,
        );
    }

    /// Clears the 2D array `texture` of `[width, height, slices]` to `value`.
    pub fn clear_texture_2d_array_uint(
        &self,
        encoder: &mut Encoder,
        texture: u64,
        size: [u64; 3],
        value: [u32; 4],
    ) {
        self.clear_texture(encoder, CLEAR_TEXTURE_2D_ARRAY_UINT, texture, size, value);
    }

    /// Clears the 2D array `texture` of `[width, height, slices]` to `value`.
    pub fn clear_texture_2d_array_float(
        &self,
        encoder: &mut Encoder,
        texture: u64,
        size: [u64; 3],
        value: [f32; 4],
    ) {
        let value = value.map(f32::to_bits);
        self.clear_texture(encoder, CLEAR_TEXTURE_2D_ARRAY_FLOAT, texture, size, value);
    }

    /// Clears the 3D `texture` of `[width, height, depth]` to `value`.
    pub fn clear_texture_3d_uint(
        &self,
        encoder: &mut Encoder,
        texture: u64,
        size: [u64; 3],
        value: [u32; 4],
    ) {
        self.clear_texture(encoder, CLEAR_TEXTURE_3D_UINT, texture, size, value);
    }

    /// Clears the 3D `texture` of `[width, height, depth]` to `value`.
    pub fn clear_texture_3d_float(
        &self,
        encoder: &mut Encoder,
        texture: u64,
        size: [u64; 3],
        value: [f32; 4],
    ) {
        let value = value.map(f32::to_bits);
        self.clear_texture(encoder, CLEAR_TEXTURE_3D_FLOAT, texture, size, value);
    }

    /// Turns the geometry draw's arguments at `offset` of `commands` into its mesh draw's.
    ///
    /// `encoder` is the live render encoder of the pass being encoded.
    pub fn marshal_gs(&self, encoder: u64, commands: u64, offset: u32) {
        marshal(handle(self.gs_marshal.as_ref()), encoder, commands, offset);
    }

    /// Turns the tessellation draw's arguments at `offset` of `commands` into its mesh draw's.
    pub fn marshal_ts(&self, encoder: u64, commands: u64, offset: u32) {
        marshal(handle(self.ts_marshal.as_ref()), encoder, commands, offset);
    }

    /// A context whose pipelines are the handles from [`FAKE_PIPELINE`] up.
    #[cfg(test)]
    fn fake() -> Self {
        let fake = |i: usize| {
            // SAFETY: a handle no object has; dropping it calls the tests' release fake.
            unsafe { Obj::adopt(FAKE_PIPELINE + i as u64) }
        };
        Self {
            compute: core::array::from_fn(fake),
            gs_marshal: None,
            ts_marshal: None,
        }
    }

    fn set_pso(&self, encoder: &mut Encoder, kernel: usize, [width, height, depth]: [u64; 3]) {
        encoder.encode_compute_command(WmtcmdComputeSetpso {
            r#type: WMT_COMPUTE_COMMAND_SET_PSO,
            reserved: [0; 3],
            next: NO_NEXT,
            pso: handle(self.compute[kernel].as_ref()),
            threadgroup_size: WMTSize {
                width,
                height,
                depth,
            },
        });
    }

    /// A texel-buffer clear: `value`, `[offset, size]` and 8 bytes of padding as argument 1.
    fn clear_texel_buffer(
        &self,
        encoder: &mut Encoder,
        kernel: usize,
        texture: u64,
        [offset, size]: [u32; 2],
        value: [u32; 4],
    ) {
        self.set_pso(encoder, kernel, LINEAR_GROUP);
        set_texture(encoder, texture, 0);
        let [a, b, c, d] = value;
        set_bytes(encoder, &words([a, b, c, d, offset, size, 0, 0]), 1);
        dispatch(encoder, [u64::from(size), 1, 1]);
    }

    fn clear_texture(
        &self,
        encoder: &mut Encoder,
        kernel: usize,
        texture: u64,
        size: [u64; 3],
        value: [u32; 4],
    ) {
        self.set_pso(encoder, kernel, TILE_GROUP);
        set_texture(encoder, texture, 0);
        set_bytes(encoder, &words(value), 1);
        dispatch(encoder, size);
    }
}

/// The compute pipeline of `library`'s kernel `name` on `device`, or `None`.
fn compute_pipeline(device: &Obj, library: &Library, name: &CStr) -> Option<Obj> {
    let function = library.function(name);
    let info = WMTComputePipelineInfo {
        compute_function: function.as_ref().map_or(0, Obj::handle),
        binary_archives_for_lookup: NO_NEXT,
        binary_archive_for_serialization: 0,
        num_binary_archives_for_lookup: 0,
        fail_on_binary_archive_miss: 0,
        support_indirect_command_buffers: 0,
        tgsize_is_multiple_of_sgwidth: 0,
        immutable_buffers: 0,
    };
    let mut error = 0;
    // SAFETY: a live device; `info` and `error` are valid for the call.
    let pso = unsafe {
        MTLDevice_newComputePipelineState(device.handle(), &raw const info, &raw mut error)
    };
    // SAFETY: a new error's handle carries its reference, which is dropped here unread.
    drop(unsafe { Obj::adopt(error) });
    // SAFETY: a new pipeline's handle carries its reference.
    unsafe { Obj::adopt(pso) }
}

fn handle(object: Option<&Obj>) -> u64 {
    object.map_or(0, Obj::handle)
}

fn set_buffer(encoder: &mut Encoder, buffer: u64, offset: u64, index: u8) {
    encoder.encode_compute_command(WmtcmdComputeSetbuffer {
        r#type: WMT_COMPUTE_COMMAND_SET_BUFFER,
        reserved: [0; 3],
        next: NO_NEXT,
        buffer,
        offset,
        index,
    });
}

fn set_texture(encoder: &mut Encoder, texture: u64, index: u8) {
    encoder.encode_compute_command(WmtcmdComputeSettexture {
        r#type: WMT_COMPUTE_COMMAND_SET_TEXTURE,
        reserved: [0; 3],
        next: NO_NEXT,
        texture,
        index,
    });
}

/// Binds a copy of `bytes`, which lives in the encoder's memory until the chunk is encoded.
fn set_bytes(encoder: &mut Encoder, bytes: &[u8], index: u8) {
    let copy = encoder.allocate_cpu(bytes.len(), 8);
    // SAFETY: fresh memory of `bytes.len()` bytes, which `bytes` does not overlap.
    unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), copy.as_ptr(), bytes.len()) };
    encoder.encode_compute_command(WmtcmdComputeSetbytes {
        r#type: WMT_COMPUTE_COMMAND_SET_BYTES,
        reserved: [0; 3],
        next: NO_NEXT,
        bytes: mem_ptr(copy.as_ptr().expose_provenance()),
        length: bytes.len() as u64,
        index,
    });
}

fn dispatch(encoder: &mut Encoder, [width, height, depth]: [u64; 3]) {
    encoder.encode_compute_command(WmtcmdComputeDispatch {
        r#type: WMT_COMPUTE_COMMAND_DISPATCH_THREADS,
        reserved: [0; 3],
        next: NO_NEXT,
        size: WMTSize {
            width,
            height,
            depth,
        },
    });
}

/// `words` as the bytes a shader argument holds.
fn words<const N: usize>(words: [u32; N]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_ne_bytes()).collect()
}

/// One point drawn by the marshal pipeline `pso`, its arguments at `offset` of `commands`.
///
/// The C++'s four calls on the render encoder as one list of four records: the pipeline, the
/// arguments at the internal argument index, the draw, and that index unbound again.
fn marshal(pso: u64, encoder: u64, commands: u64, offset: u32) {
    let index = u8::try_from(CUSTOM_BUFFER_ARGUMENT_INDEX_0).unwrap_or(u8::MAX);
    let unbind = WmtcmdRenderSetbuffer {
        r#type: WMT_RENDER_COMMAND_SET_VERTEX_BUFFER,
        reserved: [0; 3],
        next: NO_NEXT,
        buffer: 0,
        offset: 0,
        index,
    };
    let draw = WmtcmdRenderDraw {
        r#type: WMT_RENDER_COMMAND_DRAW,
        reserved: [0; 3],
        next: link(&unbind),
        primitive_type: WMT_PRIMITIVE_TYPE_POINT,
        vertex_start: 0,
        vertex_count: 1,
        instance_count: 1,
        base_instance: 0,
    };
    let bind = WmtcmdRenderSetbuffer {
        r#type: WMT_RENDER_COMMAND_SET_VERTEX_BUFFER,
        reserved: [0; 3],
        next: link(&draw),
        buffer: commands,
        offset: u64::from(offset),
        index,
    };
    let set_pso = WmtcmdRenderSetpso {
        r#type: WMT_RENDER_COMMAND_SET_PSO,
        reserved: [0; 3],
        next: link(&bind),
        pso,
    };
    let head = NonNull::from(&set_pso).cast::<WmtcmdBase>();
    // SAFETY: the pass's live render encoder; the four records, linked through `next`, live on
    // this stack frame for the call.
    unsafe { MTLRenderCommandEncoder_encodeCommands(encoder, head.as_ptr()) };
}

/// `record`'s address as a `next`.
fn link<T>(record: &T) -> MemPtr {
    mem_ptr(core::ptr::from_ref(record).expose_provenance())
}

/// `addr` as a pointer of `winemetal.h`'s structures.
#[cfg(target_pointer_width = "64")]
const fn mem_ptr(addr: usize) -> MemPtr {
    MemPtr { ptr: addr as u64 }
}

/// `addr` as a pointer of `winemetal.h`'s structures: a 32-bit address and a high half of 0.
#[cfg(target_pointer_width = "32")]
fn mem_ptr(addr: usize) -> MemPtr {
    MemPtr {
        ptr: u32::try_from(addr).unwrap_or(0),
        high: 0,
    }
}

#[cfg(test)]
mod tests;
