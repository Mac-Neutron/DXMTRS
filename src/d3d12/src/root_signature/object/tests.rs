use core::{
    ffi::c_void,
    mem::ManuallyDrop,
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};
use std::{
    io::Write,
    path::PathBuf,
    sync::{Mutex, PoisonError},
};

use winemetal_wire::{
    WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_BORDER_COLOR, WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
    WMT_SAMPLER_ADDRESS_MODE_REPEAT, WMT_SAMPLER_BORDER_COLOR_OPAQUE_BLACK,
    WMT_SAMPLER_BORDER_COLOR_TRANSPARENT_BLACK, WMT_SAMPLER_MIN_MAG_FILTER_LINEAR,
    WMT_SAMPLER_MIN_MAG_FILTER_NEAREST, WMTSamplerInfo,
};

use crate::{
    D3D12CreateRootSignatureDeserializer, D3D12CreateVersionedRootSignatureDeserializer,
    D3D12SerializeRootSignature, D3D12SerializeVersionedRootSignature,
    abi::{
        root_signature::{
            D3D_ROOT_SIGNATURE_VERSION_1_0, D3D_ROOT_SIGNATURE_VERSION_1_1,
            D3D12_DESCRIPTOR_RANGE_FLAG_DATA_VOLATILE,
            D3D12_DESCRIPTOR_RANGE_FLAG_DESCRIPTORS_VOLATILE, D3D12_DESCRIPTOR_RANGE_TYPE_SAMPLER,
            D3D12_DESCRIPTOR_RANGE_TYPE_SRV, D3D12_ROOT_DESCRIPTOR_FLAG_DATA_VOLATILE,
            D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS, D3D12_ROOT_PARAMETER_TYPE_CBV,
            D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE, DescriptorRange, DescriptorRange1,
            RootConstants, RootDescriptor, RootDescriptorTable, RootDescriptorTable1,
            RootParameter, RootParameter1, RootParameter1Payload, RootParameterPayload,
            RootSignatureDesc, RootSignatureDesc1, StaticSamplerDesc, VersionedDesc,
            VersionedRootSignatureDesc,
        },
        sampler::{
            STATIC_BORDER_COLOR_OPAQUE_BLACK, STATIC_BORDER_COLOR_TRANSPARENT_BLACK,
            TEXTURE_ADDRESS_MODE_BORDER, TEXTURE_ADDRESS_MODE_CLAMP, TEXTURE_ADDRESS_MODE_WRAP,
        },
        vtbl::{ID3D12RootSignatureDeserializerVtbl, ID3D12VersionedRootSignatureDeserializerVtbl},
    },
    com::{
        self, E_INVALIDARG, IID_ID3D12_ROOT_SIGNATURE, IID_ID3D12_ROOT_SIGNATURE_DESERIALIZER,
        IID_ID3D12_VERSIONED_ROOT_SIGNATURE_DESERIALIZER, S_OK,
    },
};

static SAMPLER_LOCK: Mutex<()> = Mutex::new(());
static NEXT_SAMPLER_ID: AtomicU64 = AtomicU64::new(1);
static SAMPLER_INFOS: Mutex<Vec<WMTSamplerInfo>> = Mutex::new(Vec::new());

#[unsafe(export_name = "MTLDevice_newSamplerState")]
extern "C-unwind" fn fake_new_sampler_state(_device: u64, info: *mut WMTSamplerInfo) -> u64 {
    let id = NEXT_SAMPLER_ID.fetch_add(1, Ordering::Relaxed);
    // SAFETY: `Sampler::create` passes a valid mutable `WMTSamplerInfo` pointer.
    let info_ref = unsafe { &mut *info };
    info_ref.gpu_resource_id = 0x9000 + id;
    SAMPLER_INFOS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(*info_ref);
    0x5000 + id
}

const SMITE_GOLDEN: &str = "\
rs-01b171aa16dc6474.bin hr=0x00000000 upload=6 params=5 slots=[0,1,2,3,4] heap=-1 lock=5 samplers=6
rs-02f6bc0a2b6326c4.bin hr=0x00000000 upload=9 params=8 slots=[0,1,2,3,4,5,6,7] heap=-1 lock=8 samplers=6
rs-05dff85d410b6bb3.bin hr=0x00000000 upload=7 params=6 slots=[0,1,2,3,4,5] heap=-1 lock=6 samplers=6
rs-06ee4be130cf2131.bin hr=0x00000000 upload=5 params=4 slots=[0,1,2,3] heap=-1 lock=4 samplers=6
rs-09ba8c761ee5d08d.bin hr=0x00000000 upload=7 params=6 slots=[0,1,2,3,4,5] heap=-1 lock=6 samplers=6
rs-0d3dea5fe36f5701.bin hr=0x00000000 upload=6 params=5 slots=[0,1,2,3,4] heap=-1 lock=5 samplers=6
rs-14df4db38e330952.bin hr=0x00000000 upload=6 params=5 slots=[0,1,2,3,4] heap=-1 lock=5 samplers=6
rs-156f5f99cd1b1c24.bin hr=0x00000000 upload=8 params=7 slots=[0,1,2,3,4,5,6] heap=-1 lock=7 samplers=6
rs-17c93b0917d93d69.bin hr=0x00000000 upload=15 params=14 slots=[0,1,2,3,4,5,6,7,8,9,10,11,12,13] heap=-1 lock=14 samplers=6
rs-1baaa7845520141e.bin hr=0x00000000 upload=13 params=12 slots=[0,1,2,3,4,5,6,7,8,9,10,11] heap=-1 lock=12 samplers=6
rs-1ed983bbab33a35a.bin hr=0x00000000 upload=7 params=6 slots=[0,1,2,3,4,5] heap=-1 lock=6 samplers=6
rs-1f0f4c7086077d85.bin hr=0x00000000 upload=11 params=10 slots=[0,1,2,3,4,5,6,7,8,9] heap=-1 lock=10 samplers=6
rs-2208838a1b927821.bin hr=0x00000000 upload=3 params=2 slots=[0,1] heap=-1 lock=2 samplers=6
rs-24b2d996744f8fcb.bin hr=0x00000000 upload=8 params=7 slots=[0,1,2,3,4,5,6] heap=-1 lock=7 samplers=6
rs-24c4e41e882b602d.bin hr=0x00000000 upload=9 params=8 slots=[0,1,2,3,4,5,6,7] heap=-1 lock=8 samplers=6
rs-24d0316c6c87fa51.bin hr=0x00000000 upload=3 params=2 slots=[0,1] heap=-1 lock=2 samplers=6
rs-25e26eb85f51ea01.bin hr=0x00000000 upload=9 params=8 slots=[0,1,2,3,4,5,6,7] heap=-1 lock=8 samplers=6
rs-2917c1333ad9359e.bin hr=0x00000000 upload=15 params=14 slots=[0,1,2,3,4,5,6,7,8,9,10,11,12,13] heap=-1 lock=14 samplers=6
rs-292dee701ff2b51a.bin hr=0x00000000 upload=11 params=10 slots=[0,1,2,3,4,5,6,7,8,9] heap=-1 lock=10 samplers=6
rs-2b7eeb1056f892cc.bin hr=0x00000000 upload=11 params=10 slots=[0,1,2,3,4,5,6,7,8,9] heap=-1 lock=10 samplers=6
rs-2b7f7eea5454d4e5.bin hr=0x00000000 upload=6 params=5 slots=[0,1,2,3,4] heap=-1 lock=5 samplers=6
rs-2bd212fc61ca4c88.bin hr=0x00000000 upload=10 params=9 slots=[0,1,2,3,4,5,6,7,8] heap=-1 lock=9 samplers=6
rs-2cf95fed0d611a4f.bin hr=0x00000000 upload=8 params=7 slots=[0,1,2,3,4,5,6] heap=-1 lock=7 samplers=6
rs-2d0d64df9e55f087.bin hr=0x00000000 upload=5 params=4 slots=[0,1,2,3] heap=-1 lock=4 samplers=6
rs-32c3a3551c6d5e20.bin hr=0x00000000 upload=14 params=13 slots=[0,1,2,3,4,5,6,7,8,9,10,11,12] heap=-1 lock=13 samplers=6
rs-33170d68b91e39c6.bin hr=0x00000000 upload=9 params=8 slots=[0,1,2,3,4,5,6,7] heap=-1 lock=8 samplers=6
rs-3342957bbe39c882.bin hr=0x00000000 upload=12 params=11 slots=[0,1,2,3,4,5,6,7,8,9,10] heap=-1 lock=11 samplers=6
rs-3372ec3052f9cbe2.bin hr=0x00000000 upload=12 params=11 slots=[0,1,2,3,4,5,6,7,8,9,10] heap=-1 lock=11 samplers=6
rs-39a1d1807254c7cb.bin hr=0x00000000 upload=2 params=1 slots=[0] heap=-1 lock=1 samplers=6
rs-3a99147cdd0e7637.bin hr=0x00000000 upload=9 params=8 slots=[0,1,2,3,4,5,6,7] heap=-1 lock=8 samplers=6
rs-3b0f6cdd4f0c41f2.bin hr=0x00000000 upload=5 params=4 slots=[0,1,2,3] heap=-1 lock=4 samplers=6
rs-3d0b230af4fc2a26.bin hr=0x00000000 upload=13 params=12 slots=[0,1,2,3,4,5,6,7,8,9,10,11] heap=-1 lock=12 samplers=6
rs-3d5331f272e1ce46.bin hr=0x00000000 upload=15 params=14 slots=[0,1,2,3,4,5,6,7,8,9,10,11,12,13] heap=-1 lock=14 samplers=6
rs-409882a927f53c51.bin hr=0x00000000 upload=14 params=13 slots=[0,1,2,3,4,5,6,7,8,9,10,11,12] heap=-1 lock=13 samplers=6
rs-40d4afc3643a20dc.bin hr=0x00000000 upload=8 params=7 slots=[0,1,2,3,4,5,6] heap=-1 lock=7 samplers=6
rs-41754924ff9bcb49.bin hr=0x00000000 upload=6 params=5 slots=[0,1,2,3,4] heap=-1 lock=5 samplers=6
rs-48748d079739d613.bin hr=0x00000000 upload=4 params=3 slots=[0,1,2] heap=-1 lock=3 samplers=6
rs-4a0e6518e7c205a0.bin hr=0x00000000 upload=4 params=3 slots=[0,1,2] heap=-1 lock=3 samplers=6
rs-4c16aea3d2a4fc22.bin hr=0x00000000 upload=9 params=8 slots=[0,1,2,3,4,5,6,7] heap=-1 lock=8 samplers=6
rs-50db067c380d930e.bin hr=0x00000000 upload=9 params=8 slots=[0,1,2,3,4,5,6,7] heap=-1 lock=8 samplers=6
rs-510b4df0981eb575.bin hr=0x00000000 upload=11 params=10 slots=[0,1,2,3,4,5,6,7,8,9] heap=-1 lock=10 samplers=6
rs-51d3524a49179ab0.bin hr=0x00000000 upload=15 params=14 slots=[0,1,2,3,4,5,6,7,8,9,10,11,12,13] heap=-1 lock=14 samplers=6
rs-54794b7282702884.bin hr=0x00000000 upload=9 params=8 slots=[0,1,2,3,4,5,6,7] heap=-1 lock=8 samplers=6
rs-56ed1aa865e209c8.bin hr=0x00000000 upload=9 params=8 slots=[0,1,2,3,4,5,6,7] heap=-1 lock=8 samplers=6
rs-57203c7a570b09ce.bin hr=0x00000000 upload=8 params=7 slots=[0,1,2,3,4,5,6] heap=-1 lock=7 samplers=6
rs-57b1802c9ea57a48.bin hr=0x00000000 upload=7 params=6 slots=[0,1,2,3,4,5] heap=-1 lock=6 samplers=6
rs-5a54224c50749ea2.bin hr=0x00000000 upload=7 params=6 slots=[0,1,2,3,4,5] heap=-1 lock=6 samplers=6
rs-5dd0de5eb13e26a0.bin hr=0x00000000 upload=12 params=11 slots=[0,1,2,3,4,5,6,7,8,9,10] heap=-1 lock=11 samplers=6
rs-61d0c827e34cb69e.bin hr=0x00000000 upload=13 params=12 slots=[0,1,2,3,4,5,6,7,8,9,10,11] heap=-1 lock=12 samplers=6
rs-627aaad18662d8f2.bin hr=0x00000000 upload=7 params=6 slots=[0,1,2,3,4,5] heap=-1 lock=6 samplers=6
rs-64bd5dca89ead4c6.bin hr=0x00000000 upload=9 params=8 slots=[0,1,2,3,4,5,6,7] heap=-1 lock=8 samplers=6
rs-67fa3b24742f60dc.bin hr=0x00000000 upload=12 params=11 slots=[0,1,2,3,4,5,6,7,8,9,10] heap=-1 lock=11 samplers=6
rs-686615ee3fd765eb.bin hr=0x00000000 upload=6 params=5 slots=[0,1,2,3,4] heap=-1 lock=5 samplers=6
rs-688ee5677d84b424.bin hr=0x00000000 upload=10 params=9 slots=[0,1,2,3,4,5,6,7,8] heap=-1 lock=9 samplers=6
rs-68ea94a8d5bf4b16.bin hr=0x00000000 upload=7 params=6 slots=[0,1,2,3,4,5] heap=-1 lock=6 samplers=6
rs-6a10c0ee8deebd4f.bin hr=0x00000000 upload=5 params=4 slots=[0,1,2,3] heap=-1 lock=4 samplers=6
rs-6b1567f608c233d0.bin hr=0x00000000 upload=9 params=8 slots=[0,1,2,3,4,5,6,7] heap=-1 lock=8 samplers=6
rs-6dc78acbf40d7c06.bin hr=0x00000000 upload=9 params=8 slots=[0,1,2,3,4,5,6,7] heap=-1 lock=8 samplers=6
rs-6f9e72d7672de1b5.bin hr=0x00000000 upload=5 params=4 slots=[0,1,2,3] heap=-1 lock=4 samplers=6
rs-6fb2d640084475e9.bin hr=0x00000000 upload=10 params=9 slots=[0,1,2,3,4,5,6,7,8] heap=-1 lock=9 samplers=6
rs-7020a5e5547341ad.bin hr=0x00000000 upload=4 params=3 slots=[0,1,2] heap=-1 lock=3 samplers=6
rs-7090625e971c46f4.bin hr=0x00000000 upload=9 params=8 slots=[0,1,2,3,4,5,6,7] heap=-1 lock=8 samplers=6
rs-71d5371153ce3606.bin hr=0x00000000 upload=14 params=13 slots=[0,1,2,3,4,5,6,7,8,9,10,11,12] heap=-1 lock=13 samplers=6
rs-721e6ea003397cac.bin hr=0x00000000 upload=14 params=13 slots=[0,1,2,3,4,5,6,7,8,9,10,11,12] heap=-1 lock=13 samplers=6
rs-734f895ecacf421a.bin hr=0x00000000 upload=9 params=8 slots=[0,1,2,3,4,5,6,7] heap=-1 lock=8 samplers=6
rs-758ea24e74ddaba9.bin hr=0x00000000 upload=15 params=14 slots=[0,1,2,3,4,5,6,7,8,9,10,11,12,13] heap=-1 lock=14 samplers=6
rs-768896a8e92a10cb.bin hr=0x00000000 upload=6 params=5 slots=[0,1,2,3,4] heap=-1 lock=5 samplers=6
rs-7a7e797da74ed89b.bin hr=0x00000000 upload=16 params=15 slots=[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14] heap=-1 lock=15 samplers=6
rs-7ca57f3011598f7f.bin hr=0x00000000 upload=7 params=6 slots=[0,1,2,3,4,5] heap=-1 lock=6 samplers=6
rs-81bf39e8cc941ceb.bin hr=0x00000000 upload=9 params=8 slots=[0,1,2,3,4,5,6,7] heap=-1 lock=8 samplers=6
rs-820f55ac3f9f7f20.bin hr=0x00000000 upload=6 params=5 slots=[0,1,2,3,4] heap=-1 lock=5 samplers=6
rs-84b8b17522a4635c.bin hr=0x00000000 upload=4 params=3 slots=[0,1,2] heap=-1 lock=3 samplers=6
rs-857bf0ba468c2e5c.bin hr=0x00000000 upload=9 params=8 slots=[0,1,2,3,4,5,6,7] heap=-1 lock=8 samplers=6
rs-87d938929cd18151.bin hr=0x00000000 upload=6 params=5 slots=[0,1,2,3,4] heap=-1 lock=5 samplers=6
rs-88278129b0ebd30b.bin hr=0x00000000 upload=2 params=1 slots=[0] heap=-1 lock=1 samplers=6
rs-8bd7c5a339b67d03.bin hr=0x00000000 upload=9 params=8 slots=[0,1,2,3,4,5,6,7] heap=-1 lock=8 samplers=6
rs-8de863d0d0264fea.bin hr=0x00000000 upload=16 params=15 slots=[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14] heap=-1 lock=15 samplers=6
rs-915e88cb2709ee7c.bin hr=0x00000000 upload=11 params=10 slots=[0,1,2,3,4,5,6,7,8,9] heap=-1 lock=10 samplers=6
rs-9516e771c523f261.bin hr=0x00000000 upload=13 params=12 slots=[0,1,2,3,4,5,6,7,8,9,10,11] heap=-1 lock=12 samplers=6
rs-956b25a8417ce71d.bin hr=0x00000000 upload=16 params=15 slots=[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14] heap=-1 lock=15 samplers=6
rs-97c6aafc761e4813.bin hr=0x00000000 upload=10 params=9 slots=[0,1,2,3,4,5,6,7,8] heap=-1 lock=9 samplers=6
rs-9a5c1fe47e0d8e9c.bin hr=0x00000000 upload=8 params=7 slots=[0,1,2,3,4,5,6] heap=-1 lock=7 samplers=6
rs-9ae5243b76c778e6.bin hr=0x00000000 upload=8 params=7 slots=[0,1,2,3,4,5,6] heap=-1 lock=7 samplers=6
rs-9c194f66f13a781f.bin hr=0x00000000 upload=1 params=0 slots=[] heap=-1 lock=0 samplers=6
rs-9da1e703b9cc0400.bin hr=0x00000000 upload=10 params=9 slots=[0,1,2,3,4,5,6,7,8] heap=-1 lock=9 samplers=6
rs-9f823477614d161f.bin hr=0x00000000 upload=12 params=11 slots=[0,1,2,3,4,5,6,7,8,9,10] heap=-1 lock=11 samplers=6
rs-a2519c2a0c0d075a.bin hr=0x00000000 upload=8 params=7 slots=[0,1,2,3,4,5,6] heap=-1 lock=7 samplers=6
rs-a4199c987f210a65.bin hr=0x00000000 upload=10 params=9 slots=[0,1,2,3,4,5,6,7,8] heap=-1 lock=9 samplers=6
rs-a423eb294332e669.bin hr=0x00000000 upload=10 params=9 slots=[0,1,2,3,4,5,6,7,8] heap=-1 lock=9 samplers=6
rs-a4c0f76e50829ba5.bin hr=0x00000000 upload=8 params=7 slots=[0,1,2,3,4,5,6] heap=-1 lock=7 samplers=6
rs-a6183045692e84bb.bin hr=0x00000000 upload=3 params=2 slots=[0,1] heap=-1 lock=2 samplers=6
rs-a6ae4de76472e9b0.bin hr=0x00000000 upload=11 params=10 slots=[0,1,2,3,4,5,6,7,8,9] heap=-1 lock=10 samplers=6
rs-a9fbf01f03c294dc.bin hr=0x00000000 upload=14 params=13 slots=[0,1,2,3,4,5,6,7,8,9,10,11,12] heap=-1 lock=13 samplers=6
rs-aa8a572c156b301b.bin hr=0x00000000 upload=10 params=9 slots=[0,1,2,3,4,5,6,7,8] heap=-1 lock=9 samplers=6
rs-aaf002be44eb8fff.bin hr=0x00000000 upload=8 params=7 slots=[0,1,2,3,4,5,6] heap=-1 lock=7 samplers=6
rs-ac3d193a13735349.bin hr=0x00000000 upload=10 params=9 slots=[0,1,2,3,4,5,6,7,8] heap=-1 lock=9 samplers=6
rs-ac45fbeecab6dedf.bin hr=0x00000000 upload=8 params=7 slots=[0,1,2,3,4,5,6] heap=-1 lock=7 samplers=6
rs-adaabfb0d61a2e11.bin hr=0x00000000 upload=11 params=10 slots=[0,1,2,3,4,5,6,7,8,9] heap=-1 lock=10 samplers=6
rs-adff380dc2154610.bin hr=0x00000000 upload=13 params=12 slots=[0,1,2,3,4,5,6,7,8,9,10,11] heap=-1 lock=12 samplers=6
rs-ae166daa6c535ec3.bin hr=0x00000000 upload=3 params=2 slots=[0,1] heap=-1 lock=2 samplers=6
rs-ae77d95c6718c594.bin hr=0x00000000 upload=5 params=4 slots=[0,1,2,3] heap=-1 lock=4 samplers=6
rs-af04824f189141c7.bin hr=0x00000000 upload=8 params=7 slots=[0,1,2,3,4,5,6] heap=-1 lock=7 samplers=6
rs-afe89c7dbc3e2c3d.bin hr=0x00000000 upload=11 params=10 slots=[0,1,2,3,4,5,6,7,8,9] heap=-1 lock=10 samplers=6
rs-b22b97d8de2fe01f.bin hr=0x00000000 upload=9 params=8 slots=[0,1,2,3,4,5,6,7] heap=-1 lock=8 samplers=6
rs-b2cc97223980a08d.bin hr=0x00000000 upload=4 params=3 slots=[0,1,2] heap=-1 lock=3 samplers=6
rs-b39ec1b36f917f9b.bin hr=0x00000000 upload=11 params=10 slots=[0,1,2,3,4,5,6,7,8,9] heap=-1 lock=10 samplers=6
rs-b4131d2db9467d5a.bin hr=0x00000000 upload=12 params=11 slots=[0,1,2,3,4,5,6,7,8,9,10] heap=-1 lock=11 samplers=6
rs-b5075de79ea1ae14.bin hr=0x00000000 upload=4 params=3 slots=[0,1,2] heap=-1 lock=3 samplers=6
rs-b56854410eb4d195.bin hr=0x00000000 upload=6 params=5 slots=[0,1,2,3,4] heap=-1 lock=5 samplers=6
rs-b6d4045ed785e809.bin hr=0x00000000 upload=13 params=12 slots=[0,1,2,3,4,5,6,7,8,9,10,11] heap=-1 lock=12 samplers=6
rs-ba0e226e50e2a38c.bin hr=0x00000000 upload=10 params=9 slots=[0,1,2,3,4,5,6,7,8] heap=-1 lock=9 samplers=6
rs-bb84702ee75884f1.bin hr=0x00000000 upload=7 params=6 slots=[0,1,2,3,4,5] heap=-1 lock=6 samplers=6
rs-bc1e10c98979987a.bin hr=0x00000000 upload=5 params=4 slots=[0,1,2,3] heap=-1 lock=4 samplers=6
rs-bd167953fca3cb8e.bin hr=0x00000000 upload=9 params=8 slots=[0,1,2,3,4,5,6,7] heap=-1 lock=8 samplers=6
rs-c0b79c3d529060d8.bin hr=0x00000000 upload=10 params=9 slots=[0,1,2,3,4,5,6,7,8] heap=-1 lock=9 samplers=6
rs-c230fe8fa204d9f9.bin hr=0x00000000 upload=6 params=5 slots=[0,1,2,3,4] heap=-1 lock=5 samplers=6
rs-c5b55af21ed3180a.bin hr=0x00000000 upload=5 params=4 slots=[0,1,2,3] heap=-1 lock=4 samplers=6
rs-c5ecf99616262c07.bin hr=0x00000000 upload=8 params=7 slots=[0,1,2,3,4,5,6] heap=-1 lock=7 samplers=6
rs-c893a24654d9de29.bin hr=0x00000000 upload=12 params=11 slots=[0,1,2,3,4,5,6,7,8,9,10] heap=-1 lock=11 samplers=6
rs-cc60010d274b06bd.bin hr=0x00000000 upload=8 params=7 slots=[0,1,2,3,4,5,6] heap=-1 lock=7 samplers=6
rs-ccf181d528fc09cf.bin hr=0x00000000 upload=13 params=12 slots=[0,1,2,3,4,5,6,7,8,9,10,11] heap=-1 lock=12 samplers=6
rs-cfe7a9d1517539a3.bin hr=0x00000000 upload=6 params=5 slots=[0,1,2,3,4] heap=-1 lock=5 samplers=6
rs-d059de9e9b489e65.bin hr=0x00000000 upload=3 params=2 slots=[0,1] heap=-1 lock=2 samplers=6
rs-d31abcfd033eaaa6.bin hr=0x00000000 upload=17 params=16 slots=[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15] heap=-1 lock=16 samplers=6
rs-d3bafad8e91bf690.bin hr=0x00000000 upload=4 params=3 slots=[0,1,2] heap=-1 lock=3 samplers=6
rs-d71d4747c45824fb.bin hr=0x00000000 upload=14 params=13 slots=[0,1,2,3,4,5,6,7,8,9,10,11,12] heap=-1 lock=13 samplers=6
rs-d8ae691e2aa48a06.bin hr=0x00000000 upload=7 params=6 slots=[0,1,2,3,4,5] heap=-1 lock=6 samplers=6
rs-da1f4646e6965ecb.bin hr=0x00000000 upload=9 params=8 slots=[0,1,2,3,4,5,6,7] heap=-1 lock=8 samplers=6
rs-dbe7eac8511eaee4.bin hr=0x00000000 upload=10 params=9 slots=[0,1,2,3,4,5,6,7,8] heap=-1 lock=9 samplers=6
rs-dcc1d97c6e5af654.bin hr=0x00000000 upload=5 params=4 slots=[0,1,2,3] heap=-1 lock=4 samplers=6
rs-dea60e55948a7377.bin hr=0x00000000 upload=11 params=10 slots=[0,1,2,3,4,5,6,7,8,9] heap=-1 lock=10 samplers=6
rs-e161857fe5baa509.bin hr=0x00000000 upload=7 params=6 slots=[0,1,2,3,4,5] heap=-1 lock=6 samplers=6
rs-e2594cffd42f40cd.bin hr=0x00000000 upload=2 params=1 slots=[0] heap=-1 lock=1 samplers=6
rs-e4c304b5b1090fd9.bin hr=0x00000000 upload=14 params=13 slots=[0,1,2,3,4,5,6,7,8,9,10,11,12] heap=-1 lock=13 samplers=6
rs-e5617e0c9d423f83.bin hr=0x00000000 upload=10 params=9 slots=[0,1,2,3,4,5,6,7,8] heap=-1 lock=9 samplers=6
rs-e5671840589ca84d.bin hr=0x00000000 upload=8 params=7 slots=[0,1,2,3,4,5,6] heap=-1 lock=7 samplers=6
rs-e95353bff2d66b1d.bin hr=0x00000000 upload=10 params=9 slots=[0,1,2,3,4,5,6,7,8] heap=-1 lock=9 samplers=6
rs-eab1703ec901b853.bin hr=0x00000000 upload=5 params=4 slots=[0,1,2,3] heap=-1 lock=4 samplers=6
rs-eac8635df7213dee.bin hr=0x00000000 upload=7 params=6 slots=[0,1,2,3,4,5] heap=-1 lock=6 samplers=6
rs-eb5afd764ab672d3.bin hr=0x00000000 upload=8 params=7 slots=[0,1,2,3,4,5,6] heap=-1 lock=7 samplers=6
rs-ec6c3524d9c202aa.bin hr=0x00000000 upload=8 params=7 slots=[0,1,2,3,4,5,6] heap=-1 lock=7 samplers=6
rs-ec954a7722d8ca1d.bin hr=0x00000000 upload=3 params=2 slots=[0,1] heap=-1 lock=2 samplers=6
rs-edc719b3683e3a0e.bin hr=0x00000000 upload=7 params=6 slots=[0,1,2,3,4,5] heap=-1 lock=6 samplers=6
rs-f0ee134df5058fb2.bin hr=0x00000000 upload=8 params=7 slots=[0,1,2,3,4,5,6] heap=-1 lock=7 samplers=6
rs-f4a67481c6663bbe.bin hr=0x00000000 upload=8 params=7 slots=[0,1,2,3,4,5,6] heap=-1 lock=7 samplers=6
rs-f4a7b402e097f071.bin hr=0x00000000 upload=9 params=8 slots=[0,1,2,3,4,5,6,7] heap=-1 lock=8 samplers=6
rs-f533504a454c027a.bin hr=0x00000000 upload=5 params=4 slots=[0,1,2,3] heap=-1 lock=4 samplers=6
rs-f54fe3ce66761ec6.bin hr=0x00000000 upload=13 params=12 slots=[0,1,2,3,4,5,6,7,8,9,10,11] heap=-1 lock=12 samplers=6
rs-f9f37210a798f982.bin hr=0x00000000 upload=2 params=1 slots=[0] heap=-1 lock=1 samplers=0
rs-fcfbaa72666c9e78.bin hr=0x00000000 upload=4 params=3 slots=[0,1,2] heap=-1 lock=3 samplers=6";

#[test]
fn smite_root_signatures_as_cpp() {
    let _guard = SAMPLER_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let empty_desc = VersionedRootSignatureDesc {
        version: D3D_ROOT_SIGNATURE_VERSION_1_0,
        desc: VersionedDesc {
            desc_1_0: ManuallyDrop::new(RootSignatureDesc {
                num_parameters: 0,
                p_parameters: ptr::null(),
                num_static_samplers: 0,
                p_static_samplers: ptr::null(),
                flags: 0,
            }),
        },
    };
    // SAFETY: `empty_desc` has valid (null, 0-length) pointers.
    let empty_blob = unsafe { crate::root_signature::serialize_versioned(&empty_desc) }
        .expect("empty root signature serializes");
    let mut empty_out: *mut c_void = ptr::null_mut();
    // SAFETY: `empty_blob` is valid for its length; `empty_out` is a writable out-pointer.
    let empty_hr = unsafe {
        super::create_root_signature(
            ptr::null_mut(),
            0,
            empty_blob.as_ptr().cast(),
            empty_blob.len(),
            ptr::from_ref(&IID_ID3D12_ROOT_SIGNATURE).cast(),
            &raw mut empty_out,
        )
    };
    assert_eq!(empty_hr, S_OK);
    assert!(!empty_out.is_null());
    // SAFETY: `empty_out` is a live `RootSignature` returned by `create_root_signature`.
    unsafe { com::release_com::<super::RootSignature>(empty_out.cast()) };

    let dir = std::env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join("Library/Application Support/MacNeutron/vm.0.7.2/runs/2437170/traces/dxmt")
    });
    let Some(dir) = dir.filter(|p| p.is_dir()) else {
        let _ = std::io::stderr()
            .lock()
            .write_all(b"smite_root_signatures_as_cpp: skipped (SMITE 2 capture folder absent)\n");
        return;
    };

    for expected_line in SMITE_GOLDEN.lines() {
        let name = expected_line
            .split_whitespace()
            .next()
            .expect("golden line starts with filename");
        let bytes = std::fs::read(dir.join(name)).expect("read SMITE 2 rs-*.bin blob");
        let mut out: *mut c_void = ptr::null_mut();
        // SAFETY: `bytes` is valid for its length and `out` is a valid out-pointer.
        let hr = unsafe {
            super::create_root_signature(
                ptr::null_mut(),
                0,
                bytes.as_ptr().cast(),
                bytes.len(),
                ptr::from_ref(&IID_ID3D12_ROOT_SIGNATURE).cast(),
                &raw mut out,
            )
        };
        assert_eq!(hr, S_OK, "{name}");
        // SAFETY: `out` was created by `create_root_signature`.
        let rs = unsafe { super::cast_root_signature(out) }.expect("cast RootSignature");
        let slots: Vec<String> = rs
            .slot_qword_offsets()
            .iter()
            .map(ToString::to_string)
            .collect();
        let heap = if rs.heap_qword_offset() == !0 {
            -1
        } else {
            i32::try_from(rs.heap_qword_offset()).unwrap_or(-1)
        };
        let samplers = rs.encoded_static_samplers().len() / 4;
        let got = format!(
            "{name} hr=0x{:08x} upload={} params={} slots=[{}] heap={heap} lock={} samplers={samplers}",
            hr.cast_unsigned(),
            rs.upload_qwords(),
            rs.parameter_slots(),
            slots.join(","),
            rs.lock_qword_offset(),
        );
        assert_eq!(got, expected_line);
        // SAFETY: `out` is a live `RootSignature` with refcount 1.
        unsafe { com::release_com::<super::RootSignature>(out.cast()) };
    }
}

#[test]
fn mixed_sampler_table_refused() {
    let ranges = [
        DescriptorRange1 {
            range_type: D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
            num_descriptors: 1,
            base_shader_register: 0,
            register_space: 0,
            flags: 0,
            offset_in_descriptors_from_table_start: 0,
        },
        DescriptorRange1 {
            range_type: D3D12_DESCRIPTOR_RANGE_TYPE_SAMPLER,
            num_descriptors: 1,
            base_shader_register: 0,
            register_space: 0,
            flags: 0,
            offset_in_descriptors_from_table_start: 1,
        },
    ];
    let param = RootParameter1 {
        parameter_type: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
        payload: RootParameter1Payload {
            descriptor_table: ManuallyDrop::new(RootDescriptorTable1 {
                num_descriptor_ranges: 2,
                p_descriptor_ranges: ranges.as_ptr(),
            }),
        },
        shader_visibility: 0,
    };
    let desc = VersionedRootSignatureDesc {
        version: D3D_ROOT_SIGNATURE_VERSION_1_1,
        desc: VersionedDesc {
            desc_1_1: ManuallyDrop::new(RootSignatureDesc1 {
                num_parameters: 1,
                p_parameters: &raw const param,
                num_static_samplers: 0,
                p_static_samplers: ptr::null(),
                flags: 0,
            }),
        },
    };
    // SAFETY: `desc` points to stack arrays valid for their declared counts.
    let blob = unsafe { crate::root_signature::serialize_versioned(&desc) }
        .expect("mixed table serializes");
    let mut out: *mut c_void = ptr::dangling_mut();
    // SAFETY: `blob` is valid for its length and `out` is a writable out-pointer.
    let hr = unsafe {
        super::create_root_signature(
            ptr::null_mut(),
            0,
            blob.as_ptr().cast(),
            blob.len(),
            ptr::from_ref(&IID_ID3D12_ROOT_SIGNATURE).cast(),
            &raw mut out,
        )
    };
    assert_eq!(hr, E_INVALIDARG);
    assert!(out.is_null());
}

#[test]
fn static_samplers_as_cpp() {
    let _guard = SAMPLER_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    SAMPLER_INFOS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    NEXT_SAMPLER_ID.store(1, Ordering::Relaxed);

    let samplers = [
        StaticSamplerDesc {
            filter: 0x15,
            address_u: TEXTURE_ADDRESS_MODE_WRAP,
            address_v: TEXTURE_ADDRESS_MODE_CLAMP,
            address_w: TEXTURE_ADDRESS_MODE_BORDER,
            mip_lod_bias: 0.75,
            max_anisotropy: 4,
            comparison_func: 2,
            border_color: STATIC_BORDER_COLOR_OPAQUE_BLACK,
            min_lod: 0.0,
            max_lod: 1000.0,
            shader_register: 0,
            register_space: 0,
            shader_visibility: 5,
        },
        StaticSamplerDesc {
            filter: 0x00,
            address_u: TEXTURE_ADDRESS_MODE_WRAP,
            address_v: TEXTURE_ADDRESS_MODE_WRAP,
            address_w: TEXTURE_ADDRESS_MODE_WRAP,
            mip_lod_bias: -1.5,
            max_anisotropy: 1,
            comparison_func: 1,
            border_color: STATIC_BORDER_COLOR_TRANSPARENT_BLACK,
            min_lod: 0.25,
            max_lod: 8.0,
            shader_register: 1,
            register_space: 0,
            shader_visibility: 0,
        },
    ];
    let desc = VersionedRootSignatureDesc {
        version: D3D_ROOT_SIGNATURE_VERSION_1_0,
        desc: VersionedDesc {
            desc_1_0: ManuallyDrop::new(RootSignatureDesc {
                num_parameters: 0,
                p_parameters: ptr::null(),
                num_static_samplers: 2,
                p_static_samplers: samplers.as_ptr(),
                flags: 0,
            }),
        },
    };
    // SAFETY: `desc` points to `samplers` of length 2.
    let blob = unsafe { crate::root_signature::serialize_versioned(&desc) }
        .expect("static samplers serialize");
    let mut out: *mut c_void = ptr::null_mut();
    // SAFETY: `blob` is valid for its length and `out` is a writable out-pointer.
    let hr = unsafe {
        super::create_root_signature(
            ptr::null_mut(),
            0,
            blob.as_ptr().cast(),
            blob.len(),
            ptr::from_ref(&IID_ID3D12_ROOT_SIGNATURE).cast(),
            &raw mut out,
        )
    };
    assert_eq!(hr, S_OK);
    assert!(!out.is_null());

    // SAFETY: `out` is a live `RootSignature` returned by `create_root_signature`.
    let rs = unsafe { super::cast_root_signature(out) }.expect("cast RootSignature");
    assert_eq!(
        rs.encoded_static_samplers(),
        &[
            0x9001,
            0x9002,
            u64::from(0.75_f32.to_bits()),
            0,
            0x9003,
            0x9004,
            u64::from((-1.5_f32).to_bits()),
            0,
        ]
    );

    let recorded = SAMPLER_INFOS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    assert_eq!(recorded.len(), 4);
    assert_eq!(recorded[0].min_filter, WMT_SAMPLER_MIN_MAG_FILTER_LINEAR);
    assert_eq!(recorded[0].mag_filter, WMT_SAMPLER_MIN_MAG_FILTER_LINEAR);
    assert_eq!(recorded[0].s_address_mode, WMT_SAMPLER_ADDRESS_MODE_REPEAT);
    assert_eq!(
        recorded[0].t_address_mode,
        WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE
    );
    assert_eq!(
        recorded[0].r_address_mode,
        WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_BORDER_COLOR
    );
    assert_eq!(
        recorded[0].border_color,
        WMT_SAMPLER_BORDER_COLOR_OPAQUE_BLACK
    );
    assert_eq!(
        recorded[1].s_address_mode,
        WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_BORDER_COLOR
    );
    assert_eq!(
        recorded[1].t_address_mode,
        WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_BORDER_COLOR
    );
    assert_eq!(
        recorded[1].r_address_mode,
        WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_BORDER_COLOR
    );
    assert_eq!(recorded[2].min_filter, WMT_SAMPLER_MIN_MAG_FILTER_NEAREST);
    assert_eq!(recorded[2].mag_filter, WMT_SAMPLER_MIN_MAG_FILTER_NEAREST);
    assert_eq!(
        recorded[2].border_color,
        WMT_SAMPLER_BORDER_COLOR_TRANSPARENT_BLACK
    );
    assert_eq!(
        recorded[3].s_address_mode,
        WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE
    );
    assert_eq!(
        recorded[3].t_address_mode,
        WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE
    );
    assert_eq!(
        recorded[3].r_address_mode,
        WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE
    );

    // SAFETY: `out` is a live `RootSignature` with refcount 1.
    unsafe { com::release_com::<super::RootSignature>(out.cast()) };
}

#[test]
fn versioned_deserializer_as_cpp() {
    let range_1_0 = DescriptorRange {
        range_type: D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
        num_descriptors: 4,
        base_shader_register: 2,
        register_space: 1,
        offset_in_descriptors_from_table_start: 0,
    };
    let params_1_0 = [
        RootParameter {
            parameter_type: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
            payload: RootParameterPayload {
                descriptor_table: ManuallyDrop::new(RootDescriptorTable {
                    num_descriptor_ranges: 1,
                    p_descriptor_ranges: &raw const range_1_0,
                }),
            },
            shader_visibility: 1,
        },
        RootParameter {
            parameter_type: D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS,
            payload: RootParameterPayload {
                constants: ManuallyDrop::new(RootConstants {
                    shader_register: 3,
                    register_space: 0,
                    num_32bit_values: 5,
                }),
            },
            shader_visibility: 2,
        },
        RootParameter {
            parameter_type: D3D12_ROOT_PARAMETER_TYPE_CBV,
            payload: RootParameterPayload {
                descriptor: ManuallyDrop::new(RootDescriptor {
                    shader_register: 7,
                    register_space: 4,
                }),
            },
            shader_visibility: 0,
        },
    ];
    let sampler = StaticSamplerDesc {
        filter: 0x15,
        address_u: TEXTURE_ADDRESS_MODE_WRAP,
        address_v: TEXTURE_ADDRESS_MODE_WRAP,
        address_w: TEXTURE_ADDRESS_MODE_WRAP,
        mip_lod_bias: 0.0,
        max_anisotropy: 1,
        comparison_func: 1,
        border_color: STATIC_BORDER_COLOR_OPAQUE_BLACK,
        min_lod: 0.0,
        max_lod: 100.0,
        shader_register: 0,
        register_space: 0,
        shader_visibility: 5,
    };
    let desc_1_0 = RootSignatureDesc {
        num_parameters: 3,
        p_parameters: params_1_0.as_ptr(),
        num_static_samplers: 1,
        p_static_samplers: &raw const sampler,
        flags: 0x1,
    };

    let mut err_ptr: *mut c_void = ptr::dangling_mut();
    let mut bad_blob_ptr: *mut c_void = ptr::null_mut();
    // SAFETY: `desc_1_0`, `bad_blob_ptr`, and `err_ptr` are valid caller pointers; version 99 is invalid.
    let bad_hr = unsafe {
        D3D12SerializeRootSignature(
            (&raw const desc_1_0).cast(),
            99,
            &raw mut bad_blob_ptr,
            &raw mut err_ptr,
        )
    };
    assert_eq!(bad_hr, E_INVALIDARG);
    assert!(err_ptr.is_null());

    // SAFETY: `desc_1_0` points to valid stack arrays.
    let blob_bytes =
        unsafe { crate::root_signature::serialize(&desc_1_0, D3D_ROOT_SIGNATURE_VERSION_1_0) }
            .expect("v1.0 root signature serializes");

    let mut deser_1_0: *mut c_void = ptr::null_mut();
    // SAFETY: `blob_bytes` is valid for its length and `deser_1_0` is a writable out-pointer.
    let hr = unsafe {
        D3D12CreateRootSignatureDeserializer(
            blob_bytes.as_ptr().cast(),
            blob_bytes.len(),
            &IID_ID3D12_ROOT_SIGNATURE_DESERIALIZER,
            &raw mut deser_1_0,
        )
    };
    assert_eq!(hr, S_OK);
    assert!(!deser_1_0.is_null());
    // SAFETY: `deser_1_0` is a valid `ID3D12RootSignatureDeserializer` COM object.
    let vtbl_1_0 = unsafe { com::vtbl::<ID3D12RootSignatureDeserializerVtbl>(deser_1_0) };
    // SAFETY: calling `GetRootSignatureDesc` on a live deserializer.
    let raw_d10 =
        unsafe { (vtbl_1_0.get_root_signature_desc)(deser_1_0) }.cast::<RootSignatureDesc>();
    assert!(!raw_d10.is_null());
    // SAFETY: `raw_d10` is owned by `deser_1_0`.
    let got_d10 = unsafe { &*raw_d10 };
    assert_eq!(got_d10.num_parameters, 3);
    assert_eq!(got_d10.num_static_samplers, 1);
    assert_eq!(got_d10.flags, 0x1);
    // SAFETY: `release` drops the single reference on `deser_1_0`.
    unsafe { (vtbl_1_0.release)(deser_1_0) };

    let mut vdeser: *mut c_void = ptr::null_mut();
    // SAFETY: `blob_bytes` is valid for its length and `vdeser` is a writable out-pointer.
    let hr = unsafe {
        D3D12CreateVersionedRootSignatureDeserializer(
            blob_bytes.as_ptr().cast(),
            blob_bytes.len(),
            &IID_ID3D12_VERSIONED_ROOT_SIGNATURE_DESERIALIZER,
            &raw mut vdeser,
        )
    };
    assert_eq!(hr, S_OK);
    assert!(!vdeser.is_null());
    // SAFETY: `vdeser` is a valid `ID3D12VersionedRootSignatureDeserializer` COM object.
    let vvtbl = unsafe { com::vtbl::<ID3D12VersionedRootSignatureDeserializerVtbl>(vdeser) };
    // SAFETY: calling `GetUnconvertedRootSignatureDesc` on a live versioned deserializer.
    let unconverted = unsafe { (vvtbl.get_unconverted_root_signature_desc)(vdeser) }
        .cast::<VersionedRootSignatureDesc>();
    assert!(!unconverted.is_null());
    // SAFETY: `unconverted` points into `vdeser`.
    let unconverted_ver = unsafe { (*unconverted).version };
    assert_eq!(unconverted_ver, D3D_ROOT_SIGNATURE_VERSION_1_0);

    let mut at_1_0: *const c_void = ptr::null();
    // SAFETY: `vdeser` is live and `at_1_0` is a writable out-pointer.
    let hr = unsafe {
        (vvtbl.get_root_signature_desc_at_version)(
            vdeser,
            D3D_ROOT_SIGNATURE_VERSION_1_0,
            (&raw mut at_1_0).cast(),
        )
    };
    assert_eq!(hr, S_OK);
    assert_eq!(at_1_0.cast::<VersionedRootSignatureDesc>(), unconverted);

    let mut at_1_1: *const c_void = ptr::null();
    // SAFETY: `vdeser` is live and `at_1_1` is a writable out-pointer.
    let hr = unsafe {
        (vvtbl.get_root_signature_desc_at_version)(
            vdeser,
            D3D_ROOT_SIGNATURE_VERSION_1_1,
            (&raw mut at_1_1).cast(),
        )
    };
    assert_eq!(hr, S_OK);
    assert!(!at_1_1.is_null());
    // SAFETY: `at_1_1` points into `vdeser`'s v1.1 converted description.
    let at_1_1_ref = unsafe { &*at_1_1.cast::<VersionedRootSignatureDesc>() };
    // SAFETY: `at_1_1_ref` was returned for `D3D_ROOT_SIGNATURE_VERSION_1_1`.
    let d11: &RootSignatureDesc1 = unsafe { &at_1_1_ref.desc.desc_1_1 };
    assert_eq!(d11.num_parameters, 3);
    assert_eq!(d11.num_static_samplers, 1);
    assert_eq!(d11.flags, 0x1);
    // SAFETY: `d11.p_parameters` has 3 elements.
    let p11 = unsafe { core::slice::from_raw_parts(d11.p_parameters, 3) };
    // SAFETY: parameter 0 is a descriptor table with 1 range.
    let table11 = unsafe { &*p11[0].payload.descriptor_table };
    // SAFETY: `table11` has 1 range.
    let r11 = unsafe { &*table11.p_descriptor_ranges };
    assert_eq!(
        r11.flags,
        D3D12_DESCRIPTOR_RANGE_FLAG_DESCRIPTORS_VOLATILE
            | D3D12_DESCRIPTOR_RANGE_FLAG_DATA_VOLATILE
    );
    // SAFETY: parameter 2 is a root CBV descriptor.
    let cbv11 = unsafe { &*p11[2].payload.descriptor };
    assert_eq!(cbv11.shader_register, 7);
    assert_eq!(cbv11.register_space, 4);
    assert_eq!(cbv11.flags, D3D12_ROOT_DESCRIPTOR_FLAG_DATA_VOLATILE);

    let sentinel: *const c_void = ptr::without_provenance(1);
    let mut bad_out = sentinel;
    // SAFETY: `vdeser` is live; invalid version leaves `bad_out` untouched.
    let hr = unsafe {
        (vvtbl.get_root_signature_desc_at_version)(vdeser, 99, (&raw mut bad_out).cast())
    };
    assert_eq!(hr, E_INVALIDARG);
    assert_eq!(bad_out, sentinel);

    let vdesc_1_1 = VersionedRootSignatureDesc {
        version: D3D_ROOT_SIGNATURE_VERSION_1_1,
        desc: VersionedDesc {
            desc_1_1: ManuallyDrop::new(RootSignatureDesc1 {
                num_parameters: d11.num_parameters,
                p_parameters: d11.p_parameters,
                num_static_samplers: d11.num_static_samplers,
                p_static_samplers: d11.p_static_samplers,
                flags: d11.flags,
            }),
        },
    };
    let mut err_ptr2: *mut c_void = ptr::dangling_mut();
    // SAFETY: `vdesc_1_1` is valid while `vdeser` is alive; null `blob` returns `E_INVALIDARG`.
    let hr = unsafe {
        D3D12SerializeVersionedRootSignature(
            (&raw const vdesc_1_1).cast(),
            ptr::null_mut(),
            &raw mut err_ptr2,
        )
    };
    assert_eq!(hr, E_INVALIDARG);
    assert!(err_ptr2.is_null());

    // SAFETY: `release` drops the single reference on `vdeser`.
    unsafe { (vvtbl.release)(vdeser) };
}
