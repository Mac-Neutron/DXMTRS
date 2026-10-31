//! Argument tables: the bindings a shader reads, written where its reflection says.
//!
//! Each `encode_*` writes qwords at a byte offset of the pass's argument buffer, makes what it
//! points at resident, and records where the table is (a set-buffer-offset record at the
//! translator's binding index for the stage).

use core::ptr::NonNull;

use winemetal_sys::wire::{
    MTL_SM50_SHADER_ARGUMENT_BUFFER, MTL_SM50_SHADER_ARGUMENT_TBUFFER_OFFSET,
    MTL_SM50_SHADER_ARGUMENT_TEXTURE, MTL_SM50_SHADER_ARGUMENT_TEXTURE_ARRAY,
    MTL_SM50_SHADER_ARGUMENT_TEXTURE_MINLOD_CLAMP, MTL_SM50_SHADER_ARGUMENT_UAV_COUNTER,
    MtlSm50ShaderArgument, SM50_BINDING_INDEX_ARGUMENT_TABLE, SM50_BINDING_INDEX_ARGUMENT_TABLE2,
    SM50_BINDING_INDEX_CONSTANT_BUFFER, SM50_BINDING_INDEX_CONSTANT_BUFFER2,
    SM50_BINDING_INDEX_STREAM_OUTPUT0, SM50_BINDING_INDEX_VERTEX_BUFFER,
    WMT_COMPUTE_COMMAND_SET_BUFFER_OFFSET, WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER_OFFSET,
    WMT_RENDER_COMMAND_SET_MESH_BUFFER_OFFSET, WMT_RENDER_COMMAND_SET_OBJECT_BUFFER_OFFSET,
    WMT_RENDER_COMMAND_SET_VERTEX_BUFFER_OFFSET, WmtcmdComputeSetbufferoffset,
    WmtcmdRenderSetbufferoffset,
};

use super::{
    BINDING_SAMPLER, BINDING_SRV, BINDING_UAV, CONSTANT_BUFFER_SLOTS, Encoder, NO_NEXT,
    RESOURCE_SLOTS, ResourceView, SAMPLER_SLOTS, binding_index, kind, resident, stage,
};
use crate::deptrack::Access;

/// Where an argument's read and write bits sit in its flags (`MTL_SM50_SHADER_ARGUMENT_*_ACCESS`).
const ACCESS_SHIFT: u32 = 10;

impl Encoder {
    /// The vertex buffers `slots` (a mask) names, packed as 16-byte entries at `offset`.
    ///
    /// An entry is the GPU address, the stride and the length from the binding's offset; an
    /// unbound slot reads the zeroed dummy buffer with a stride and length of 0.
    pub fn encode_vertex_buffers<const K: u8>(&mut self, slots: u32, offset: u64) {
        let Some(table) = self.table(offset) else {
            return;
        };
        let bindings = core::mem::take(&mut self.vertex_buffers);
        let used = bindings.iter().take(32 - slots.leading_zeros() as usize);
        let mut entry = 0;
        for (slot, binding) in used.enumerate() {
            if slots & (1 << slot) == 0 {
                continue;
            }
            let bound = binding.buffer.as_ref().and_then(|buffer| {
                let length = buffer.length().saturating_sub(u64::from(binding.offset));
                self.access_buffer::<{ stage::VERTEX }>(buffer, Access::READ)
                    .map(|(allocation, suballocation)| (allocation, suballocation, length))
            });
            if let Some((allocation, suballocation, length)) = bound {
                let address = allocation.gpu_address() + suballocation;
                table.set(entry, address + u64::from(binding.offset));
                let length = u32::try_from(length).unwrap_or(u32::MAX);
                table.set(
                    entry + 1,
                    u64::from(binding.stride) | u64::from(length) << 32,
                );
                self.make_allocation_resident::<{ stage::VERTEX }, K>(&allocation, true, false);
            } else {
                table.set(entry, self.dummy_address());
                table.set(entry + 1, 0);
                self.use_dummy::<{ stage::VERTEX }, K>();
            }
            entry += 2;
        }
        self.vertex_buffers = bindings;
        self.set_buffer_offset::<{ stage::VERTEX }, K>(
            offset,
            SM50_BINDING_INDEX_VERTEX_BUFFER,
            SM50_BINDING_INDEX_VERTEX_BUFFER,
        );
    }

    /// The stream-output targets of an ordinary pipeline's vertex stage, in the table at `offset`.
    ///
    /// Each is an address and a counter's (0).
    /// The C++'s tessellation and geometry forms only log that they are not implemented, which
    /// the front end, whose log it is, does instead of calling this.
    pub fn encode_stream_output(&mut self, offset: u64) {
        let Some(table) = self.table(offset) else {
            return;
        };
        let bindings = core::mem::take(&mut self.stream_outputs);
        for (slot, binding) in bindings.iter().enumerate() {
            let bound = binding
                .buffer
                .as_ref()
                .and_then(|buffer| self.access_buffer::<{ stage::VERTEX }>(buffer, Access::WRITE));
            if let Some((allocation, suballocation)) = bound {
                let address = allocation.gpu_address() + suballocation;
                table.set(2 * slot, address + u64::from(binding.offset));
                self.make_allocation_resident::<{ stage::VERTEX }, { kind::ORDINARY }>(
                    &allocation,
                    false,
                    true,
                );
            } else {
                table.set(2 * slot, 0);
            }
            table.set(2 * slot + 1, 0);
        }
        self.stream_outputs = bindings;
        self.set_buffer_offset::<{ stage::VERTEX }, { kind::ORDINARY }>(
            offset,
            SM50_BINDING_INDEX_STREAM_OUTPUT0,
            SM50_BINDING_INDEX_STREAM_OUTPUT0,
        );
        if let Some(pass) = self.pass.as_mut() {
            pass.emulated_stream_output = true;
        }
    }

    /// Stage `S`'s constant buffers, in the table at `offset`.
    ///
    /// One GPU address each, where `arguments` (the reflection's constant-buffer list) put it.
    pub fn encode_constant_buffers<const S: u8, const K: u8>(
        &mut self,
        arguments: &[MtlSm50ShaderArgument],
        offset: u64,
    ) {
        let Some(table) = self.table(offset) else {
            return;
        };
        let bindings = core::mem::take(&mut self.constant_buffers);
        for arg in arguments {
            debug_assert!(arg.r#type == 0, "a constant buffer");
            let slot = CONSTANT_BUFFER_SLOTS * usize::from(S) + index(arg.sm50_binding_slot);
            let binding = bindings.get(slot);
            let bound = binding.and_then(|b| {
                let buffer = b.buffer.as_ref()?;
                let (allocation, suballocation) = self.access_buffer::<S>(buffer, Access::READ)?;
                Some((allocation, suballocation + u64::from(b.offset)))
            });
            let at = index(arg.structure_ptr_offset);
            if let Some((allocation, offset)) = bound {
                table.set(at, allocation.gpu_address() + offset);
                self.make_allocation_resident::<S, K>(&allocation, true, false);
            } else {
                table.set(at, self.dummy_address());
                self.use_dummy::<S, K>();
            }
        }
        self.constant_buffers = bindings;
        self.set_buffer_offset::<S, K>(
            offset,
            SM50_BINDING_INDEX_CONSTANT_BUFFER,
            SM50_BINDING_INDEX_CONSTANT_BUFFER2,
        );
    }

    /// Stage `S`'s samplers, shader resources and unordered-access views, in the table at `offset`.
    ///
    /// Each where `arguments` (the reflection's argument list) puts it.
    /// A sampler is three qwords (its state, its cube state, its LOD bias's bits); a buffer two
    /// (address, length); a texel buffer two (view, element count and first element); a texture
    /// two (view, array length); an unordered-access view with a counter a third, the counter's
    /// address. An unbound slot is 0s, or the dummy sampler.
    pub fn encode_shader_resources<const S: u8, const K: u8>(
        &mut self,
        arguments: &[MtlSm50ShaderArgument],
        offset: u64,
    ) {
        let Some(table) = self.table(offset) else {
            return;
        };
        let samplers = core::mem::take(&mut self.samplers);
        let resources = core::mem::take(&mut self.resources);
        let uavs = core::mem::take(if S == stage::COMPUTE {
            &mut self.compute_uavs
        } else {
            &mut self.pixel_uavs
        });
        for arg in arguments {
            let at = index(arg.structure_ptr_offset);
            let slot = index(arg.sm50_binding_slot);
            match arg.r#type {
                BINDING_SAMPLER => {
                    let bound = samplers
                        .get(SAMPLER_SLOTS * usize::from(S) + slot)
                        .and_then(Option::as_ref);
                    let (state, cube_state, lod_bias) = bound
                        .map_or((self.dummy_sampler_id, self.dummy_sampler_id, 0.0), |s| {
                            (s.state, s.cube_state, s.lod_bias)
                        });
                    table.set(at, state);
                    table.set(at + 1, cube_state);
                    table.set(at + 2, u64::from(lod_bias.to_bits()));
                }
                BINDING_SRV => {
                    let view = resources.get(RESOURCE_SLOTS * usize::from(S) + slot);
                    self.encode_view::<S, K>(&table, at, arg.flags, view, Access::READ);
                    debug_assert!(
                        arg.flags & MTL_SM50_SHADER_ARGUMENT_UAV_COUNTER == 0,
                        "a shader resource has no counter"
                    );
                }
                BINDING_UAV => {
                    let mut access = Access::UAV;
                    access.set(Access::READ, arg.flags >> ACCESS_SHIFT & 1 != 0);
                    access.set(Access::WRITE, arg.flags >> ACCESS_SHIFT & 2 != 0);
                    self.encode_view::<S, K>(&table, at, arg.flags, uavs.get(slot), access);
                    if arg.flags & MTL_SM50_SHADER_ARGUMENT_UAV_COUNTER != 0 {
                        // ponytail: the counter's address comes with the counter pool (the
                        // queries' lane); 0 is what the C++ writes for a view bound without one.
                        table.set(at + 2, 0);
                    }
                }
                _ => debug_assert!(false, "a constant buffer among the arguments"),
            }
        }
        self.samplers = samplers;
        self.resources = resources;
        if S == stage::COMPUTE {
            self.compute_uavs = uavs;
        } else {
            self.pixel_uavs = uavs;
        }
        self.set_buffer_offset::<S, K>(
            offset,
            SM50_BINDING_INDEX_ARGUMENT_TABLE,
            SM50_BINDING_INDEX_ARGUMENT_TABLE2,
        );
    }

    /// One shader resource or unordered-access view's two qwords at `at`, as `flags` declares it.
    fn encode_view<const S: u8, const K: u8>(
        &mut self,
        table: &Table,
        at: usize,
        flags: u32,
        view: Option<&ResourceView>,
        access: Access,
    ) {
        let (read, write) = (
            access.contains(Access::READ),
            access.contains(Access::WRITE),
        );
        let mut qwords = [0, 0];
        if flags & MTL_SM50_SHADER_ARGUMENT_BUFFER != 0 {
            if let Some(ResourceView::Buffer { buffer, slice, .. }) = view
                && let Some((allocation, suballocation)) = self.access_buffer::<S>(buffer, access)
            {
                let address = allocation.gpu_address() + suballocation;
                qwords = [
                    address + u64::from(slice.byte_offset),
                    u64::from(slice.byte_length),
                ];
                self.make_allocation_resident::<S, K>(&allocation, read, write);
            }
        } else if flags & MTL_SM50_SHADER_ARGUMENT_TEXTURE != 0 {
            match view {
                Some(ResourceView::Buffer { buffer, key, slice }) => {
                    debug_assert!(flags & MTL_SM50_SHADER_ARGUMENT_TBUFFER_OFFSET != 0);
                    if let Some((allocation, view, texel)) =
                        self.buffer_view::<S>(buffer, *key, access)
                    {
                        // The first element wraps as the C++'s 32-bit sum does.
                        let first = (u64::from(slice.first_element) + texel) & u64::from(u32::MAX);
                        qwords = [
                            view.gpu_resource_id,
                            u64::from(slice.element_count) << 32 | first,
                        ];
                        self.make_buffer_view_resident::<S, K>(
                            buffer,
                            &allocation,
                            *key,
                            read,
                            write,
                        );
                    }
                }
                Some(ResourceView::Texture { texture, key }) => {
                    debug_assert!(flags & MTL_SM50_SHADER_ARGUMENT_TEXTURE_MINLOD_CLAMP != 0);
                    let key = key.with_array(flags & MTL_SM50_SHADER_ARGUMENT_TEXTURE_ARRAY != 0);
                    if let Some((allocation, view)) = self.texture_view::<S>(texture, key, access) {
                        // The array length, and a minimum LOD of 0.0's bits.
                        qwords = [view.gpu_resource_id, u64::from(key.array_size()) << 32];
                        self.make_texture_view_resident::<S, K>(
                            &allocation,
                            key,
                            view.texture,
                            read,
                            write,
                        );
                    }
                }
                _ => {}
            }
        } else {
            return;
        }
        table.set(at, qwords[0]);
        table.set(at + 1, qwords[1]);
    }

    /// The pass's argument table from byte `offset` of its argument buffer.
    fn table(&self, offset: u64) -> Option<Table> {
        let Some(arguments) = self.pass.as_ref().and_then(|p| p.arguments.as_ref()) else {
            debug_assert!(
                false,
                "an argument table outside a pass with an argument buffer"
            );
            return None;
        };
        debug_assert!(
            offset.is_multiple_of(8),
            "argument tables are qword-aligned"
        );
        let skip = usize::try_from(offset / 8)
            .unwrap_or(usize::MAX)
            .min(arguments.qwords);
        // SAFETY: `skip` is at most the slice's qwords: still inside (or one past) the slice.
        let start = unsafe { arguments.start.add(skip) };
        Some(Table {
            start,
            qwords: arguments.qwords - skip,
        })
    }

    fn dummy_address(&self) -> u64 {
        self.dummy_cbuffer.as_ref().map_or(0, |a| a.gpu_address())
    }

    /// The dummy buffer made resident for stage `S`'s reads, as the C++ does at each unbound slot.
    fn use_dummy<const S: u8, const K: u8>(&mut self) {
        let buffer = self.dummy_cbuffer.as_ref().map_or(0, |a| a.buffer());
        self.use_resource::<S>(buffer, resident::mask::<S, K>(true, false));
    }

    /// Records where stage `S`'s table at `offset` is.
    ///
    /// At `index` of the stage's binding, or at `tessellation_index` for a tessellation pipeline's
    /// vertex stage, which shares an object function with the hull stage.
    fn set_buffer_offset<const S: u8, const K: u8>(
        &mut self,
        offset: u64,
        index: u32,
        tessellation_index: u32,
    ) {
        let Some(arguments) = self.pass.as_ref().and_then(|p| p.arguments.as_ref()) else {
            return;
        };
        let offset = arguments.offset + offset;
        if S == stage::COMPUTE {
            self.encode_compute_command(WmtcmdComputeSetbufferoffset {
                r#type: WMT_COMPUTE_COMMAND_SET_BUFFER_OFFSET,
                reserved: [0; 3],
                next: NO_NEXT,
                offset,
                index: binding_index(index),
            });
            return;
        }
        let (r#type, index) = match S {
            stage::VERTEX if K == kind::TESSELLATION => (
                WMT_RENDER_COMMAND_SET_OBJECT_BUFFER_OFFSET,
                tessellation_index,
            ),
            stage::VERTEX if K == kind::GEOMETRY => {
                (WMT_RENDER_COMMAND_SET_OBJECT_BUFFER_OFFSET, index)
            }
            stage::VERTEX => (WMT_RENDER_COMMAND_SET_VERTEX_BUFFER_OFFSET, index),
            stage::PIXEL => (WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER_OFFSET, index),
            stage::HULL => (WMT_RENDER_COMMAND_SET_OBJECT_BUFFER_OFFSET, index),
            _ => (WMT_RENDER_COMMAND_SET_MESH_BUFFER_OFFSET, index),
        };
        self.encode_render_command(WmtcmdRenderSetbufferoffset {
            r#type,
            reserved: [0; 3],
            next: NO_NEXT,
            offset,
            index: binding_index(index),
        });
    }
}

/// Qwords of an argument table, written through the pass's argument buffer.
struct Table {
    start: NonNull<u64>,
    qwords: usize,
}

impl Table {
    /// Writes qword `at`.
    ///
    /// # Panics
    ///
    /// Past the pass's argument buffer: a table the front end sized wrong.
    fn set(&self, at: usize, value: u64) {
        assert!(
            at < self.qwords,
            "an argument past the pass's argument buffer"
        );
        // SAFETY: `at` is inside the `qwords` qwords from `start` (checked above).
        let qword = unsafe { self.start.add(at) };
        // SAFETY: argument-buffer memory, aligned, that `ArgumentBuffer::new` gave to this pass
        // alone.
        unsafe { qword.write(value) };
    }
}

/// A reflection's slot or qword offset as an index.
fn index(v: u32) -> usize {
    usize::try_from(v).unwrap_or(usize::MAX)
}
