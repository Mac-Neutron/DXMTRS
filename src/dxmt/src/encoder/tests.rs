use core::ptr::{self, NonNull};

use winemetal_sys::wire::{
    MTL_SM50_SHADER_ARGUMENT_BUFFER, MTL_SM50_SHADER_ARGUMENT_TEXTURE,
    MTL_SM50_SHADER_ARGUMENT_UAV_COUNTER, MtlSm50ShaderArgument, SM50_BINDING_INDEX_ARGUMENT_TABLE,
    SM50_BINDING_INDEX_CONSTANT_BUFFER, SM50_BINDING_INDEX_CONSTANT_BUFFER2,
    SM50_BINDING_INDEX_VERTEX_BUFFER, WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER_OFFSET,
    WMT_RENDER_COMMAND_SET_OBJECT_BUFFER_OFFSET, WMT_RENDER_COMMAND_SET_VERTEX_BUFFER_OFFSET,
    WMT_RENDER_COMMAND_USE_RESOURCE, WMT_RENDER_STAGE_FRAGMENT, WMT_RENDER_STAGE_OBJECT,
    WMT_RENDER_STAGE_VERTEX, WMT_RESOURCE_USAGE_READ,
};

use super::*;

/// What a qword holds until something writes it.
const UNTOUCHED: u64 = 0xaaaa_aaaa_aaaa_aaaa;
/// The argument buffer's offset in its Metal buffer.
const ARGS_AT: u64 = 0x1000;

/// A render record as the tests compare it.
///
/// A use (resource, usage, stages) or a set-buffer-offset (type, offset, index).
#[derive(Debug, PartialEq)]
enum Rec {
    Use(u64, u32, u8),
    Offset(u16, u64, u8),
}

fn argument(r#type: u32, slot: u32, flags: u32, at: u32) -> MtlSm50ShaderArgument {
    MtlSm50ShaderArgument {
        r#type,
        sm50_binding_slot: slot,
        flags,
        structure_ptr_offset: at,
    }
}

/// A headless encoder in a render pass whose argument buffer is `args`.
fn render_pass(args: &mut [u64]) -> Encoder {
    let mut encoder = Encoder::headless();
    let len = size_of_val(args);
    // SAFETY: `args` outlives the encoder in every test and nothing else touches it meanwhile.
    let buffer = unsafe { ArgumentBuffer::new(NonNull::from(args).cast(), len, 0x77, ARGS_AT) };
    encoder.start_render_pass(0, 0, 0, Some(buffer));
    encoder
}

/// The render pass's records after its head.
fn records(encoder: &Encoder) -> Vec<Rec> {
    let mut out = Vec::new();
    let mut at = encoder.pass().unwrap().records();
    loop {
        // SAFETY: a record of the pass, led by `WmtcmdBase`'s fields.
        let next = unsafe { at.as_ref() }.next.ptr;
        let Some(record) = NonNull::new(ptr::with_exposed_provenance_mut::<WmtcmdBase>(
            usize::try_from(next).unwrap(),
        )) else {
            return out;
        };
        // SAFETY: as above.
        let r#type = unsafe { record.as_ref() }.r#type;
        out.push(if r#type == WMT_RENDER_COMMAND_USE_RESOURCE {
            // SAFETY: a use-resource record by its type.
            let u = unsafe { record.cast::<WmtcmdRenderUseresource>().as_ref() };
            Rec::Use(u.resource, u.usage, u.stages)
        } else {
            // SAFETY: the tests record no other render type than these two.
            let o = unsafe { record.cast::<WmtcmdRenderSetbufferoffset>().as_ref() };
            Rec::Offset(o.r#type, o.offset, o.index)
        });
        at = record;
    }
}

#[test]
fn argument_slots_from_reflection() {
    let mut args = [UNTOUCHED; 32];
    let mut encoder = render_pass(&mut args);
    encoder.dummy_sampler_id = 0xd5;
    encoder.bind_sampler::<{ stage::PIXEL }>(
        2,
        Some(BoundSampler {
            state: 0x51,
            cube_state: 0x52,
            lod_bias: 1.5,
            owner: Arc::new(()),
        }),
    );
    let reflection = [
        argument(BINDING_SAMPLER, 2, 0, 4),
        argument(BINDING_SAMPLER, 3, 0, 7),
        argument(BINDING_SRV, 1, MTL_SM50_SHADER_ARGUMENT_TEXTURE, 10),
        argument(
            BINDING_UAV,
            0,
            MTL_SM50_SHADER_ARGUMENT_BUFFER | MTL_SM50_SHADER_ARGUMENT_UAV_COUNTER,
            12,
        ),
        argument(BINDING_SRV, 9, 0, 16),
    ];
    // The table starts at qword 1 of the argument buffer.
    encoder.encode_shader_resources::<{ stage::PIXEL }, { kind::ORDINARY }>(&reflection, 8);
    let table = &args[1..];
    assert_eq!(
        table[4..7],
        [0x51, 0x52, u64::from(1.5f32.to_bits())],
        "a bound sampler"
    );
    assert_eq!(
        table[7..10],
        [0xd5, 0xd5, 0],
        "the dummy sampler, LOD bias 0"
    );
    assert_eq!(table[10..12], [0, 0], "an unbound texture");
    assert_eq!(
        table[12..15],
        [0, 0, 0],
        "an unbound buffer and its counter"
    );
    assert_eq!(
        table[16], UNTOUCHED,
        "an argument neither buffer nor texture"
    );
    assert!([0, 1, 2, 3, 15, 17].iter().all(|&q| table[q] == UNTOUCHED));
    assert_eq!(
        records(&encoder),
        [Rec::Offset(
            WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER_OFFSET,
            ARGS_AT + 8,
            binding_index(SM50_BINDING_INDEX_ARGUMENT_TABLE)
        )]
    );
}

#[test]
fn unbound_buffers_read_the_dummy() {
    let mut args = [UNTOUCHED; 32];
    let mut encoder = render_pass(&mut args);
    // A tessellation pipeline's vertex stage: an object function at the second table index.
    let cbuffers = [argument(0, 0, 0, 0), argument(0, 3, 0, 1)];
    encoder.encode_constant_buffers::<{ stage::VERTEX }, { kind::TESSELLATION }>(&cbuffers, 0);
    encoder.encode_vertex_buffers::<{ kind::ORDINARY }>(0b1010, 16);
    assert_eq!(args[..2], [0, 0], "the dummy's address (null headless)");
    assert_eq!(
        args[2..6],
        [0, 0, 0, 0],
        "two packed entries: address, stride and length 0"
    );
    assert_eq!(args[6], UNTOUCHED);
    assert_eq!(
        records(&encoder),
        [
            Rec::Use(0, WMT_RESOURCE_USAGE_READ, WMT_RENDER_STAGE_OBJECT),
            Rec::Use(0, WMT_RESOURCE_USAGE_READ, WMT_RENDER_STAGE_OBJECT),
            Rec::Offset(
                WMT_RENDER_COMMAND_SET_OBJECT_BUFFER_OFFSET,
                ARGS_AT,
                binding_index(SM50_BINDING_INDEX_CONSTANT_BUFFER2)
            ),
            Rec::Use(0, WMT_RESOURCE_USAGE_READ, WMT_RENDER_STAGE_VERTEX),
            Rec::Use(0, WMT_RESOURCE_USAGE_READ, WMT_RENDER_STAGE_VERTEX),
            Rec::Offset(
                WMT_RENDER_COMMAND_SET_VERTEX_BUFFER_OFFSET,
                ARGS_AT + 16,
                binding_index(SM50_BINDING_INDEX_VERTEX_BUFFER)
            ),
        ]
    );
}

#[test]
#[should_panic(expected = "past the pass's argument buffer")]
fn a_table_past_the_buffer_stops() {
    let mut args = [UNTOUCHED; 4];
    let mut encoder = render_pass(&mut args);
    encoder.encode_constant_buffers::<{ stage::PIXEL }, { kind::ORDINARY }>(
        &[argument(0, 0, 0, 4)],
        0,
    );
}

#[test]
fn track_follows_the_stage() {
    let mut encoder = Encoder::headless();
    let mut tracker = GenericAccessTracker::default();
    encoder.start_compute_pass(None);
    track::<{ stage::COMPUTE }>(encoder.pass.as_mut(), &mut tracker, Access::WRITE);
    encoder.pass = None;
    encoder.start_render_pass(0, 0, 0, None);
    let pass = encoder.pass.as_mut().unwrap();
    assert_eq!((pass.id_vertex, pass.id), (257, 258));
    track::<{ stage::VERTEX }>(Some(&mut *pass), &mut tracker, Access::READ);
    assert!(pass.fence_wait_vertex.test(256) && pass.fence_wait.is_empty());
    track::<{ stage::PIXEL }>(Some(&mut *pass), &mut tracker, Access::READ);
    assert!(
        pass.fence_wait.test(256),
        "the fragment half waits for the compute write too"
    );
}

#[test]
fn residency_once_per_pass_and_use() {
    let read = resident::mask::<{ stage::HULL }, { kind::TESSELLATION }>(true, false);
    let write = resident::mask::<{ stage::HULL }, { kind::TESSELLATION }>(false, true);
    let mut r = resident::Residency::default();
    assert_eq!(r.check(300, read), Some(read));
    assert_eq!(r.check(300, read), None, "resident already for this pass");
    assert_eq!(
        r.check(300, write),
        Some(read | write),
        "a new use adds to the mask"
    );
    assert_eq!(r.check(301, read), Some(read), "a new pass starts over");
    assert_eq!(r.check(300, write), None, "an earlier pass is ignored");
    assert_eq!(
        resident::mask::<{ stage::VERTEX }, { kind::GEOMETRY }>(true, true),
        resident::mask::<{ stage::VERTEX }, { kind::GEOMETRY }>(true, false)
            | resident::mask::<{ stage::VERTEX }, { kind::GEOMETRY }>(false, true)
    );
}

#[test]
fn bound_buffers_tracked_and_resident_once_per_pass() {
    // SAFETY: fake non-zero handle for the test binary's `NSObject_retain`/`NSObject_release`.
    let device = unsafe { Obj::adopt(0xD9) }.expect("non-zero handle");
    let buffer = |flags| {
        let buffer = Arc::new(Buffer::new(256, &device));
        buffer.rename(buffer.allocate(flags));
        buffer
    };
    let (target, constants) = (
        buffer(BufferAllocationFlags::empty()),
        buffer(BufferAllocationFlags::GPU_READONLY),
    );
    let (written, read_only) = (target.current().unwrap(), constants.current().unwrap());
    let mut args = [UNTOUCHED; 4];
    let mut encoder = Encoder::headless();
    encoder.start_compute_pass(None);
    let _ = encoder.access_buffer::<{ stage::COMPUTE }>(&target, Access::WRITE);
    encoder.pass = None;
    let len = size_of_val(&args);
    // SAFETY: `args` outlives the encoder and nothing else touches it meanwhile.
    let arguments =
        unsafe { ArgumentBuffer::new(NonNull::from(&mut args).cast(), len, 0x77, ARGS_AT) };
    encoder.start_render_pass(0, 0, 0, Some(arguments));
    encoder.bind_constant_buffer::<{ stage::PIXEL }>(0, 16, Some(Arc::clone(&target)));
    encoder.bind_constant_buffer::<{ stage::PIXEL }>(1, 0, Some(Arc::clone(&constants)));
    let cbuffers = [argument(0, 0, 0, 0), argument(0, 1, 0, 1)];
    encoder.encode_constant_buffers::<{ stage::PIXEL }, { kind::ORDINARY }>(&cbuffers, 0);
    encoder.encode_constant_buffers::<{ stage::PIXEL }, { kind::ORDINARY }>(&cbuffers, 0);
    assert_eq!(
        args[..2],
        [written.gpu_address() + 16, read_only.gpu_address()]
    );
    let offset = || {
        Rec::Offset(
            WMT_RENDER_COMMAND_SET_FRAGMENT_BUFFER_OFFSET,
            ARGS_AT,
            binding_index(SM50_BINDING_INDEX_CONSTANT_BUFFER),
        )
    };
    assert_eq!(
        records(&encoder),
        [
            Rec::Use(
                written.buffer(),
                WMT_RESOURCE_USAGE_READ,
                WMT_RENDER_STAGE_FRAGMENT
            ),
            Rec::Use(
                read_only.buffer(),
                WMT_RESOURCE_USAGE_READ,
                WMT_RENDER_STAGE_FRAGMENT
            ),
            offset(),
            offset(),
        ],
        "one use per buffer and pass, the read-only one's too"
    );
    assert!(
        encoder.pass().unwrap().fence_wait.test(256),
        "the read waits for the compute pass's write"
    );
    let used = |at: &Arc<BufferAllocation>| {
        encoder.buffers_used.map[&Arc::as_ptr(at).addr()]
            .trackers
            .len()
    };
    assert_eq!(
        (used(&written), used(&read_only)),
        (1, 0),
        "a read-only buffer has an entry, with no tracker"
    );
}
