use core::{
    ffi::c_void,
    mem::{ManuallyDrop, align_of, offset_of, size_of},
    ptr,
};

use crate::{
    abi::objects::{
        CommandSignatureDesc, D3D12_INDIRECT_ARGUMENT_TYPE_CONSTANT,
        D3D12_INDIRECT_ARGUMENT_TYPE_CONSTANT_BUFFER_VIEW, D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH,
        D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH_MESH, D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH_RAYS,
        D3D12_INDIRECT_ARGUMENT_TYPE_DRAW, D3D12_INDIRECT_ARGUMENT_TYPE_DRAW_INDEXED,
        D3D12_INDIRECT_ARGUMENT_TYPE_INDEX_BUFFER_VIEW,
        D3D12_INDIRECT_ARGUMENT_TYPE_SHADER_RESOURCE_VIEW,
        D3D12_INDIRECT_ARGUMENT_TYPE_UNORDERED_ACCESS_VIEW,
        D3D12_INDIRECT_ARGUMENT_TYPE_VERTEX_BUFFER_VIEW, DispatchArguments, DrawArguments,
        DrawIndexedArguments, IndexBufferView, IndirectArgumentConstant, IndirectArgumentDesc,
        IndirectArgumentDescUnion, IndirectArgumentRootView, IndirectArgumentVertexBuffer,
        VertexBufferView,
    },
    com::{E_INVALIDARG, IID_ID3D12_COMMAND_SIGNATURE, S_FALSE},
    command_signature,
};

#[test]
fn indirect_structs_as_cpp() {
    assert_eq!(size_of::<DrawArguments>(), 16);
    assert_eq!(align_of::<DrawArguments>(), 4);
    assert_eq!(offset_of!(DrawArguments, vertex_count_per_instance), 0);
    assert_eq!(offset_of!(DrawArguments, instance_count), 4);
    assert_eq!(offset_of!(DrawArguments, start_vertex_location), 8);
    assert_eq!(offset_of!(DrawArguments, start_instance_location), 12);

    assert_eq!(size_of::<DrawIndexedArguments>(), 20);
    assert_eq!(align_of::<DrawIndexedArguments>(), 4);
    assert_eq!(
        offset_of!(DrawIndexedArguments, index_count_per_instance),
        0
    );
    assert_eq!(offset_of!(DrawIndexedArguments, instance_count), 4);
    assert_eq!(offset_of!(DrawIndexedArguments, start_index_location), 8);
    assert_eq!(offset_of!(DrawIndexedArguments, base_vertex_location), 12);
    assert_eq!(
        offset_of!(DrawIndexedArguments, start_instance_location),
        16
    );

    assert_eq!(size_of::<DispatchArguments>(), 12);
    assert_eq!(align_of::<DispatchArguments>(), 4);
    assert_eq!(offset_of!(DispatchArguments, thread_group_count_x), 0);
    assert_eq!(offset_of!(DispatchArguments, thread_group_count_y), 4);
    assert_eq!(offset_of!(DispatchArguments, thread_group_count_z), 8);

    assert_eq!(size_of::<VertexBufferView>(), 16);
    assert_eq!(align_of::<VertexBufferView>(), 8);
    assert_eq!(offset_of!(VertexBufferView, buffer_location), 0);
    assert_eq!(offset_of!(VertexBufferView, size_in_bytes), 8);
    assert_eq!(offset_of!(VertexBufferView, stride_in_bytes), 12);

    assert_eq!(size_of::<IndexBufferView>(), 16);
    assert_eq!(align_of::<IndexBufferView>(), 8);
    assert_eq!(offset_of!(IndexBufferView, buffer_location), 0);
    assert_eq!(offset_of!(IndexBufferView, size_in_bytes), 8);
    assert_eq!(offset_of!(IndexBufferView, format), 12);

    assert_eq!(size_of::<IndirectArgumentVertexBuffer>(), 4);
    assert_eq!(size_of::<IndirectArgumentConstant>(), 12);
    assert_eq!(size_of::<IndirectArgumentRootView>(), 4);
    assert_eq!(size_of::<IndirectArgumentDescUnion>(), 12);
    assert_eq!(size_of::<IndirectArgumentDesc>(), 16);
    assert_eq!(align_of::<IndirectArgumentDesc>(), 4);
    assert_eq!(offset_of!(IndirectArgumentDesc, r#type), 0);
    assert_eq!(offset_of!(IndirectArgumentDesc, u), 4);

    #[cfg(target_pointer_width = "64")]
    {
        assert_eq!(size_of::<CommandSignatureDesc>(), 24);
        assert_eq!(align_of::<CommandSignatureDesc>(), 8);
        assert_eq!(offset_of!(CommandSignatureDesc, byte_stride), 0);
        assert_eq!(offset_of!(CommandSignatureDesc, num_argument_descs), 4);
        assert_eq!(offset_of!(CommandSignatureDesc, p_argument_descs), 8);
        assert_eq!(offset_of!(CommandSignatureDesc, node_mask), 16);
    }
    #[cfg(target_pointer_width = "32")]
    {
        assert_eq!(size_of::<CommandSignatureDesc>(), 16);
        assert_eq!(align_of::<CommandSignatureDesc>(), 4);
        assert_eq!(offset_of!(CommandSignatureDesc, byte_stride), 0);
        assert_eq!(offset_of!(CommandSignatureDesc, num_argument_descs), 4);
        assert_eq!(offset_of!(CommandSignatureDesc, p_argument_descs), 8);
        assert_eq!(offset_of!(CommandSignatureDesc, node_mask), 12);
    }

    assert_eq!(D3D12_INDIRECT_ARGUMENT_TYPE_DRAW, 0);
    assert_eq!(D3D12_INDIRECT_ARGUMENT_TYPE_DRAW_INDEXED, 1);
    assert_eq!(D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH, 2);
    assert_eq!(D3D12_INDIRECT_ARGUMENT_TYPE_VERTEX_BUFFER_VIEW, 3);
    assert_eq!(D3D12_INDIRECT_ARGUMENT_TYPE_INDEX_BUFFER_VIEW, 4);
    assert_eq!(D3D12_INDIRECT_ARGUMENT_TYPE_CONSTANT, 5);
    assert_eq!(D3D12_INDIRECT_ARGUMENT_TYPE_CONSTANT_BUFFER_VIEW, 6);
    assert_eq!(D3D12_INDIRECT_ARGUMENT_TYPE_SHADER_RESOURCE_VIEW, 7);
    assert_eq!(D3D12_INDIRECT_ARGUMENT_TYPE_UNORDERED_ACCESS_VIEW, 8);
    assert_eq!(D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH_RAYS, 9);
    assert_eq!(D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH_MESH, 10);

    // Null descriptor returns E_INVALIDARG.
    // SAFETY: Testing `CreateCommandSignature` null-descriptor validation.
    let hr_null = unsafe {
        command_signature::create_command_signature(
            ptr::null_mut(),
            ptr::null(),
            ptr::null_mut(),
            ptr::from_ref(&IID_ID3D12_COMMAND_SIGNATURE).cast::<c_void>(),
            ptr::null_mut(),
        )
    };
    assert_eq!(hr_null, E_INVALIDARG);

    // Valid descriptor with null output pointer returns S_FALSE.
    let arg = IndirectArgumentDesc {
        r#type: D3D12_INDIRECT_ARGUMENT_TYPE_DRAW,
        u: IndirectArgumentDescUnion {
            vertex_buffer: ManuallyDrop::new(IndirectArgumentVertexBuffer { slot: 0 }),
        },
    };
    let desc = CommandSignatureDesc {
        byte_stride: 16,
        num_argument_descs: 1,
        p_argument_descs: &raw const arg,
        node_mask: 0,
    };
    // SAFETY: `desc` and `arg` are valid stack structs; output pointer is null.
    let hr_probe = unsafe {
        command_signature::create_command_signature(
            ptr::null_mut(),
            (&raw const desc).cast(),
            ptr::null_mut(),
            ptr::from_ref(&IID_ID3D12_COMMAND_SIGNATURE).cast::<c_void>(),
            ptr::null_mut(),
        )
    };
    assert_eq!(hr_probe, S_FALSE);
}
