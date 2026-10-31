use core::ptr::{self, NonNull};

use winemetal_sys::wire::{
    WMT_COMPUTE_COMMAND_DISPATCH_THREADS, WMT_COMPUTE_COMMAND_SET_BUFFER,
    WMT_COMPUTE_COMMAND_SET_BYTES, WMT_COMPUTE_COMMAND_SET_PSO, WMT_COMPUTE_COMMAND_SET_TEXTURE,
    WmtcmdBase,
};

use super::*;

/// A compute record as the tests compare it.
#[derive(Debug, PartialEq)]
enum Rec {
    Pso(u64, [u64; 3]),
    Buffer(u64, u64, u8),
    Texture(u64, u8),
    Bytes(Vec<u8>, u8),
    Dispatch([u64; 3]),
}

/// The address a `next` or `bytes` field holds.
fn address(p: MemPtr) -> usize {
    usize::try_from(p.ptr).unwrap()
}

/// The compute pass's records after its head.
fn records(encoder: &Encoder) -> Vec<Rec> {
    let mut out = Vec::new();
    let mut at = encoder.pass().unwrap().records();
    loop {
        // SAFETY: a record of the pass, led by `WmtcmdBase`'s fields.
        let next = unsafe { at.as_ref() }.next;
        let Some(record) = NonNull::new(ptr::with_exposed_provenance_mut::<WmtcmdBase>(address(
            next,
        ))) else {
            return out;
        };
        at = record;
        let p = record.as_ptr();
        // SAFETY: a record of the pass, of the type its `type` field names (every read below).
        let r#type = unsafe { (*p).r#type };
        out.push(match r#type {
            WMT_COMPUTE_COMMAND_SET_PSO => {
                // SAFETY: as above.
                let c = unsafe { &*p.cast::<WmtcmdComputeSetpso>() };
                let s = c.threadgroup_size;
                Rec::Pso(c.pso, [s.width, s.height, s.depth])
            }
            WMT_COMPUTE_COMMAND_SET_BUFFER => {
                // SAFETY: as above.
                let c = unsafe { &*p.cast::<WmtcmdComputeSetbuffer>() };
                Rec::Buffer(c.buffer, c.offset, c.index)
            }
            WMT_COMPUTE_COMMAND_SET_TEXTURE => {
                // SAFETY: as above.
                let c = unsafe { &*p.cast::<WmtcmdComputeSettexture>() };
                Rec::Texture(c.texture, c.index)
            }
            WMT_COMPUTE_COMMAND_SET_BYTES => {
                // SAFETY: as above.
                let c = unsafe { &*p.cast::<WmtcmdComputeSetbytes>() };
                let bytes = ptr::with_exposed_provenance::<u8>(address(c.bytes));
                let len = usize::try_from(c.length).unwrap();
                // SAFETY: the copy the encoder holds, `length` bytes.
                let bytes = unsafe { core::slice::from_raw_parts(bytes, len) }.to_vec();
                Rec::Bytes(bytes, c.index)
            }
            WMT_COMPUTE_COMMAND_DISPATCH_THREADS => {
                // SAFETY: as above.
                let s = unsafe { &*p.cast::<WmtcmdComputeDispatch>() }.size;
                Rec::Dispatch([s.width, s.height, s.depth])
            }
            other => panic!("record type {other}"),
        });
    }
}

fn bytes<const N: usize>(words: [u32; N]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_ne_bytes()).collect()
}

#[test]
fn clears_record_as_cpp() {
    let ctx = EmulatedContext::fake();
    let mut encoder = Encoder::headless();
    encoder.start_compute_pass(None);
    ctx.clear_buffer_uint(&mut encoder, 0x55, 16, 100, [1, 2, 3, 4]);
    ctx.clear_buffer_float(&mut encoder, 0x56, 0, 0x1_0000_0003, [1.0, 0.0, 0.0, -2.0]);
    ctx.clear_texture_buffer_float(&mut encoder, 0x66, 3, 9, [0.5, 1.0, 2.0, 4.0]);
    ctx.clear_texture_2d_array_uint(&mut encoder, 0x77, [4, 5, 6], [9, 8, 7, 6]);
    let pso = |i: usize| FAKE_PIPELINE + i as u64;
    let floats = |f: [f32; 4]| bytes(f.map(f32::to_bits));
    assert_eq!(
        records(&encoder),
        [
            // The buffer clear: the values at 1, the element count's low 4 bytes at 2.
            Rec::Pso(pso(CLEAR_BUFFER_UINT), [32, 1, 1]),
            Rec::Buffer(0x55, 16, 0),
            Rec::Bytes(bytes([1, 2, 3, 4]), 1),
            Rec::Bytes(bytes([100]), 2),
            Rec::Dispatch([100, 1, 1]),
            // A float buffer clear is the uint one over the floats' bits.
            Rec::Pso(pso(CLEAR_BUFFER_UINT), [32, 1, 1]),
            Rec::Buffer(0x56, 0, 0),
            Rec::Bytes(floats([1.0, 0.0, 0.0, -2.0]), 1),
            Rec::Bytes(bytes([3]), 2),
            Rec::Dispatch([0x1_0000_0003, 1, 1]),
            // A texel buffer: values, offset, size and 8 bytes of padding in one argument.
            Rec::Pso(pso(CLEAR_TEXTURE_BUFFER_FLOAT), [32, 1, 1]),
            Rec::Texture(0x66, 0),
            Rec::Bytes(
                [floats([0.5, 1.0, 2.0, 4.0]), bytes([3, 9, 0, 0])].concat(),
                1
            ),
            Rec::Dispatch([9, 1, 1]),
            // An array texture: 8x4 groups over its width, height and slices.
            Rec::Pso(pso(CLEAR_TEXTURE_2D_ARRAY_UINT), [8, 4, 1]),
            Rec::Texture(0x77, 0),
            Rec::Bytes(bytes([9, 8, 7, 6]), 1),
            Rec::Dispatch([4, 5, 6]),
        ]
    );
}
