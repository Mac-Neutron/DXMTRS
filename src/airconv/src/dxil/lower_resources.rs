//! DXIL's resource operations lowered to AIR.
//!
//! Ported from `A/dxil/dxil_lower_resources.cpp`: constant buffers, raw, structured and typed
//! buffers, textures (load, store, sample, gather, LOD, dimensions, atomics) and the 64-bit
//! atomics' lock path, through the compile's binding map. GPU efficiency spec E5: a raw or
//! structured buffer load is bounds-checked once for all its components (Metal Shader
//! Converter's form: one that straddles its view's end reads zeros); `DXMT_DXIL_BOUNDS`
//! ([`super::bounds`]) checks each component, or nothing.

use airconv_ir::binding::TextureKind as Kind;

use super::{
    bounds,
    lower::{Handle, Lowering, const_arg, primitive_bits, unsupported},
    types::{op, resource_class, resource_kind, shader_kind},
};
use crate::{
    air::builder::{
        FpBinOp, FpUnOp, Gradient, IntBinOp, Lod, MemFlags, MemoryAccess, Query, SampleAt, TexelAt,
        Texture, ThreadScope,
    },
    api::Error,
    dxbc::converter::{BufferDescriptor, TextureDescriptor, air_kind, req},
    llvm::{TyKind, Val, sys},
};

/// The device's 64-bit atomics lock table size in `u32` locks (`kAtomic64Locks`).
///
/// `airconv_public.h:42`, C++ only; no mirror in `abi.rs` yet (lane l's request, X2).
const ATOMIC64_LOCKS: u32 = 1 << 16;
/// The device address space.
const DEVICE: u32 = 1;

impl<'cx> Lowering<'_, 'cx> {
    /// The resource operations (`LowerResource`).
    ///
    /// # Errors
    ///
    /// `ResolveHandle`'s refusals, the op's refusal, the C++'s other `DXIL:` texts and what the
    /// binding map refuses.
    pub fn lower_resource(&self, opcode: u32, call: Val<'cx>) -> Result<Option<Val<'cx>>, Error> {
        let b = self.b();
        let air = self.air();
        let cx = self.cx();
        let tcx = self.tcx();
        let arg = |i| self.arg(call, i);
        let i32_ty = tcx.int(32);
        if opcode == op::CHECK_ACCESS_FULLY_MAPPED {
            // (71, i32 status): every resource is fully mapped here
            return Ok(Some(b.icmp(sys::INT_NE, arg(1)?, self.int(0))));
        }
        let view = self.handle(call, 1)?;
        let kind = view.resource.kind;
        let structured = kind == resource_kind::STRUCTURED_BUFFER;
        let stride = self.int(view.resource.stride);
        // A structured buffer's element `index`'s byte `offset`, a raw buffer's byte `index`.
        let byte_offset = |index: u32| -> Result<Val<'cx>, Error> {
            Ok(if structured {
                b.add(b.mul(arg(index)?, stride), arg(index + 1)?)
            } else {
                arg(index)?
            })
        };
        let conv = self.conv;
        match opcode {
            // (59, handle, i32 row) -> %dx.types.CBufRet.T
            op::C_BUFFER_LOAD_LEGACY => {
                let ty = call.ty();
                let Some(d) =
                    conv.binding
                        .constant_buffer(air, conv.function, view.range, view.index)?
                else {
                    return Ok(Some(cx.null(ty)));
                };
                let row_ty = air.int_ty(4);
                let row = b.load(row_ty, b.gep(row_ty, d.pointer, &[arg(2)?]));
                let elem = tcx.field(ty, 0).ok_or_else(|| unsupported(call))?;
                let count = tcx.fields(ty).map_or(0, |f| f.len());
                let bits = primitive_bits(tcx, elem);
                let mut fields = Vec::with_capacity(count);
                for e in 0..u64::try_from(count).unwrap_or(0) {
                    let v = match bits {
                        32 => b.extract_element_u64(row, e),
                        16 => {
                            let word = b.extract_element_u64(row, e / 2);
                            let shift = self.int(if e % 2 == 0 { 0 } else { 16 });
                            b.trunc(b.lshr(word, shift), tcx.int(16))
                        }
                        // 64
                        _ => {
                            let i64_ty = tcx.int(64);
                            let lo = b.zext(b.extract_element_u64(row, e * 2), i64_ty);
                            let hi = b.zext(b.extract_element_u64(row, e * 2 + 1), i64_ty);
                            b.or(lo, b.shl(hi, cx.const_int(i64_ty, 32, false)))
                        }
                    };
                    fields.push(Some(b.bitcast(v, elem)));
                }
                Ok(Some(self.aggregate(ty, &fields)))
            }
            // (58, handle, i32 byteOffset, i32 alignment) -> T
            op::C_BUFFER_LOAD => {
                let ty = call.ty();
                let Some(d) =
                    conv.binding
                        .constant_buffer(air, conv.function, view.range, view.index)?
                else {
                    return Ok(Some(cx.null(ty)));
                };
                let offset = arg(2)?;
                let row = b.lshr(offset, self.int(4));
                let column = b.and(b.lshr(offset, self.int(2)), self.int(3));
                let word_ptr = b.gep(air.int_ty(4), d.pointer, &[row, column]);
                let mut word = b.load(i32_ty, word_ptr);
                match primitive_bits(tcx, ty) {
                    16 => {
                        let shift = b.shl(b.and(offset, self.int(2)), self.int(3));
                        word = b.trunc(b.lshr(word, shift), tcx.int(16));
                    }
                    64 => return Err(Error::new(b"DXIL: 64-bit cbufferLoad not supported")),
                    _ => {}
                }
                Ok(Some(b.bitcast(word, ty)))
            }
            // (139, handle, i32 index, i32 elementOffset, i8 mask, i32 alignment)
            op::RAW_BUFFER_LOAD => {
                let offset = byte_offset(2)?;
                self.buffer_load(call, &view, offset, const_arg(call, 4, 0xf))
            }
            // (140, handle, i32 index, i32 elementOffset, v0..v3, i8 mask, i32 alignment)
            op::RAW_BUFFER_STORE => {
                let offset = byte_offset(2)?;
                self.buffer_store(call, &view, offset, 4, const_arg(call, 8, 0xf))
            }
            // (68, handle, i32 index, i32 offset)
            op::BUFFER_LOAD => {
                if kind == resource_kind::TYPED_BUFFER {
                    return self.typed_buffer_load(call, &view, arg(2)?);
                }
                let offset = byte_offset(2)?;
                self.buffer_load(call, &view, offset, extracted_components(call))
            }
            // (69, handle, i32 index, i32 offset, v0..v3, i8 mask)
            op::BUFFER_STORE => {
                if kind == resource_kind::TYPED_BUFFER {
                    return self.typed_buffer_store(call, &view, arg(2)?);
                }
                let offset = byte_offset(2)?;
                self.buffer_store(call, &view, offset, 4, const_arg(call, 8, 0xf))
            }
            // (72, handle, i32 mipLevel)
            op::GET_DIMENSIONS => self.dimensions(call, &view),
            // (78, handle, i32 op, i32 c0, c1, c2, i32 value) -> old,
            // (79, handle, i32 c0, c1, c2, i32 compare, i32 value) -> old
            op::ATOMIC_BIN_OP | op::ATOMIC_COMPARE_EXCHANGE => {
                match tcx.kind(call.ty()) {
                    TyKind::Int(32) => {}
                    TyKind::Int(64) => return self.atomic64(opcode, call, &view),
                    _ => return Err(unsupported(call)),
                }
                if kind != resource_kind::RAW_BUFFER && !structured {
                    // Typed buffers and textures.
                    return self.texture_op(opcode, call, &view);
                }
                let cas = opcode == op::ATOMIC_COMPARE_EXCHANGE;
                let offset = byte_offset(if cas { 2 } else { 3 })?;
                let Some(buffer) = self.buffer(&view)? else {
                    return Ok(Some(self.int(0)));
                };
                // Null out of bounds, as loads and stores.
                let ptr = self.element_pointer(&buffer, offset, i32_ty);
                let old = if cas {
                    let relaxed = sys::ATOMIC_ORDERING_MONOTONIC;
                    let pair = b.atomic_cmpxchg(ptr, arg(5)?, arg(6)?, relaxed, relaxed, false);
                    b.extract_value(pair, 0)
                } else {
                    air.create_atomic_rmw(atomic_op(const_arg(call, 2, 0)), ptr, arg(6)?)?
                };
                Ok(Some(old))
            }
            op::TEXTURE_LOAD
            | op::TEXTURE_STORE
            | op::SAMPLE
            | op::SAMPLE_BIAS
            | op::SAMPLE_LEVEL
            | op::SAMPLE_GRAD
            | op::SAMPLE_CMP
            | op::SAMPLE_CMP_LEVEL_ZERO
            | op::TEXTURE_GATHER
            | op::TEXTURE_GATHER_CMP
            | op::CALCULATE_LOD
            | op::SAMPLE_CMP_GRAD
            | op::SAMPLE_CMP_BIAS => self.texture_op(opcode, call, &view),
            // (75, srv, i32 index) -> {f32, f32}: D3D's standard positions (`air_samplepos`)
            op::TEXTURE_2D_MS_GET_SAMPLE_POSITION => {
                // No texture: (0, 0), as the DXBC samplepos lowering.
                let count = match self.texture(&view)? {
                    Some(d) => {
                        let (tex, handle, _, _, _) = parts(d);
                        air.create_texture_query(
                            &tex,
                            handle,
                            Query::NumSamples,
                            Some(self.int(0)),
                        )?
                    }
                    None => self.int(0),
                };
                let pos = conv.sample_pos(count, arg(2)?)?;
                let value = self.aggregate(
                    call.ty(),
                    &[
                        Some(b.extract_element_u64(pos, 0)),
                        Some(b.extract_element_u64(pos, 1)),
                    ],
                );
                Ok(Some(value))
            }
            _ => Err(unsupported(call)),
        }
    }

    /// The buffer view of a handle (`Buffer`), `None` when unbound.
    fn buffer(&self, view: &Handle<'_, 'cx>) -> Result<Option<BufferDescriptor<'cx>>, Error> {
        let conv = self.conv;
        if view.cls == resource_class::UAV {
            conv.binding
                .uav_buffer(conv.air, conv.function, view.range, view.index)
        } else {
            conv.binding
                .srv_buffer(conv.air, conv.function, view.range, view.index)
        }
    }

    /// The texture view of a handle (`Texture`), `None` when unbound.
    fn texture(&self, view: &Handle<'_, 'cx>) -> Result<Option<TextureDescriptor<'cx>>, Error> {
        let conv = self.conv;
        if view.cls == resource_class::UAV {
            conv.binding
                .uav_texture(conv.air, conv.function, view.range, view.index)
        } else {
            conv.binding
                .srv_texture(conv.air, conv.function, view.range, view.index)
        }
    }

    /// A pointer to `count` consecutive `ty` at `byte_offset`, null unless all are in bounds.
    ///
    /// `AccessPointer` (E5); one `ty` is `ElementPointer`. Unchecked without metadata (a root
    /// descriptor) or with `DXMT_DXIL_BOUNDS=off`.
    fn access_pointer(
        &self,
        buffer: &BufferDescriptor<'cx>,
        byte_offset: Val<'cx>,
        ty: crate::llvm::Ty,
        count: u32,
    ) -> Val<'cx> {
        let b = self.b();
        let tcx = self.tcx();
        let space = tcx.addrspace(buffer.pointer.ty()).unwrap_or(0);
        let bytes = b.bitcast(buffer.pointer, tcx.ptr(tcx.int(8), space));
        let ptr = b.bitcast(b.gep(tcx.int(8), bytes, &[byte_offset]), tcx.ptr(ty, space));
        let Some(metadata) = buffer.metadata else {
            return ptr;
        };
        if bounds::off() {
            return ptr;
        }
        let size = count.wrapping_mul(primitive_bits(tcx, ty) / 8);
        let in_bounds = self.in_bounds(metadata, byte_offset, size);
        b.select(in_bounds, ptr, self.cx().null(ptr.ty()))
    }

    /// `ElementPointer`: a bound-checked pointer to one `ty`, null out of bounds.
    fn element_pointer(
        &self,
        buffer: &BufferDescriptor<'cx>,
        byte_offset: Val<'cx>,
        ty: crate::llvm::Ty,
    ) -> Val<'cx> {
        self.access_pointer(buffer, byte_offset, ty, 1)
    }

    /// `bytes` from `byte_offset` lie inside the buffer's view, without wrapping (`InBounds`).
    ///
    /// An offset near 4 GiB (a -1 index, say) must not pass the check. In 32 bits (offset <=
    /// length - bytes, length >= bytes): Apple GPUs have no 64-bit integer ALU, and a 64-bit form
    /// cost SMITE 2 0.5 ms of vertex time per frame.
    fn in_bounds(&self, metadata: Val<'cx>, byte_offset: Val<'cx>, bytes: u32) -> Val<'cx> {
        let b = self.b();
        let length = self.decode_low(metadata);
        let n = self.int(bytes);
        let fits = b.icmp(sys::INT_UGE, length, n);
        let inside = b.icmp(sys::INT_ULE, byte_offset, b.sub(length, n));
        b.and(fits, inside)
    }

    /// `LoadElement`: one bound-checked element, coherent where the view is.
    fn load_element(
        &self,
        buffer: &BufferDescriptor<'cx>,
        byte_offset: Val<'cx>,
        ty: crate::llvm::Ty,
    ) -> Result<Val<'cx>, Error> {
        let ptr = self.element_pointer(buffer, byte_offset, ty);
        if buffer.global_coherent {
            self.air().create_device_coherent_load(ty, ptr)
        } else {
            Ok(self.b().load(ty, ptr))
        }
    }

    /// `StoreElement`: one bound-checked element, coherent where the view is.
    fn store_element(
        &self,
        buffer: &BufferDescriptor<'cx>,
        byte_offset: Val<'cx>,
        value: Val<'cx>,
    ) -> Result<(), Error> {
        let ptr = self.element_pointer(buffer, byte_offset, value.ty());
        if buffer.global_coherent {
            self.air().create_device_coherent_store(value, ptr)?;
        } else {
            self.b().store(value, ptr);
        }
        Ok(())
    }

    /// `%dx.types.ResRet.T = {T, T, T, T, i32 status}`; components outside `mask` stay undef.
    ///
    /// `LowerBufferLoad`.
    fn buffer_load(
        &self,
        call: Val<'cx>,
        view: &Handle<'_, 'cx>,
        byte_offset: Val<'cx>,
        mask: u32,
    ) -> Result<Option<Val<'cx>>, Error> {
        let b = self.b();
        let tcx = self.tcx();
        let buffer = self.buffer(view)?;
        let ty = call.ty();
        let elem = tcx.field(ty, 0).ok_or_else(|| unsupported(call))?;
        let Some(buffer) = buffer else {
            return Ok(Some(self.cx().null(ty)));
        };
        let mut fields = [None; 5];
        let size = primitive_bits(tcx, elem) / 8;
        if bounds::per_component() {
            for (c, field) in (0..4_u32).zip(&mut fields) {
                if mask & (1 << c) != 0 {
                    let offset = b.add(byte_offset, self.int(c.wrapping_mul(size)));
                    *field = Some(self.load_element(&buffer, offset, elem)?);
                }
            }
        } else {
            // One check for every component loaded: all of them or none (E5).
            let count = 32 - mask.leading_zeros();
            let base = self.access_pointer(&buffer, byte_offset, elem, count);
            for (c, field) in (0..4_u32).zip(&mut fields) {
                if mask & (1 << c) != 0 {
                    let ptr = b.const_gep1_32(elem, base, c);
                    *field = Some(if buffer.global_coherent {
                        self.air().create_device_coherent_load(elem, ptr)?
                    } else {
                        b.load(elem, ptr)
                    });
                }
            }
        }
        // Status: fully mapped.
        fields[4] = Some(self.int(1));
        Ok(Some(self.aggregate(ty, &fields)))
    }

    /// The `mask` values from argument `first_value` stored, each bound-checked.
    ///
    /// `LowerBufferStore`.
    fn buffer_store(
        &self,
        call: Val<'cx>,
        view: &Handle<'_, 'cx>,
        byte_offset: Val<'cx>,
        first_value: u32,
        mask: u32,
    ) -> Result<Option<Val<'cx>>, Error> {
        if let Some(buffer) = self.buffer(view)? {
            let elem = self.arg(call, first_value)?.ty();
            let size = primitive_bits(self.tcx(), elem) / 8;
            for c in 0..4_u32 {
                if mask & (1 << c) != 0 {
                    let offset = self.b().add(byte_offset, self.int(c.wrapping_mul(size)));
                    self.store_element(&buffer, offset, self.arg(call, first_value + c)?)?;
                }
            }
        }
        Ok(None)
    }

    /// A typed buffer read (`LowerTypedBufferLoad`).
    ///
    /// Typed buffers are Metal texture buffers; the descriptor's metadata holds the view's first
    /// element and element count (as the DXBC `ld`/`store_uav_typed` lowering reads it).
    fn typed_buffer_load(
        &self,
        call: Val<'cx>,
        view: &Handle<'_, 'cx>,
        index: Val<'cx>,
    ) -> Result<Option<Val<'cx>>, Error> {
        let b = self.b();
        let air = self.air();
        let ty = call.ty();
        let Some(d) = self.texture(view)? else {
            return Ok(Some(self.cx().null(ty)));
        };
        let (tex, handle, metadata, coherent, _) = parts(d);
        let mut address = b.add(index, self.decode_low(metadata));
        if !bounds::off() {
            let oob = b.icmp(sys::INT_UGE, index, self.decode_high(metadata));
            address = b.select(oob, self.cx().all_ones(address.ty()), address);
        }
        if matches!(tex.memory_access, MemoryAccess::ReadWrite) {
            // As the DXBC typed UAV load: see earlier writes.
            air.create_texture_fence(&tex, handle)?;
        }
        let at = TexelAt {
            pos: address,
            array_index: None,
            sample_or_face: None,
            level: Some(self.int(0)),
        };
        let (texel, _) = air.create_read(&tex, handle, &at, coherent)?;
        let elem = self.tcx().field(ty, 0).ok_or_else(|| unsupported(call))?;
        let mut fields = [None; 5];
        for (c, field) in (0..4_u64).zip(&mut fields) {
            *field = Some(self.cast(b.extract_element_u64(texel, c), elem));
        }
        fields[4] = Some(self.int(1));
        Ok(Some(self.aggregate(ty, &fields)))
    }

    /// A typed buffer write, the values at arguments 4..7 (`LowerTypedBufferStore`).
    fn typed_buffer_store(
        &self,
        call: Val<'cx>,
        view: &Handle<'_, 'cx>,
        index: Val<'cx>,
    ) -> Result<Option<Val<'cx>>, Error> {
        let b = self.b();
        let air = self.air();
        if let Some(d) = self.texture(view)? {
            let (tex, handle, metadata, coherent, _) = parts(d);
            let texel_ty = air.texel_type(&tex);
            let element = self
                .tcx()
                .element(texel_ty)
                .ok_or_else(|| unsupported(call))?;
            let mut texel = self.cx().undef(texel_ty);
            for c in 0..4_u32 {
                let value = self.cast(self.arg(call, 4 + c)?, element);
                texel = b.insert_element_u64(texel, value, c.into());
            }
            let address = b.add(index, self.decode_low(metadata));
            let at = TexelAt {
                pos: address,
                array_index: None,
                sample_or_face: None,
                level: Some(self.int(0)),
            };
            air.create_write(&tex, handle, &at, texel, coherent)?;
        }
        Ok(None)
    }

    /// Texture loads, stores, atomics, samples, gathers and LODs (`LowerTexture`).
    ///
    /// Coordinates, array slice and sampler choice for a texture kind, as airconv's DXBC sample
    /// lowering builds them; DXIL's c0..c3 are undef where unused.
    fn texture_op(
        &self,
        opcode: u32,
        call: Val<'cx>,
        view: &Handle<'_, 'cx>,
    ) -> Result<Option<Val<'cx>>, Error> {
        let b = self.b();
        let air = self.air();
        let cx = self.cx();
        let tcx = self.tcx();
        let arg = |i| self.arg(call, i);
        let ty = call.ty();
        let Some(d) = self.texture(view)? else {
            let value = (tcx.kind(ty) != TyKind::Void).then(|| cx.null(ty));
            return Ok(value);
        };
        let (tex, handle, metadata, coherent, logical) = parts(d);
        let dims = dimensions(logical);
        let one_d = matches!(logical, Kind::Texture1d | Kind::Texture1dArray);
        let i32_ty = tcx.int(32);

        // Float coordinates from c0.. (sample/gather/lod) or integer positions (load/store).
        let vector = |first: u32, elem| -> Result<Val<'cx>, Error> {
            let mut v = cx.undef(tcx.vector(elem, dims));
            for i in 0..dims {
                let component = if one_d && i == 1 {
                    cx.null(elem)
                } else {
                    arg(first + i)?
                };
                v = b.insert_element_u64(v, component, i.into());
            }
            Ok(v)
        };
        let array_index = |first: u32| -> Result<Option<Val<'cx>>, Error> {
            if !is_array(logical) {
                return Ok(None);
            }
            let v = arg(first + array_component(logical))?;
            Ok(Some(if super::lower::is_fp(tcx, v.ty()) {
                self.clamp_array_index(v, metadata)?
            } else {
                v
            }))
        };
        // Texel offsets o0.. from argument `first` (`count` of them, the rest 0). Constants (undef
        // is 0) as immediates. Runtime ones, SM 6.7's programmable offsets and Gather's at any
        // model, pass through: wrapped to D3D's low 4 bits, sign-extended, when `wrap` (Load and
        // Sample*); Gather's unwrapped, as the DXBC path's gather4_po (spec 3.6). Constants wrap
        // too: from 6.7 DXC no longer checks their range. Answers whether all were constant.
        let offset_values = |first: u32, count: u32, wrap: bool| -> Result<_, Error> {
            let mut constant = true;
            let mut out = [self.int(0); 3];
            for (i, slot) in (0..3_u32).zip(&mut out) {
                let v = if i < count && !(one_d && i == 1) {
                    Some(arg(first + i)?)
                } else {
                    None
                };
                match v {
                    // SAFETY: a value of the module.
                    Some(v) if unsafe { sys::is_a_constant(v.raw()) }.is_null() => {
                        *slot = if wrap {
                            let shifted = b.shl(v, cx.const_int(v.ty(), 28, false));
                            b.ashr(shifted, cx.const_int(v.ty(), 28, false))
                        } else {
                            v
                        };
                        constant = false;
                    }
                    _ => {
                        let c =
                            v.map_or(0, |v| super::metadata::const_u32(v.raw(), 0).cast_signed());
                        let c = if wrap { ((c & 15) ^ 8) - 8 } else { c };
                        *slot = self.int(c.cast_unsigned());
                    }
                }
            }
            Ok((out, constant))
        };
        // The offsets as the texture kind's offset vector (`GetOffset`): all constant, today's
        // immediate form.
        let offsets = |first: u32, count: u32, wrap: bool| -> Result<Option<Val<'cx>>, Error> {
            let (v, constant) = offset_values(first, count, wrap)?;
            if !constant {
                return Ok(air.get_offset_values(&tex, v));
            }
            let immediates = v.map(|v| super::metadata::const_u32(v.raw(), 0).cast_signed());
            Ok(air.get_offset(&tex, immediates))
        };
        // The texel (vector or depth scalar) as the call's ResRet struct. No texel: the builder
        // has no form for this texture kind (a comparison of a 1D texture, which binds as a 2D
        // colour texture, never a depth one).
        let result =
            |texel: Result<(Val<'cx>, Val<'cx>), Error>| -> Result<Option<Val<'cx>>, Error> {
                let Ok((texel, _)) = texel else {
                    return Err(unsupported(call));
                };
                let elem = tcx.field(ty, 0).ok_or_else(|| unsupported(call))?;
                let vector = matches!(tcx.kind(texel.ty()), TyKind::Vector(..));
                let mut fields = [None; 5];
                for (c, field) in (0..4_u64).zip(&mut fields) {
                    let v = if vector {
                        b.extract_element_u64(texel, c)
                    } else {
                        texel
                    };
                    *field = Some(self.cast(v, elem));
                }
                // Status: fully mapped.
                fields[4] = Some(self.int(1));
                Ok(Some(self.aggregate(ty, &fields)))
            };

        match opcode {
            // (66, handle, i32 mipOrSample, i32 c0, c1, c2, i32 o0, o1, o2)
            op::TEXTURE_LOAD => {
                let pos = vector(3, i32_ty)?;
                // Added to pos, which `GetOffset` has no form for on multisampled kinds.
                let (immediates, _) = offset_values(6, dims, true)?;
                let mut offset = cx.undef(pos.ty());
                for (i, &immediate) in (0..u64::from(dims)).zip(&immediates) {
                    offset = b.insert_element_u64(offset, immediate, i);
                }
                let pos = b.add(pos, offset);
                let ms = is_multisampled(logical);
                if matches!(tex.memory_access, MemoryAccess::ReadWrite) {
                    // See earlier writes, as the DXBC typed UAV load.
                    air.create_texture_fence(&tex, handle)?;
                }
                let at = TexelAt {
                    pos,
                    array_index: array_index(3)?,
                    sample_or_face: if ms { Some(arg(2)?) } else { None },
                    level: Some(if ms { self.int(0) } else { arg(2)? }),
                };
                return result(air.create_read(&tex, handle, &at, coherent));
            }
            // (67, handle, i32 c0, c1, c2, v0, v1, v2, v3, i8 mask)
            op::TEXTURE_STORE => {
                let pos = vector(2, i32_ty)?;
                let texel_ty = air.texel_type(&tex);
                let element = tcx.element(texel_ty).ok_or_else(|| unsupported(call))?;
                let mut texel = cx.undef(texel_ty);
                for c in 0..4_u32 {
                    let value = self.cast(arg(5 + c)?, element);
                    texel = b.insert_element_u64(texel, value, c.into());
                }
                let at = TexelAt {
                    pos,
                    array_index: array_index(2)?,
                    sample_or_face: None,
                    level: Some(self.int(0)),
                };
                air.create_write(&tex, handle, &at, texel, coherent)?;
                return Ok(None);
            }
            // (78, handle, i32 op, i32 c0, c1, c2, i32 value),
            // (79, handle, i32 c0, c1, c2, i32 compare, i32 value)
            op::ATOMIC_BIN_OP | op::ATOMIC_COMPARE_EXCHANGE => {
                let cas = opcode == op::ATOMIC_COMPARE_EXCHANGE;
                let first = if cas { 2 } else { 3 };
                let (pos, array) = if logical == Kind::TextureBuffer {
                    // As the DXBC typed UAV atomic: out of range goes to ~0u.
                    let index = arg(first)?;
                    let oob = b.icmp(sys::INT_UGE, index, self.decode_high(metadata));
                    let address = b.add(index, self.decode_low(metadata));
                    (b.select(oob, self.int(u32::MAX), address), None)
                } else {
                    (vector(first, i32_ty)?, array_index(first)?)
                };
                let splat = |v| b.vector_splat(4, v, c"");
                let old = if cas {
                    let cmp = splat(arg(5)?);
                    let new = splat(arg(6)?);
                    air.create_atomic_cmpxchg(&tex, handle, pos, cmp, new, array)
                        .map(|(old, _)| old)
                } else {
                    let op = atomic_op(const_arg(call, 2, 0));
                    air.create_texture_atomic_rmw(&tex, handle, op, pos, splat(arg(6)?), array)
                };
                // Not an integer texture.
                let old = old.map_err(|_| unsupported(call))?;
                return Ok(Some(b.extract_element_u64(old, 0)));
            }
            _ => {}
        }

        // Sampling and gathering: the sampler is argument 2, coordinates start at 3.
        let sampler_handle = self.handle(call, 2)?;
        let conv = self.conv;
        let Some(samplers) = conv.binding.sampler(
            air,
            conv.function,
            sampler_handle.range,
            sampler_handle.index,
        )?
        else {
            return Ok(Some(cx.null(ty)));
        };
        let cube = dims == 3 && logical != Kind::Texture3d;
        let sampler = if cube {
            samplers.cube_sampler
        } else {
            samplers.sampler
        };
        // The sampler's LOD bias.
        let bias = b.bitcast(b.trunc(samplers.metadata, i32_ty), tcx.float());
        let coord = vector(3, tcx.float())?;
        let array = array_index(3)?;
        let at = |offset| SampleAt {
            coord,
            array_index: array,
            offset,
        };
        // Sample*'s o0..o2. Only there: Gather's argument 9 is its channel, and CalculateLOD's
        // arguments end at 6.
        let sample_offset = || offsets(7, dims, true);
        let min_lod = |shader_clamp: Val<'cx>| -> Result<Val<'cx>, Error> {
            let clamp = self.decode_min_lod_clamp(metadata);
            // SAFETY: a value of the module.
            if unsafe { sys::is_a_undef_value(shader_clamp.raw()) }.is_null() {
                return air.create_fp_bin_op(FpBinOp::Fmax, clamp, shader_clamp, true);
            }
            Ok(clamp)
        };
        // MacNeutron: D3D adds the sampler's MipLODBias to a gradient sample's LOD; Metal's
        // gradient sample takes no bias. Gradients scaled by 2^bias move the LOD by bias, also
        // past the anisotropy cap.
        let scaled = |(dx, dy): (Val<'cx>, Val<'cx>), lod_bias| -> Result<_, Error> {
            let factor = air.create_fp_un_op(FpUnOp::Exp2, lod_bias, false)?;
            let scale = b.vector_splat(dims, factor, c"");
            let ddx = b.fmul(dx, scale);
            Ok((ddx, b.fmul(dy, scale)))
        };
        // SampleGrad's and SampleCmpGrad's ddx0..2 from argument `first`, ddy0..2 after them (a
        // 1D texture's y is 0).
        let gradients = |first: u32| -> Result<_, Error> {
            let zero = cx.const_real(tcx.float(), 0.0);
            let (mut ddx, mut ddy) = (cx.undef(coord.ty()), cx.undef(coord.ty()));
            for i in 0..dims {
                let x = if one_d && i == 1 {
                    zero
                } else {
                    arg(first + i)?
                };
                ddx = b.insert_element_u64(ddx, x, i.into());
                let y = if one_d && i == 1 {
                    zero
                } else {
                    arg(first + 3 + i)?
                };
                ddy = b.insert_element_u64(ddy, y, i.into());
            }
            scaled((ddx, ddy), bias)
        };
        // A compute shader has no implicit derivatives (spec 3.3): Sample, SampleBias, SampleCmp
        // and SampleCmpBias take the quad's coarse gradients of their coordinates through the
        // gradient path (`reference`: the comparisons').
        let compute = self.entry.kind == shader_kind::COMPUTE;
        let coarse = |lod_bias, shader_clamp, reference: Option<Val<'cx>>| {
            let (dx, dy) = scaled(self.coarse_gradients(coord)?, lod_bias)?;
            let grad = Gradient {
                dx,
                dy,
                min_lod: min_lod(shader_clamp)?,
            };
            let at = at(sample_offset()?);
            result(reference.map_or_else(
                || air.create_sample_grad(&tex, handle, sampler, &at, &grad),
                |reference| {
                    air.create_sample_cmp_grad(&tex, handle, sampler, &at, reference, &grad)
                },
            ))
        };

        match opcode {
            // (60, srv, sampler, c0..c3, o0..o2, clamp)
            op::SAMPLE => {
                if compute {
                    return coarse(bias, arg(10)?, None);
                }
                let at = at(sample_offset()?);
                let lod = Lod::BiasMinLod(bias, min_lod(arg(10)?)?);
                result(air.create_sample(&tex, handle, sampler, &at, &lod))
            }
            // (61, ..., bias, clamp)
            op::SAMPLE_BIAS => {
                if compute {
                    return coarse(b.fadd(arg(10)?, bias), arg(11)?, None);
                }
                let at = at(sample_offset()?);
                let lod_bias = b.fadd(arg(10)?, bias);
                let lod = Lod::BiasMinLod(lod_bias, min_lod(arg(11)?)?);
                result(air.create_sample(&tex, handle, sampler, &at, &lod))
            }
            // (62, ..., lod)
            op::SAMPLE_LEVEL => {
                let at = at(sample_offset()?);
                let lod = Lod::Level(b.fadd(arg(10)?, bias));
                result(air.create_sample(&tex, handle, sampler, &at, &lod))
            }
            // (63, ..., ddx0..2, ddy0..2, clamp)
            op::SAMPLE_GRAD => {
                let (dx, dy) = gradients(10)?;
                let min_lod = min_lod(arg(16)?)?;
                let at = at(sample_offset()?);
                let grad = Gradient { dx, dy, min_lod };
                result(air.create_sample_grad(&tex, handle, sampler, &at, &grad))
            }
            // (254, ..., compare, ddx0..2, ddy0..2, clamp); cubes take no offset
            op::SAMPLE_CMP_GRAD => {
                let (dx, dy) = gradients(11)?;
                let min_lod = min_lod(arg(17)?)?;
                let at = at(sample_offset()?);
                let grad = Gradient { dx, dy, min_lod };
                result(air.create_sample_cmp_grad(&tex, handle, sampler, &at, arg(10)?, &grad))
            }
            // (64, ..., compare, clamp)
            op::SAMPLE_CMP => {
                if compute {
                    return coarse(bias, arg(11)?, Some(arg(10)?));
                }
                let at = at(sample_offset()?);
                let lod = Lod::BiasMinLod(bias, min_lod(arg(11)?)?);
                result(air.create_sample_cmp(&tex, handle, sampler, &at, arg(10)?, &lod))
            }
            // (255, ..., compare, bias, clamp)
            op::SAMPLE_CMP_BIAS => {
                if compute {
                    return coarse(b.fadd(arg(11)?, bias), arg(12)?, Some(arg(10)?));
                }
                let at = at(sample_offset()?);
                let lod_bias = b.fadd(arg(11)?, bias);
                let lod = Lod::BiasMinLod(lod_bias, min_lod(arg(12)?)?);
                result(air.create_sample_cmp(&tex, handle, sampler, &at, arg(10)?, &lod))
            }
            // (65, ..., compare)
            op::SAMPLE_CMP_LEVEL_ZERO => {
                let at = at(sample_offset()?);
                let lod = Lod::Level(cx.const_real(tcx.float(), 0.0));
                result(air.create_sample_cmp(&tex, handle, sampler, &at, arg(10)?, &lod))
            }
            // (73, srv, sampler, c0..c3, i32 o0, o1, i32 channel)
            op::TEXTURE_GATHER => {
                let at = at(offsets(7, 2, false)?);
                result(air.create_gather(&tex, handle, sampler, &at, Some(arg(9)?)))
            }
            // (74, ..., i32 channel, float compare)
            op::TEXTURE_GATHER_CMP => {
                let reference = arg(10)?;
                let at = at(offsets(7, 2, false)?);
                result(air.create_gather_compare(&tex, handle, sampler, &at, reference))
            }
            // (81, srv, sampler, c0, c1, c2, i1 clamped)
            op::CALCULATE_LOD => {
                // ponytail: a cube in a compute shader keeps Metal's LOD (no derivatives in a
                // kernel); D3D's projection onto the major face when a shader needs it
                let clamped = const_arg(call, 6, 1) != 0;
                if !compute || cube {
                    let (lod, unclamped) =
                        air.create_calculate_lod(&tex, handle, sampler, coord)?;
                    return Ok(Some(if clamped { lod } else { unclamped }));
                }
                // Compute (spec 3.3): 0.5 log2 of the larger squared gradient in mip 0's texels;
                // clamped to the view's mips.
                let (ddx, ddy) = self.coarse_gradients(coord)?;
                let mut size = cx.undef(coord.ty());
                for (i, axis) in (0..dims).zip([Query::Width, Query::Height, Query::Depth]) {
                    let n = if one_d && i == 1 {
                        self.int(0)
                    } else {
                        air.create_texture_query(&tex, handle, axis, Some(self.int(0)))?
                    };
                    size = b.insert_element_u64(size, b.uitofp(n, tcx.float()), i.into());
                }
                let texels2 = |g| {
                    let t = b.fmul(g, size);
                    air.create_dot_product(t, t)
                };
                let x2 = texels2(ddx)?;
                let larger = air.create_fp_bin_op(FpBinOp::Fmax, x2, texels2(ddy)?, true)?;
                let log = air.create_fp_un_op(FpUnOp::Log2, larger, false)?;
                let mut lod = b.fmul(cx.const_real(tcx.float(), 0.5), log);
                if clamped {
                    let mips = air.create_texture_query(
                        &tex,
                        handle,
                        Query::NumMipLevels,
                        Some(self.int(0)),
                    )?;
                    let top = b.uitofp(b.sub(mips, self.int(1)), tcx.float());
                    let floor = cx.const_real(tcx.float(), 0.0);
                    let low = air.create_fp_bin_op(FpBinOp::Fmax, lod, floor, true)?;
                    lod = air.create_fp_bin_op(FpBinOp::Fmin, low, top, true)?;
                }
                Ok(Some(lod))
            }
            _ => Err(unsupported(call)),
        }
    }

    /// `%dx.types.Dimensions {width, height, depth or array size, mips or samples}`.
    ///
    /// `LowerDimensions`, as the DXBC `resinfo`/`bufinfo` lowerings answer them.
    fn dimensions(
        &self,
        call: Val<'cx>,
        view: &Handle<'_, 'cx>,
    ) -> Result<Option<Val<'cx>>, Error> {
        let b = self.b();
        let air = self.air();
        let ty = call.ty();
        let kind = view.resource.kind;
        let zero = self.int(0);
        if kind == resource_kind::RAW_BUFFER || kind == resource_kind::STRUCTURED_BUFFER {
            let buffer = self.buffer(view)?;
            let mut size = buffer
                .and_then(|buffer| buffer.metadata)
                .map_or(zero, |metadata| self.decode_low(metadata));
            if kind == resource_kind::STRUCTURED_BUFFER && view.resource.stride != 0 {
                size = b.udiv(size, self.int(view.resource.stride));
            }
            let value = self.aggregate(ty, &[Some(size), Some(zero), Some(zero), Some(zero)]);
            return Ok(Some(value));
        }
        let Some(d) = self.texture(view)? else {
            return Ok(Some(self.cx().null(ty)));
        };
        let (tex, handle, metadata, _, logical) = parts(d);
        if kind == resource_kind::TYPED_BUFFER {
            let count = self.decode_high(metadata);
            let value = self.aggregate(ty, &[Some(count), Some(zero), Some(zero), Some(zero)]);
            return Ok(Some(value));
        }
        let mut level = self.arg(call, 2)?;
        // SAFETY: a value of the module.
        if !unsafe { sys::is_a_undef_value(level.raw()) }.is_null() {
            level = zero;
        }
        let query = |what, lod| air.create_texture_query(&tex, handle, what, Some(lod));
        let width = query(Query::Width, level)?;
        let (mut height, mut depth) = (zero, zero);
        if logical == Kind::Texture1dArray {
            height = query(Query::ArrayLength, zero)?;
        } else if logical != Kind::Texture1d {
            height = query(Query::Height, level)?;
        }
        if logical == Kind::Texture3d {
            depth = query(Query::Depth, level)?;
        } else if is_array(logical) && logical != Kind::Texture1dArray {
            depth = query(Query::ArrayLength, zero)?;
        }
        let last = if is_multisampled(logical) {
            query(Query::NumSamples, zero)?
        } else {
            query(Query::NumMipLevels, zero)?
        };
        Ok(Some(self.aggregate(
            ty,
            &[Some(width), Some(height), Some(depth), Some(last)],
        )))
    }

    /// Ops 78 and 79 with the i64 overload, under a lock from the device's table (`Atomic64`).
    ///
    /// A collision only serialises. Buffers (raw, structured, root and heap ones, and typed
    /// ones, which DXIL can only reach with atomics: their buffer's base comes from the
    /// descriptor, their first element from its metadata) are keyed by the element's device
    /// address, so every view of the same bytes takes the same lock, and their halves are read
    /// and written as relaxed 32-bit atomics. Textures (`R32G32_UINT`) are keyed by the texel's
    /// (x, y) alone, which every view of it agrees on, and read and written through the
    /// `<4 x i32>` texel (lo .x, hi .y) under device and texture fences, with a texture fence
    /// after the write. A pixel shader's helper lanes (D3D gives them no UAV side effects, and a
    /// helper whose lock atomic were discarded could spin forever) and an out-of-bounds buffer
    /// element skip it and read 0; a texture out of bounds is left to Metal, as the 32-bit
    /// texture atomics are.
    fn atomic64(
        &self,
        opcode: u32,
        call: Val<'cx>,
        view: &Handle<'_, 'cx>,
    ) -> Result<Option<Val<'cx>>, Error> {
        let b = self.b();
        let air = self.air();
        let cx = self.cx();
        let tcx = self.tcx();
        let conv = self.conv;
        let arg = |i| self.arg(call, i);
        let kind = view.resource.kind;
        let Some(locks) = conv.binding.atomic64_locks(air, conv.function)? else {
            return Err(unsupported(call));
        };
        let raw = kind == resource_kind::RAW_BUFFER || kind == resource_kind::STRUCTURED_BUFFER;
        let texel = !raw && kind != resource_kind::TYPED_BUFFER;
        let (buffer, d) = if raw {
            (self.buffer(view)?, None)
        } else {
            (None, self.texture(view)?)
        };
        if buffer.is_none() && d.is_none() {
            // Unbound.
            return Ok(Some(cx.const_int(tcx.int(64), 0, false)));
        }
        if kind == resource_kind::TYPED_BUFFER
            && d.as_ref().is_some_and(|d| d.buffer_address.is_none())
        {
            return Err(unsupported(call));
        }
        let cas = opcode == op::ATOMIC_COMPARE_EXCHANGE;
        // (78, handle, op, c0, c1, c2, value); (79, handle, c0, c1, c2, compare, value)
        let coord = if cas { 2 } else { 3 };
        let (i32_ty, i64_ty) = (tcx.int(32), tcx.int(64));
        // A buffer's qword, null out of bounds; a texture's texel.
        let mut ptr = None;
        let mut texture = None;
        if let Some(buffer) = &buffer {
            let offset = if kind == resource_kind::STRUCTURED_BUFFER {
                b.add(
                    b.mul(arg(coord)?, self.int(view.resource.stride)),
                    arg(coord + 1)?,
                )
            } else {
                arg(coord)?
            };
            ptr = Some(self.element_pointer(buffer, offset, i64_ty));
        } else if let Some(d) = d {
            if texel {
                // c0.. as the texture ops': a 1D texture's y is 0, an array's slice its array
                // component.
                let (tex, handle, _, coherent, logical) = parts(d);
                let one_d = matches!(logical, Kind::Texture1d | Kind::Texture1dArray);
                let dims = dimensions(logical);
                let mut pos = cx.undef(tcx.vector(i32_ty, dims));
                for i in 0..dims {
                    let v = if one_d && i == 1 {
                        self.int(0)
                    } else {
                        arg(coord + i)?
                    };
                    pos = b.insert_element_u64(pos, v, i.into());
                }
                let array = if is_array(logical) {
                    Some(arg(coord + array_component(logical))?)
                } else {
                    None
                };
                texture = Some((tex, handle, coherent, pos, array));
            } else {
                // The qword index plus the first element of the buffer's base; out of bounds from
                // the view's element count.
                let base = req(d.buffer_address, "a typed buffer of no address")?;
                let qwords = b.bitcast(base, tcx.ptr(i64_ty, DEVICE));
                let index = b.zext(b.add(arg(coord)?, self.decode_low(d.metadata)), i64_ty);
                let element = b.gep(i64_ty, qwords, &[index]);
                let oob = b.icmp(sys::INT_UGE, arg(coord)?, self.decode_high(d.metadata));
                ptr = Some(b.select(oob, cx.null(element.ty()), element));
            }
        }
        let mut skip = ptr.map_or_else(|| air.get_bool(false), |ptr| b.is_null(ptr));
        if self.entry.kind == shader_kind::PIXEL {
            let helper = self.simd(b"air.simd_is_helper_thread", tcx.int(1), &[])?;
            skip = b.or(skip, helper);
        }
        let head = self.insert_block()?;
        let run = b.not(skip);
        // SAFETY: a condition and the call it splits before, of the function built.
        let then = self.val(unsafe {
            sys::airconv_shim_split_block_and_insert_if_then(run.raw(), call.raw(), 0)
        })?;
        b.position_before(then);
        // The key, in 32-bit math: h = (the address's low 32 bits) >> 3, or x * 0x9E3779B1 + y;
        // h ^= h >> 16; lock h & (ATOMIC64_LOCKS - 1).
        let mut key = match (&texture, ptr) {
            (Some((_, _, _, pos, _)), _) => {
                let x = b.mul(b.extract_element_u64(*pos, 0), self.int(0x9E37_79B1));
                b.add(x, b.extract_element_u64(*pos, 1))
            }
            (None, Some(ptr)) => b.lshr(b.trunc(b.ptrtoint(ptr, i64_ty), i32_ty), self.int(3)),
            (None, None) => return Err(unsupported(call)),
        };
        key = b.and(
            b.xor(key, b.lshr(key, self.int(16))),
            self.int(ATOMIC64_LOCKS - 1),
        );
        let halves = ptr.filter(|_| !texel).map(|ptr| {
            let lo = b.bitcast(ptr, tcx.ptr(i32_ty, DEVICE));
            (lo, b.const_gep1_32(i32_ty, lo, 1))
        });
        let relaxed = sys::ATOMIC_ORDERING_MONOTONIC;
        let thirty_two = cx.const_int(i64_ty, 32, false);
        // One texel read, or the two halves.
        let load = || -> Result<Val<'cx>, Error> {
            let t = match &texture {
                Some((tex, handle, coherent, pos, array)) => {
                    let at = texel_at(*pos, *array, self.int(0));
                    Some(air.create_read(tex, *handle, &at, *coherent)?.0)
                }
                None => None,
            };
            let half = |i: u64| -> Result<Val<'cx>, Error> {
                if let Some(t) = t {
                    return Ok(b.zext(b.extract_element_u64(t, i), i64_ty));
                }
                let (lo, hi) = req_pair(halves)?;
                let word = b.aligned_load(i32_ty, if i == 0 { lo } else { hi }, 4);
                // SAFETY: a load just made.
                unsafe { sys::set_ordering(word.raw(), relaxed) };
                Ok(b.zext(word, i64_ty))
            };
            let low = half(0)?;
            Ok(b.or(low, b.shl(half(1)?, thirty_two)))
        };
        let store = |v: Val<'cx>| -> Result<(), Error> {
            let half = |i: u64| b.trunc(if i == 0 { v } else { b.lshr(v, thirty_two) }, i32_ty);
            if let Some((tex, handle, coherent, pos, array)) = &texture {
                // <lo, hi, 0, 0>, then the texture fence.
                let mut t = cx.null(tcx.vector(i32_ty, 4));
                for i in 0..2 {
                    t = b.insert_element_u64(t, half(i), i);
                }
                let at = texel_at(*pos, *array, self.int(0));
                air.create_write(tex, *handle, &at, t, *coherent)?;
                air.create_texture_fence(tex, *handle)?;
                return Ok(());
            }
            let (lo, hi) = req_pair(halves)?;
            for i in 0..2 {
                let word = b.aligned_store(half(i), if i == 0 { lo } else { hi }, 4);
                // SAFETY: a store just made.
                unsafe { sys::set_ordering(word.raw(), relaxed) };
            }
            Ok(())
        };
        let flags = if texel {
            MemFlags::DEVICE.with(MemFlags::TEXTURE)
        } else {
            MemFlags::DEVICE
        };
        let lock = b.gep(i32_ty, locks, &[key]);
        let old = self.under_lock(then, lock, flags, ThreadScope::Device, |at| {
            let value = load()?;
            if cas {
                // Stores only on a 64-bit match (CompareStore: the same, its result unused).
                let matched = b.icmp(sys::INT_EQ, value, arg(5)?);
                // SAFETY: a condition and the section's branch it splits before.
                let store_then = self.val(unsafe {
                    sys::airconv_shim_split_block_and_insert_if_then(matched.raw(), at.raw(), 0)
                })?;
                b.position_before(store_then);
                store(arg(6)?)?;
                b.position_before(at);
            } else {
                let op = atomic_op(const_arg(call, 2, 0));
                // SAFETY: the builder is positioned in the function built; the operands are of
                // its context.
                let next = unsafe {
                    sys::airconv_shim_build_atomic_rmw_value(
                        b.raw(),
                        op,
                        value.raw(),
                        arg(6)?.raw(),
                    )
                };
                if next.is_null() {
                    return Err(unsupported(call));
                }
                store(self.val(next)?)?;
            }
            Ok(value)
        })?;
        let end = self.insert_block()?;
        b.position_before(call);
        let result = b.phi(i64_ty);
        b.add_incoming(result, old, end);
        b.add_incoming(result, cx.const_int(i64_ty, 0, false), head);
        Ok(Some(result))
    }

    /// An array slice rounded, made an integer and clamped to the texture's (`ClampArrayIndex`).
    ///
    /// The DXBC converter's, which keeps it private.
    fn clamp_array_index(&self, value: Val<'cx>, metadata: Val<'cx>) -> Result<Val<'cx>, Error> {
        let air = self.air();
        let rounded = air.create_fp_un_op(FpUnOp::Rint, value, true)?;
        let integer = air.create_convert_to_signed(rounded)?;
        let positive = air.create_int_bin_op(IntBinOp::Max, integer, self.int(0), true)?;
        let max_index = self.b().sub(self.decode_high(metadata), self.int(1));
        air.create_int_bin_op(IntBinOp::Min, positive, max_index, true)
    }

    /// A descriptor metadata's low word: a raw buffer's byte length, a texture buffer's offset.
    ///
    /// `DecodeRawBufferByteLength`, `DecodeTextureBufferOffset`.
    fn decode_low(&self, metadata: Val<'cx>) -> Val<'cx> {
        self.b().trunc(metadata, self.tcx().int(32))
    }

    /// A descriptor metadata's high word: a texture buffer's element count, an array's length.
    ///
    /// `DecodeTextureBufferElement`, `DecodeTextureArrayLength`.
    fn decode_high(&self, metadata: Val<'cx>) -> Val<'cx> {
        let b = self.b();
        let thirty_two = self.cx().const_int(metadata.ty(), 32, false);
        b.trunc(b.lshr(metadata, thirty_two), self.tcx().int(32))
    }

    /// A texture's minimum LOD clamp, its metadata's low word as a float.
    ///
    /// `DecodeTextureMinLODClamp`.
    fn decode_min_lod_clamp(&self, metadata: Val<'cx>) -> Val<'cx> {
        self.b()
            .bitcast(self.decode_low(metadata), self.tcx().float())
    }
}

/// A texture descriptor's AIR description, handle, metadata, coherence and declared kind.
const fn parts(d: TextureDescriptor<'_>) -> (Texture, Val<'_>, Val<'_>, bool, Kind) {
    (
        Texture {
            kind: air_kind(d.kind),
            sample_type: d.sample_type,
            memory_access: d.memory_access,
        },
        d.handle,
        d.metadata,
        d.global_coherent,
        d.kind_logical,
    )
}

/// A read or write of the texel at `pos`, its slice `array`, level `level`.
const fn texel_at<'cx>(pos: Val<'cx>, array: Option<Val<'cx>>, level: Val<'cx>) -> TexelAt<'cx> {
    TexelAt {
        pos,
        array_index: array,
        sample_or_face: None,
        level: Some(level),
    }
}

/// The buffer halves of a 64-bit lock path's qword.
fn req_pair<'cx>(halves: Option<(Val<'cx>, Val<'cx>)>) -> Result<(Val<'cx>, Val<'cx>), Error> {
    halves.ok_or_else(|| crate::dxbc::converter::invalid("a buffer qword of no halves"))
}

/// Whether a declared kind has an array slice (`IsArray`).
const fn is_array(k: Kind) -> bool {
    matches!(
        k,
        Kind::Texture1dArray
            | Kind::Texture2dArray
            | Kind::Depth2dArray
            | Kind::TextureCubeArray
            | Kind::DepthCubeArray
            | Kind::Texture2dMsArray
            | Kind::Depth2dMsArray
    )
}

/// Whether a declared kind is multisampled (`IsMultisampled`).
const fn is_multisampled(k: Kind) -> bool {
    matches!(
        k,
        Kind::Texture2dMs | Kind::Texture2dMsArray | Kind::Depth2dMs | Kind::Depth2dMsArray
    )
}

/// The components of the position or coordinate vector (`Dimensions`).
///
/// 1D textures are 2D in Metal here, as the DXBC path treats them.
const fn dimensions(k: Kind) -> u32 {
    match k {
        Kind::Texture3d
        | Kind::TextureCube
        | Kind::DepthCube
        | Kind::TextureCubeArray
        | Kind::DepthCubeArray => 3,
        _ => 2,
    }
}

/// The component of c0..c3 holding the array slice (`ArrayComponent`).
const fn array_component(k: Kind) -> u32 {
    match k {
        Kind::Texture1dArray => 1,
        Kind::TextureCubeArray | Kind::DepthCubeArray => 3,
        _ => 2,
    }
}

/// DXIL's atomic operation codes (`DXIL.rst`, `AtomicBinOpCode`) as LLVM's (`AtomicOp`).
///
/// A code past the last reads as `Xchg`, the table's last.
const fn atomic_op(code: u32) -> sys::AtomicRMWBinOp {
    match code {
        0 => sys::ATOMIC_RMW_BIN_OP_ADD,
        1 => sys::ATOMIC_RMW_BIN_OP_AND,
        2 => sys::ATOMIC_RMW_BIN_OP_OR,
        3 => sys::ATOMIC_RMW_BIN_OP_XOR,
        4 => sys::ATOMIC_RMW_BIN_OP_MIN,
        5 => sys::ATOMIC_RMW_BIN_OP_MAX,
        6 => sys::ATOMIC_RMW_BIN_OP_U_MIN,
        7 => sys::ATOMIC_RMW_BIN_OP_U_MAX,
        _ => sys::ATOMIC_RMW_BIN_OP_XCHG,
    }
}

/// The components of a `ResRet` the shader reads (`ExtractedComponents`).
///
/// SM 6.0/6.1 `BufferLoad` names all four: one bounds check covering all of them would zero a
/// load of a view's last dword. Any use but an `extractvalue` of a component: all four.
fn extracted_components(call: Val<'_>) -> u32 {
    let mut mask = 0;
    // SAFETY: a call of the module.
    let mut next_use = unsafe { sys::get_first_use(call.raw()) };
    while !next_use.is_null() {
        // SAFETY: a use of the call.
        let user = unsafe { sys::get_user(next_use) };
        // SAFETY: as above.
        next_use = unsafe { sys::get_next_use(next_use) };
        // SAFETY: a value of the module.
        let extract = !unsafe { sys::is_a_extract_value_inst(user) }.is_null();
        // SAFETY: an `extractvalue`, which has indices.
        if !extract || unsafe { sys::get_num_indices(user) } != 1 {
            return 0xf;
        }
        // SAFETY: an `extractvalue` of one index.
        let indices = unsafe { sys::get_indices(user) };
        // SAFETY: LLVM answers its one index.
        let index = unsafe { *indices };
        if index < 4 {
            mask |= 1 << index;
        }
    }
    if mask == 0 { 1 } else { mask }
}
