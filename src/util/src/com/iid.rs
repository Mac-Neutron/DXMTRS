//! Interface IDs of DXGI, Direct3D 10, 11 and 12, which DXMT's DLLs answer `QueryInterface` for.
//!
//! One list for every front end. Each IID has one canonical name (the `d3d11` crate's spelling
//! where it had one). The other spellings the front ends used (`IID_ID3D11_DEVICE1` for
//! `IID_ID3D11_DEVICE_1`) are aliases of the same value, so a crate re-points its `use` without
//! renaming its call sites.

use super::Guid;
pub use super::IID_IUNKNOWN;

// ---- DXGI ----

/// `IID_IDXGIObject`.
pub const IID_IDXGI_OBJECT: Guid = Guid::new(
    0xaec2_2fb8,
    0x76f3,
    0x4639,
    [0x9b, 0xe0, 0x28, 0xeb, 0x43, 0xa6, 0x7a, 0x2e],
);

/// `IID_IDXGIDeviceSubObject`.
pub const IID_IDXGI_DEVICE_SUB_OBJECT: Guid = Guid::new(
    0x3d3e_0379,
    0xf9de,
    0x4d58,
    [0xbb, 0x6c, 0x18, 0xd6, 0x29, 0x92, 0xf1, 0xa6],
);

/// `IID_IDXGIDevice`.
pub const IID_IDXGI_DEVICE: Guid = Guid::new(
    0x54ec_77fa,
    0x1377,
    0x44e6,
    [0x8c, 0x32, 0x88, 0xfd, 0x5f, 0x44, 0xc8, 0x4c],
);

/// `IID_IDXGIDevice1`.
pub const IID_IDXGI_DEVICE_1: Guid = Guid::new(
    0x77db_970f,
    0x6276,
    0x48ba,
    [0xba, 0x28, 0x07, 0x01, 0x43, 0xb4, 0x39, 0x2c],
);

/// Another spelling of [`IID_IDXGI_DEVICE_1`].
pub const IID_IDXGI_DEVICE1: Guid = IID_IDXGI_DEVICE_1;

/// `IID_IDXGIDevice2`.
pub const IID_IDXGI_DEVICE_2: Guid = Guid::new(
    0x0500_8617,
    0xfbfd,
    0x4051,
    [0xa7, 0x90, 0x14, 0x48, 0x84, 0xb4, 0xf6, 0xa9],
);

/// Another spelling of [`IID_IDXGI_DEVICE_2`].
pub const IID_IDXGI_DEVICE2: Guid = IID_IDXGI_DEVICE_2;

/// `IID_IDXGIDevice3`.
pub const IID_IDXGI_DEVICE_3: Guid = Guid::new(
    0x6007_896c,
    0x3244,
    0x4afd,
    [0xbf, 0x18, 0xa6, 0xd3, 0xbe, 0xda, 0x50, 0x23],
);

/// Another spelling of [`IID_IDXGI_DEVICE_3`].
pub const IID_IDXGI_DEVICE3: Guid = IID_IDXGI_DEVICE_3;

/// `IID_IDXGIResource`.
pub const IID_IDXGI_RESOURCE: Guid = Guid::new(
    0x035f_3ab4,
    0x482e,
    0x4e50,
    [0xb4, 0x1f, 0x8a, 0x7f, 0x8b, 0xd8, 0x96, 0x0b],
);

/// `IID_IDXGIResource1`.
pub const IID_IDXGI_RESOURCE_1: Guid = Guid::new(
    0x3096_1379,
    0x4609,
    0x4a41,
    [0x99, 0x8e, 0x54, 0xfe, 0x56, 0x7e, 0xe0, 0xc1],
);

/// `IID_IDXGISurface`.
pub const IID_IDXGI_SURFACE: Guid = Guid::new(
    0xcafc_b56c,
    0x6ac3,
    0x4889,
    [0xbf, 0x47, 0x9e, 0x23, 0xbb, 0xd2, 0x60, 0xec],
);

/// `IID_IDXGISurface1`.
pub const IID_IDXGI_SURFACE_1: Guid = Guid::new(
    0x4ae6_3092,
    0x6327,
    0x4c1b,
    [0x80, 0xae, 0xbf, 0xe1, 0x2e, 0xa3, 0x2b, 0x86],
);

/// `IID_IDXGISurface2`.
pub const IID_IDXGI_SURFACE_2: Guid = Guid::new(
    0xaba4_96dd,
    0xb617,
    0x4cb8,
    [0xa8, 0x66, 0xbc, 0x44, 0xd7, 0xeb, 0x1f, 0xa2],
);

/// `IID_IDXGIKeyedMutex`.
pub const IID_IDXGI_KEYED_MUTEX: Guid = Guid::new(
    0x9d8e_1289,
    0xd7b3,
    0x465f,
    [0x81, 0x26, 0x25, 0x0e, 0x34, 0x9a, 0xf8, 0x5d],
);

/// `IID_IDXGISwapChain`.
pub const IID_IDXGI_SWAP_CHAIN: Guid = Guid::new(
    0x310d_36a0,
    0xd2e7,
    0x4c0a,
    [0xaa, 0x04, 0x6a, 0x9d, 0x23, 0xb8, 0x88, 0x6a],
);

/// `IID_IDXGISwapChain1`.
pub const IID_IDXGI_SWAP_CHAIN_1: Guid = Guid::new(
    0x790a_45f7,
    0x0d42,
    0x4876,
    [0x98, 0x3a, 0x0a, 0x55, 0xcf, 0xe6, 0xf4, 0xaa],
);

/// Another spelling of [`IID_IDXGI_SWAP_CHAIN_1`].
pub const IID_IDXGI_SWAP_CHAIN1: Guid = IID_IDXGI_SWAP_CHAIN_1;

/// `IID_IDXGISwapChain2`.
pub const IID_IDXGI_SWAP_CHAIN_2: Guid = Guid::new(
    0xa8be_2ac4,
    0x199f,
    0x4946,
    [0xb3, 0x31, 0x79, 0x59, 0x9f, 0xb9, 0x8d, 0xe7],
);

/// Another spelling of [`IID_IDXGI_SWAP_CHAIN_2`].
pub const IID_IDXGI_SWAP_CHAIN2: Guid = IID_IDXGI_SWAP_CHAIN_2;

/// `IID_IDXGISwapChain3`.
pub const IID_IDXGI_SWAP_CHAIN_3: Guid = Guid::new(
    0x94d9_9bdb,
    0xf1f8,
    0x4ab0,
    [0xb2, 0x36, 0x7d, 0xa0, 0x17, 0x0e, 0xda, 0xb1],
);

/// Another spelling of [`IID_IDXGI_SWAP_CHAIN_3`].
pub const IID_IDXGI_SWAP_CHAIN3: Guid = IID_IDXGI_SWAP_CHAIN_3;

/// `IID_IDXGISwapChain4`.
pub const IID_IDXGI_SWAP_CHAIN_4: Guid = Guid::new(
    0x3d58_5d5a,
    0xbd4a,
    0x489e,
    [0xb1, 0xf4, 0x3d, 0xbc, 0xb6, 0x45, 0x2f, 0xfb],
);

/// Another spelling of [`IID_IDXGI_SWAP_CHAIN_4`].
pub const IID_IDXGI_SWAP_CHAIN4: Guid = IID_IDXGI_SWAP_CHAIN_4;

/// `IID_ID3DDeviceContextState`.
pub const IID_ID3D_DEVICE_CONTEXT_STATE: Guid = Guid::new(
    0x5c1e_0d8a,
    0x7c23,
    0x48f9,
    [0x8c, 0x59, 0xa9, 0x29, 0x58, 0xce, 0xff, 0x11],
);

/// `IID_ID3DUserDefinedAnnotation`.
pub const IID_ID3D_USER_DEFINED_ANNOTATION: Guid = Guid::new(
    0xb2da_ad8b,
    0x03d4,
    0x4dbf,
    [0x95, 0xeb, 0x32, 0xab, 0x4b, 0x63, 0xd0, 0xab],
);

/// `IID_IDXGI_FACTORY`.
pub const IID_IDXGI_FACTORY: Guid = Guid::new(
    0x7b71_66ec,
    0x21c7,
    0x44ae,
    [0xb2, 0x1a, 0xc9, 0xae, 0x32, 0x1a, 0xe3, 0x69],
);

/// `IID_IDXGI_FACTORY1`.
pub const IID_IDXGI_FACTORY1: Guid = Guid::new(
    0x770a_ae78,
    0xf26f,
    0x4dba,
    [0xa8, 0x29, 0x25, 0x3c, 0x83, 0xd1, 0xb3, 0x87],
);

/// Another spelling of [`IID_IDXGI_FACTORY1`].
pub const IID_IDXGIFACTORY1: Guid = IID_IDXGI_FACTORY1;

/// `IID_IDXGI_FACTORY2`.
pub const IID_IDXGI_FACTORY2: Guid = Guid::new(
    0x50c8_3a1c,
    0xe072,
    0x4c48,
    [0x87, 0xb0, 0x36, 0x30, 0xfa, 0x36, 0xa6, 0xd0],
);

/// `IID_IDXGI_FACTORY3`.
pub const IID_IDXGI_FACTORY3: Guid = Guid::new(
    0x2548_3823,
    0xcd46,
    0x4c7d,
    [0x86, 0xca, 0x47, 0xaa, 0x95, 0xb8, 0x37, 0xbd],
);

/// `IID_IDXGI_FACTORY4`.
pub const IID_IDXGI_FACTORY4: Guid = Guid::new(
    0x1bc6_ea02,
    0xef36,
    0x464f,
    [0xbf, 0x0c, 0x21, 0xca, 0x39, 0xe5, 0x16, 0x8a],
);

/// `IID_IDXGI_FACTORY5`.
pub const IID_IDXGI_FACTORY5: Guid = Guid::new(
    0x7632_e1f5,
    0xee65,
    0x4dca,
    [0x87, 0xfd, 0x84, 0xcd, 0x75, 0xf8, 0x83, 0x8d],
);

/// `IID_IDXGI_FACTORY6`.
pub const IID_IDXGI_FACTORY6: Guid = Guid::new(
    0xc1b6_694f,
    0xff09,
    0x44a9,
    [0xb0, 0x3c, 0x77, 0x90, 0x0a, 0x0a, 0x1d, 0x17],
);

/// `IID_IDXGI_FACTORY7`.
pub const IID_IDXGI_FACTORY7: Guid = Guid::new(
    0xa496_6eed,
    0x76db,
    0x44da,
    [0x84, 0xc1, 0xee, 0x9a, 0x7a, 0xfb, 0x20, 0xa8],
);

/// `IID_IDXGI_ADAPTER`.
pub const IID_IDXGI_ADAPTER: Guid = Guid::new(
    0x2411_e7e1,
    0x12ac,
    0x4ccf,
    [0xbd, 0x14, 0x97, 0x98, 0xe8, 0x53, 0x4d, 0xc0],
);

/// `IID_IDXGI_ADAPTER1`.
pub const IID_IDXGI_ADAPTER1: Guid = Guid::new(
    0x2903_8f61,
    0x3839,
    0x4626,
    [0x91, 0xfd, 0x08, 0x68, 0x79, 0x01, 0x1a, 0x05],
);

/// `IID_IDXGI_ADAPTER2`.
pub const IID_IDXGI_ADAPTER2: Guid = Guid::new(
    0x0aa1_ae0a,
    0xfa0e,
    0x4b84,
    [0x86, 0x44, 0xe0, 0x5f, 0xf8, 0xe5, 0xac, 0xb5],
);

/// `IID_IDXGI_ADAPTER3`.
pub const IID_IDXGI_ADAPTER3: Guid = Guid::new(
    0x6459_67a4,
    0x1392,
    0x4310,
    [0xa7, 0x98, 0x80, 0x53, 0xce, 0x3e, 0x93, 0xfd],
);

/// `IID_IDXGI_ADAPTER4`.
pub const IID_IDXGI_ADAPTER4: Guid = Guid::new(
    0x3c8d_99d1,
    0x4fbf,
    0x4181,
    [0xa8, 0x2c, 0xaf, 0x66, 0xbf, 0x7b, 0xd2, 0x4e],
);

/// `IID_IDXGI_OUTPUT`.
pub const IID_IDXGI_OUTPUT: Guid = Guid::new(
    0xae02_eedb,
    0xc735,
    0x4690,
    [0x8d, 0x52, 0x5a, 0x8d, 0xc2, 0x02, 0x13, 0xaa],
);

/// `IID_IDXGI_OUTPUT1`.
pub const IID_IDXGI_OUTPUT1: Guid = Guid::new(
    0x00cd_dea8,
    0x939b,
    0x4b83,
    [0xa3, 0x40, 0xa6, 0x85, 0x22, 0x66, 0x66, 0xcc],
);

/// Another spelling of [`IID_IDXGI_OUTPUT1`].
pub const IID_IDXGI_OUTPUT_1: Guid = IID_IDXGI_OUTPUT1;

/// `IID_IDXGI_OUTPUT2`.
pub const IID_IDXGI_OUTPUT2: Guid = Guid::new(
    0x595e_39d1,
    0x2724,
    0x4663,
    [0x99, 0xb1, 0xda, 0x96, 0x9d, 0xe2, 0x83, 0x64],
);

/// `IID_IDXGI_OUTPUT3`.
pub const IID_IDXGI_OUTPUT3: Guid = Guid::new(
    0x8a6b_b301,
    0x7e7e,
    0x41f4,
    [0xa8, 0xe0, 0x5b, 0x32, 0xf7, 0xf9, 0x9b, 0x18],
);

/// `IID_IDXGI_OUTPUT4`.
pub const IID_IDXGI_OUTPUT4: Guid = Guid::new(
    0xdc7d_ca35,
    0x2196,
    0x414d,
    [0x9f, 0x53, 0x61, 0x78, 0x84, 0x03, 0x2a, 0x60],
);

/// `IID_IDXGI_OUTPUT5`.
pub const IID_IDXGI_OUTPUT5: Guid = Guid::new(
    0x80a0_7424,
    0xab52,
    0x42eb,
    [0x83, 0x3c, 0x0c, 0x42, 0xfd, 0x28, 0x2d, 0x98],
);

/// `IID_IDXGI_OUTPUT6`.
pub const IID_IDXGI_OUTPUT6: Guid = Guid::new(
    0x0683_46e8,
    0xaaec,
    0x4b84,
    [0xad, 0xd7, 0x13, 0x7f, 0x51, 0x3f, 0x77, 0xa1],
);

/// Wine's private adapter interface, which the adapter refuses without a log line.
pub const IID_WINE_DXGI_ADAPTER: Guid = Guid::new(
    0x1739_9d75,
    0x964e,
    0x4c03,
    [0x99, 0xf8, 0x9d, 0x4f, 0xd1, 0x96, 0xdd, 0x62],
);

/// `IID_IDXGI_DEVICE4`.
pub const IID_IDXGI_DEVICE4: Guid = Guid::new(
    0x95b4_f95f,
    0xd8da,
    0x4ca4,
    [0x9e, 0xe6, 0x3b, 0x76, 0xd5, 0x96, 0x8a, 0x10],
);

// ---- Direct3D 10 ----

/// `IID_ID3D10DeviceChild`.
pub const IID_ID3D10_DEVICE_CHILD: Guid = Guid::new(
    0x9b7e_4c00,
    0x342c,
    0x4106,
    [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
);

/// `IID_ID3D10Resource`.
pub const IID_ID3D10_RESOURCE: Guid = Guid::new(
    0x9b7e_4c01,
    0x342c,
    0x4106,
    [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
);

/// `IID_ID3D10Buffer`.
pub const IID_ID3D10_BUFFER: Guid = Guid::new(
    0x9b7e_4c02,
    0x342c,
    0x4106,
    [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
);

/// `IID_ID3D10Texture1D`.
pub const IID_ID3D10_TEXTURE_1D: Guid = Guid::new(
    0x9b7e_4c03,
    0x342c,
    0x4106,
    [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
);

/// `IID_ID3D10Texture2D`.
pub const IID_ID3D10_TEXTURE_2D: Guid = Guid::new(
    0x9b7e_4c04,
    0x342c,
    0x4106,
    [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
);

/// `IID_ID3D10Texture3D`.
pub const IID_ID3D10_TEXTURE_3D: Guid = Guid::new(
    0x9b7e_4c05,
    0x342c,
    0x4106,
    [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
);

/// `IID_ID3D10View`.
pub const IID_ID3D10_VIEW: Guid = Guid::new(
    0xc902_b03f,
    0x60a7,
    0x49ba,
    [0x99, 0x36, 0x2a, 0x3a, 0xb3, 0x7a, 0x7e, 0x33],
);

/// `IID_ID3D10ShaderResourceView`.
pub const IID_ID3D10_SHADER_RESOURCE_VIEW: Guid = Guid::new(
    0x9b7e_4c07,
    0x342c,
    0x4106,
    [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
);

/// `IID_ID3D10ShaderResourceView1`.
pub const IID_ID3D10_SHADER_RESOURCE_VIEW_1: Guid = Guid::new(
    0x9b7e_4c87,
    0x342c,
    0x4106,
    [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
);

/// `IID_ID3D10RenderTargetView`.
pub const IID_ID3D10_RENDER_TARGET_VIEW: Guid = Guid::new(
    0x9b7e_4c08,
    0x342c,
    0x4106,
    [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
);

/// `IID_ID3D10DepthStencilView`.
pub const IID_ID3D10_DEPTH_STENCIL_VIEW: Guid = Guid::new(
    0x9b7e_4c09,
    0x342c,
    0x4106,
    [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
);

/// `IID_ID3D10BlendState`.
pub const IID_ID3D10_BLEND_STATE: Guid = Guid::new(
    0xedad_8d19,
    0x8a35,
    0x4d6d,
    [0x85, 0x66, 0x2e, 0xa2, 0x76, 0xcd, 0xe1, 0x61],
);

/// `IID_ID3D10BlendState1`.
pub const IID_ID3D10_BLEND_STATE_1: Guid = Guid::new(
    0xedad_8d99,
    0x8a35,
    0x4d6d,
    [0x85, 0x66, 0x2e, 0xa2, 0x76, 0xcd, 0xe1, 0x61],
);

/// `IID_ID3D10DepthStencilState`.
pub const IID_ID3D10_DEPTH_STENCIL_STATE: Guid = Guid::new(
    0x2b4b_1cc8,
    0xa4ad,
    0x41f8,
    [0x83, 0x22, 0xca, 0x86, 0xfc, 0x3e, 0xc6, 0x75],
);

/// `IID_ID3D10RasterizerState`.
pub const IID_ID3D10_RASTERIZER_STATE: Guid = Guid::new(
    0xa2a0_7292,
    0x89af,
    0x4345,
    [0xbe, 0x2e, 0xc5, 0x3d, 0x9f, 0xbb, 0x6e, 0x9f],
);

/// `IID_ID3D10SamplerState`.
pub const IID_ID3D10_SAMPLER_STATE: Guid = Guid::new(
    0x9b7e_4c0c,
    0x342c,
    0x4106,
    [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
);

/// `IID_ID3D10InputLayout`.
pub const IID_ID3D10_INPUT_LAYOUT: Guid = Guid::new(
    0x9b7e_4c0b,
    0x342c,
    0x4106,
    [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
);

/// `IID_ID3D10VertexShader`.
pub const IID_ID3D10_VERTEX_SHADER: Guid = Guid::new(
    0x9b7e_4c0a,
    0x342c,
    0x4106,
    [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
);

/// `IID_ID3D10GeometryShader`.
pub const IID_ID3D10_GEOMETRY_SHADER: Guid = Guid::new(
    0x6316_be88,
    0x54cd,
    0x4040,
    [0xab, 0x44, 0x20, 0x46, 0x1b, 0xc8, 0x1f, 0x68],
);

/// `IID_ID3D10PixelShader`.
pub const IID_ID3D10_PIXEL_SHADER: Guid = Guid::new(
    0x4968_b601,
    0x9d00,
    0x4cde,
    [0x83, 0x46, 0x8e, 0x7f, 0x67, 0x58, 0x19, 0xb6],
);

/// `IID_ID3D10Asynchronous`.
pub const IID_ID3D10_ASYNCHRONOUS: Guid = Guid::new(
    0x9b7e_4c0d,
    0x342c,
    0x4106,
    [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
);

/// `IID_ID3D10Query`.
pub const IID_ID3D10_QUERY: Guid = Guid::new(
    0x9b7e_4c0e,
    0x342c,
    0x4106,
    [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
);

/// `IID_ID3D10Predicate`.
pub const IID_ID3D10_PREDICATE: Guid = Guid::new(
    0x9b7e_4c10,
    0x342c,
    0x4106,
    [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
);

/// `IID_ID3D10Counter`.
pub const IID_ID3D10_COUNTER: Guid = Guid::new(
    0x9b7e_4c11,
    0x342c,
    0x4106,
    [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
);

/// `IID_ID3D10Device`.
pub const IID_ID3D10_DEVICE: Guid = Guid::new(
    0x9b7e_4c0f,
    0x342c,
    0x4106,
    [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
);

/// `IID_ID3D10Device1`.
pub const IID_ID3D10_DEVICE1: Guid = Guid::new(
    0x9b7e_4c8f,
    0x342c,
    0x4106,
    [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
);

/// `ID3D10Blob` (`d3dcommon.h`).
pub const IID_ID3D10_BLOB: Guid = Guid::new(
    0x8ba5_fb08,
    0x5195,
    0x40e2,
    [0xac, 0x58, 0x0d, 0x98, 0x9c, 0x3a, 0x01, 0x02],
);

// ---- Direct3D 11 ----

/// `IID_ID3D11DeviceChild`.
pub const IID_ID3D11_DEVICE_CHILD: Guid = Guid::new(
    0x1841_e5c8,
    0x16b0,
    0x489b,
    [0xbc, 0xc8, 0x44, 0xcf, 0xb0, 0xd5, 0xde, 0xae],
);

/// `IID_ID3D11Resource`.
pub const IID_ID3D11_RESOURCE: Guid = Guid::new(
    0xdc8e_63f3,
    0xd12b,
    0x4952,
    [0xb4, 0x7b, 0x5e, 0x45, 0x02, 0x6a, 0x86, 0x2d],
);

/// `IID_ID3D11Buffer`.
pub const IID_ID3D11_BUFFER: Guid = Guid::new(
    0x4857_0b85,
    0xd1ee,
    0x4fcd,
    [0xa2, 0x50, 0xeb, 0x35, 0x07, 0x22, 0xb0, 0x37],
);

/// `IID_ID3D11Texture1D`.
pub const IID_ID3D11_TEXTURE_1D: Guid = Guid::new(
    0xf8fb_5c27,
    0xc6b3,
    0x4f75,
    [0xa4, 0xc8, 0x43, 0x9a, 0xf2, 0xef, 0x56, 0x4c],
);

/// `IID_ID3D11Texture2D`.
pub const IID_ID3D11_TEXTURE_2D: Guid = Guid::new(
    0x6f15_aaf2,
    0xd208,
    0x4e89,
    [0x9a, 0xb4, 0x48, 0x95, 0x35, 0xd3, 0x4f, 0x9c],
);

/// `IID_ID3D11Texture2D1`.
pub const IID_ID3D11_TEXTURE_2D1: Guid = Guid::new(
    0x5121_8251,
    0x1e33,
    0x4617,
    [0x9c, 0xcb, 0x4d, 0x3a, 0x43, 0x67, 0xe7, 0xbb],
);

/// `IID_ID3D11Texture3D`.
pub const IID_ID3D11_TEXTURE_3D: Guid = Guid::new(
    0x037e_866e,
    0xf56d,
    0x4357,
    [0xa8, 0xaf, 0x9d, 0xab, 0xbe, 0x6e, 0x25, 0x0e],
);

/// `IID_ID3D11Texture3D1`.
pub const IID_ID3D11_TEXTURE_3D1: Guid = Guid::new(
    0x0c71_1683,
    0x2853,
    0x4846,
    [0x9b, 0xb0, 0xf3, 0xe6, 0x06, 0x39, 0xe4, 0x6a],
);

/// `IID_ID3D11View`.
pub const IID_ID3D11_VIEW: Guid = Guid::new(
    0x839d_1216,
    0xbb2e,
    0x412b,
    [0xb7, 0xf4, 0xa9, 0xdb, 0xeb, 0xe0, 0x8e, 0xd1],
);

/// `IID_ID3D11ShaderResourceView`.
pub const IID_ID3D11_SHADER_RESOURCE_VIEW: Guid = Guid::new(
    0xb0e0_6fe0,
    0x8192,
    0x4e1a,
    [0xb1, 0xca, 0x36, 0xd7, 0x41, 0x47, 0x10, 0xb2],
);

/// `IID_ID3D11ShaderResourceView1`.
pub const IID_ID3D11_SHADER_RESOURCE_VIEW_1: Guid = Guid::new(
    0x9130_8b87,
    0x9040,
    0x411d,
    [0x8c, 0x67, 0xc3, 0x92, 0x53, 0xce, 0x38, 0x02],
);

/// `IID_ID3D11RenderTargetView`.
pub const IID_ID3D11_RENDER_TARGET_VIEW: Guid = Guid::new(
    0xdfdb_a067,
    0x0b8d,
    0x4865,
    [0x87, 0x5b, 0xd7, 0xb4, 0x51, 0x6c, 0xc1, 0x64],
);

/// `IID_ID3D11RenderTargetView1`.
pub const IID_ID3D11_RENDER_TARGET_VIEW_1: Guid = Guid::new(
    0xffbe_2e23,
    0xf011,
    0x418a,
    [0xac, 0x56, 0x5c, 0xee, 0xd7, 0xc5, 0xb9, 0x4b],
);

/// `IID_ID3D11DepthStencilView`.
pub const IID_ID3D11_DEPTH_STENCIL_VIEW: Guid = Guid::new(
    0x9fda_c92a,
    0x1876,
    0x48c3,
    [0xaf, 0xad, 0x25, 0xb9, 0x4f, 0x84, 0xa9, 0xb6],
);

/// `IID_ID3D11UnorderedAccessView`.
pub const IID_ID3D11_UNORDERED_ACCESS_VIEW: Guid = Guid::new(
    0x28ac_f509,
    0x7f5c,
    0x48f6,
    [0x86, 0x11, 0xf3, 0x16, 0x01, 0x0a, 0x63, 0x80],
);

/// `IID_ID3D11UnorderedAccessView1`.
pub const IID_ID3D11_UNORDERED_ACCESS_VIEW_1: Guid = Guid::new(
    0x7b3b_6153,
    0xa886,
    0x4544,
    [0xab, 0x37, 0x65, 0x37, 0xc8, 0x50, 0x04, 0x03],
);

/// `IID_ID3D11BlendState`.
pub const IID_ID3D11_BLEND_STATE: Guid = Guid::new(
    0x75b6_8faa,
    0x347d,
    0x4159,
    [0x8f, 0x45, 0xa0, 0x64, 0x0f, 0x01, 0xcd, 0x9a],
);

/// `IID_ID3D11BlendState1`.
pub const IID_ID3D11_BLEND_STATE_1: Guid = Guid::new(
    0xcc86_fabe,
    0xda55,
    0x401d,
    [0x85, 0xe7, 0xe3, 0xc9, 0xde, 0x28, 0x77, 0xe9],
);

/// `IID_ID3D11DepthStencilState`.
pub const IID_ID3D11_DEPTH_STENCIL_STATE: Guid = Guid::new(
    0x0382_3efb,
    0x8d8f,
    0x4e1c,
    [0x9a, 0xa2, 0xf6, 0x4b, 0xb2, 0xcb, 0xfd, 0xf1],
);

/// `IID_ID3D11RasterizerState`.
pub const IID_ID3D11_RASTERIZER_STATE: Guid = Guid::new(
    0x9bb4_ab81,
    0xab1a,
    0x4d8f,
    [0xb5, 0x06, 0xfc, 0x04, 0x20, 0x0b, 0x6e, 0xe7],
);

/// `IID_ID3D11RasterizerState1`.
pub const IID_ID3D11_RASTERIZER_STATE_1: Guid = Guid::new(
    0x1217_d7a6,
    0x5039,
    0x418c,
    [0xb0, 0x42, 0x9c, 0xbe, 0x25, 0x6a, 0xfd, 0x6e],
);

/// `IID_ID3D11RasterizerState2`.
pub const IID_ID3D11_RASTERIZER_STATE_2: Guid = Guid::new(
    0x6fbd_02fb,
    0x209f,
    0x46c4,
    [0xb0, 0x59, 0x2e, 0xd1, 0x55, 0x86, 0xa6, 0xac],
);

/// `IID_ID3D11SamplerState`.
pub const IID_ID3D11_SAMPLER_STATE: Guid = Guid::new(
    0xda6f_ea51,
    0x564c,
    0x4487,
    [0x98, 0x10, 0xf0, 0xd0, 0xf9, 0xb4, 0xe3, 0xa5],
);

/// `IID_ID3D11InputLayout`.
pub const IID_ID3D11_INPUT_LAYOUT: Guid = Guid::new(
    0xe481_9ddc,
    0x4cf0,
    0x4025,
    [0xbd, 0x26, 0x5d, 0xe8, 0x2a, 0x3e, 0x07, 0xb7],
);

/// `IID_ID3D11VertexShader`.
pub const IID_ID3D11_VERTEX_SHADER: Guid = Guid::new(
    0x3b30_1d64,
    0xd678,
    0x4289,
    [0x88, 0x97, 0x22, 0xf8, 0x92, 0x8b, 0x72, 0xf3],
);

/// `IID_ID3D11HullShader`.
pub const IID_ID3D11_HULL_SHADER: Guid = Guid::new(
    0x8e5c_6061,
    0x628a,
    0x4c8e,
    [0x82, 0x64, 0xbb, 0xe4, 0x5c, 0xb3, 0xd5, 0xdd],
);

/// `IID_ID3D11DomainShader`.
pub const IID_ID3D11_DOMAIN_SHADER: Guid = Guid::new(
    0xf582_c508,
    0x0f36,
    0x490c,
    [0x99, 0x77, 0x31, 0xee, 0xce, 0x26, 0x8c, 0xfa],
);

/// `IID_ID3D11GeometryShader`.
pub const IID_ID3D11_GEOMETRY_SHADER: Guid = Guid::new(
    0x3832_5b96,
    0xeffb,
    0x4022,
    [0xba, 0x02, 0x2e, 0x79, 0x5b, 0x70, 0x27, 0x5c],
);

/// `IID_ID3D11PixelShader`.
pub const IID_ID3D11_PIXEL_SHADER: Guid = Guid::new(
    0xea82_e40d,
    0x51dc,
    0x4f33,
    [0x93, 0xd4, 0xdb, 0x7c, 0x91, 0x25, 0xae, 0x8c],
);

/// `IID_ID3D11ComputeShader`.
pub const IID_ID3D11_COMPUTE_SHADER: Guid = Guid::new(
    0x4f5b_196e,
    0xc2bd,
    0x495e,
    [0xbd, 0x01, 0x1f, 0xde, 0xd3, 0x8e, 0x49, 0x69],
);

/// `IID_ID3D11Asynchronous`.
pub const IID_ID3D11_ASYNCHRONOUS: Guid = Guid::new(
    0x4b35_d0cd,
    0x1e15,
    0x4258,
    [0x9c, 0x98, 0x1b, 0x13, 0x33, 0xf6, 0xdd, 0x3b],
);

/// `IID_ID3D11Query`.
pub const IID_ID3D11_QUERY: Guid = Guid::new(
    0xd6c0_0747,
    0x87b7,
    0x425e,
    [0xb8, 0x4d, 0x44, 0xd1, 0x08, 0x56, 0x0a, 0xfd],
);

/// `IID_ID3D11Query1`.
pub const IID_ID3D11_QUERY_1: Guid = Guid::new(
    0x631b_4766,
    0x36dc,
    0x461d,
    [0x8d, 0xb6, 0xc4, 0x7e, 0x13, 0xe6, 0x09, 0x16],
);

/// `IID_ID3D11Predicate`.
pub const IID_ID3D11_PREDICATE: Guid = Guid::new(
    0x9eb5_76dd,
    0x9f77,
    0x4d86,
    [0x81, 0xaa, 0x8b, 0xab, 0x5f, 0xe4, 0x90, 0xe2],
);

/// `IID_ID3D11Counter`.
pub const IID_ID3D11_COUNTER: Guid = Guid::new(
    0x6e8c_49fb,
    0xa371,
    0x4770,
    [0xb4, 0x40, 0x29, 0x08, 0x60, 0x22, 0xb7, 0x41],
);

/// `IID_ID3D11ClassInstance`.
pub const IID_ID3D11_CLASS_INSTANCE: Guid = Guid::new(
    0xa6cd_7faa,
    0xb0b7,
    0x4a2f,
    [0x94, 0x36, 0x86, 0x62, 0xa6, 0x57, 0x97, 0xcb],
);

/// `IID_ID3D11ClassLinkage`.
pub const IID_ID3D11_CLASS_LINKAGE: Guid = Guid::new(
    0xddf5_7cba,
    0x9543,
    0x46e4,
    [0xa1, 0x2b, 0xf2, 0x07, 0xa0, 0xfe, 0x7f, 0xed],
);

/// `IID_ID3D11CommandList`.
pub const IID_ID3D11_COMMAND_LIST: Guid = Guid::new(
    0xa24b_c4d1,
    0x769e,
    0x43f7,
    [0x80, 0x13, 0x98, 0xff, 0x56, 0x6c, 0x18, 0xe2],
);

/// `IID_ID3D11DeviceContext`.
pub const IID_ID3D11_DEVICE_CONTEXT: Guid = Guid::new(
    0xc0bf_a96c,
    0xe089,
    0x44fb,
    [0x8e, 0xaf, 0x26, 0xf8, 0x79, 0x61, 0x90, 0xda],
);

/// `IID_ID3D11DeviceContext1`.
pub const IID_ID3D11_DEVICE_CONTEXT_1: Guid = Guid::new(
    0xbb2c_6faa,
    0xb5fb,
    0x4082,
    [0x8e, 0x6b, 0x38, 0x8b, 0x8c, 0xfa, 0x90, 0xe1],
);

/// `IID_ID3D11DeviceContext2`.
pub const IID_ID3D11_DEVICE_CONTEXT_2: Guid = Guid::new(
    0x420d_5b32,
    0xb90c,
    0x4da4,
    [0xbe, 0xf0, 0x35, 0x9f, 0x6a, 0x24, 0xa8, 0x3a],
);

/// `IID_ID3D11DeviceContext3`.
pub const IID_ID3D11_DEVICE_CONTEXT_3: Guid = Guid::new(
    0xb4e3_c01d,
    0xe79e,
    0x4637,
    [0x91, 0xb2, 0x51, 0x0e, 0x9f, 0x4c, 0x9b, 0x8f],
);

/// `IID_ID3D11DeviceContext4`.
pub const IID_ID3D11_DEVICE_CONTEXT_4: Guid = Guid::new(
    0x9176_00da,
    0xf58c,
    0x4c33,
    [0x98, 0xd8, 0x3e, 0x15, 0xb3, 0x90, 0xfa, 0x24],
);

/// `IID_ID3D11Fence`.
pub const IID_ID3D11_FENCE: Guid = Guid::new(
    0xaffd_e9d1,
    0x1df7,
    0x4bb7,
    [0x8a, 0x34, 0x0f, 0x46, 0x25, 0x1d, 0xab, 0x80],
);

/// `IID_ID3D11Device`.
pub const IID_ID3D11_DEVICE: Guid = Guid::new(
    0xdb6f_6ddb,
    0xac77,
    0x4e88,
    [0x82, 0x53, 0x81, 0x9d, 0xf9, 0xbb, 0xf1, 0x40],
);

/// Another spelling of [`IID_ID3D11_DEVICE`].
pub const IID_DEVICE: Guid = IID_ID3D11_DEVICE;

/// `IID_ID3D11Device1`.
pub const IID_ID3D11_DEVICE_1: Guid = Guid::new(
    0xa04b_fb29,
    0x08ef,
    0x43d6,
    [0xa4, 0x9c, 0xa9, 0xbd, 0xbd, 0xcb, 0xe6, 0x86],
);

/// Another spelling of [`IID_ID3D11_DEVICE_1`].
pub const IID_ID3D11_DEVICE1: Guid = IID_ID3D11_DEVICE_1;

/// `IID_ID3D11Device2`.
pub const IID_ID3D11_DEVICE_2: Guid = Guid::new(
    0x9d06_dffa,
    0xd1e5,
    0x4d07,
    [0x83, 0xa8, 0x1b, 0xb1, 0x23, 0xf2, 0xf8, 0x41],
);

/// Another spelling of [`IID_ID3D11_DEVICE_2`].
pub const IID_ID3D11_DEVICE2: Guid = IID_ID3D11_DEVICE_2;

/// `IID_ID3D11Device3`.
pub const IID_ID3D11_DEVICE_3: Guid = Guid::new(
    0xa05c_8c37,
    0xd2c6,
    0x4732,
    [0xb3, 0xa0, 0x9c, 0xe0, 0xb0, 0xdc, 0x9a, 0xe6],
);

/// Another spelling of [`IID_ID3D11_DEVICE_3`].
pub const IID_ID3D11_DEVICE3: Guid = IID_ID3D11_DEVICE_3;

/// `IID_ID3D11Device4`.
pub const IID_ID3D11_DEVICE_4: Guid = Guid::new(
    0x8992_ab71,
    0x02e6,
    0x4b8d,
    [0xba, 0x48, 0xb0, 0x56, 0xdc, 0xda, 0x42, 0xc4],
);

/// Another spelling of [`IID_ID3D11_DEVICE_4`].
pub const IID_ID3D11_DEVICE4: Guid = IID_ID3D11_DEVICE_4;

/// `IID_ID3D11Device5`.
pub const IID_ID3D11_DEVICE_5: Guid = Guid::new(
    0x8ffd_e202,
    0xa0e7,
    0x45df,
    [0x9e, 0x01, 0xe8, 0x37, 0x80, 0x1b, 0x5e, 0xa0],
);

/// Another spelling of [`IID_ID3D11_DEVICE_5`].
pub const IID_ID3D11_DEVICE5: Guid = IID_ID3D11_DEVICE_5;

/// `IID_ID3D11Multithread`.
pub const IID_ID3D11_MULTITHREAD: Guid = Guid::new(
    0x9b7e_4e00,
    0x342c,
    0x4106,
    [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
);

/// Another spelling of [`IID_ID3D11_MULTITHREAD`].
pub const IID_ID3D10_MULTITHREAD: Guid = IID_ID3D11_MULTITHREAD;

/// `IID_ID3D11_DEBUG`.
pub const IID_ID3D11_DEBUG: Guid = Guid::new(
    0x79cf_2233,
    0x7536,
    0x4948,
    [0x9d, 0x36, 0x1e, 0x46, 0x92, 0xdc, 0x57, 0x60],
);

/// `IID_ID3D11_INFO_QUEUE`.
pub const IID_ID3D11_INFO_QUEUE: Guid = Guid::new(
    0x6543_dbb6,
    0x1b48,
    0x42f5,
    [0xab, 0x82, 0xe9, 0x7e, 0xc7, 0x43, 0x26, 0xf6],
);

// ---- Direct3D 12 ----

/// `ID3D12Object`.
pub const IID_ID3D12_OBJECT: Guid = Guid::new(
    0xc4fe_c28f,
    0x7966,
    0x4e95,
    [0x9f, 0x94, 0xf4, 0x31, 0xcb, 0x56, 0xc3, 0xb8],
);

/// `ID3D12DeviceChild`.
pub const IID_ID3D12_DEVICE_CHILD: Guid = Guid::new(
    0x905d_b94b,
    0xa00c,
    0x4140,
    [0x9d, 0xf5, 0x2b, 0x64, 0xca, 0x9e, 0xa3, 0x57],
);

/// `ID3D12Pageable`.
pub const IID_ID3D12_PAGEABLE: Guid = Guid::new(
    0x63ee_58fb,
    0x1268,
    0x4835,
    [0x86, 0xda, 0xf0, 0x08, 0xce, 0x62, 0xf0, 0xd6],
);

/// `ID3D12RootSignature`.
pub const IID_ID3D12_ROOT_SIGNATURE: Guid = Guid::new(
    0xc54a_6b66,
    0x72df,
    0x4ee8,
    [0x8b, 0xe5, 0xa9, 0x46, 0xa1, 0x42, 0x92, 0x14],
);

/// `ID3D12RootSignatureDeserializer`.
pub const IID_ID3D12_ROOT_SIGNATURE_DESERIALIZER: Guid = Guid::new(
    0x34ab_647b,
    0x3cc8,
    0x46ac,
    [0x84, 0x1b, 0xc0, 0x96, 0x56, 0x45, 0xc0, 0x46],
);

/// `ID3D12VersionedRootSignatureDeserializer`.
pub const IID_ID3D12_VERSIONED_ROOT_SIGNATURE_DESERIALIZER: Guid = Guid::new(
    0x7f91_ce67,
    0x090c,
    0x4bb7,
    [0xb7, 0x8e, 0xed, 0x8f, 0xf2, 0xe3, 0x1d, 0xa0],
);

/// `ID3D12Heap`.
pub const IID_ID3D12_HEAP: Guid = Guid::new(
    0x6b3b_2502,
    0x6e51,
    0x45b3,
    [0x90, 0xee, 0x98, 0x84, 0x26, 0x5e, 0x8d, 0xf3],
);

/// `ID3D12Heap1`.
pub const IID_ID3D12_HEAP1: Guid = Guid::new(
    0x572f_7389,
    0x2168,
    0x49e3,
    [0x96, 0x93, 0xd6, 0xdf, 0x58, 0x71, 0xbf, 0x6d],
);

/// `ID3D12Resource`.
pub const IID_ID3D12_RESOURCE: Guid = Guid::new(
    0x6964_42be,
    0xa72e,
    0x4059,
    [0xbc, 0x79, 0x5b, 0x5c, 0x98, 0x04, 0x0f, 0xad],
);

/// `ID3D12Resource1`.
pub const IID_ID3D12_RESOURCE1: Guid = Guid::new(
    0x9d5e_227a,
    0x4430,
    0x4161,
    [0x88, 0xb3, 0x3e, 0xca, 0x6b, 0xb1, 0x6e, 0x19],
);

/// `ID3D12Resource2`.
pub const IID_ID3D12_RESOURCE2: Guid = Guid::new(
    0xbe36_ec3b,
    0xea85,
    0x4aeb,
    [0xa4, 0x5a, 0xe9, 0xd7, 0x64, 0x04, 0xa4, 0x95],
);

/// `ID3D12CommandAllocator`.
pub const IID_ID3D12_COMMAND_ALLOCATOR: Guid = Guid::new(
    0x6102_dee4,
    0xaf59,
    0x4b09,
    [0xb9, 0x99, 0xb4, 0x4d, 0x73, 0xf0, 0x9b, 0x24],
);

/// `ID3D12Fence`.
pub const IID_ID3D12_FENCE: Guid = Guid::new(
    0x0a75_3dcf,
    0xc4d8,
    0x4b91,
    [0xad, 0xf6, 0xbe, 0x5a, 0x60, 0xd9, 0x5a, 0x76],
);

/// `ID3D12Fence1`.
pub const IID_ID3D12_FENCE1: Guid = Guid::new(
    0x4336_85fe,
    0xe22b,
    0x4ca0,
    [0xa8, 0xdb, 0xb5, 0xb4, 0xf4, 0xdd, 0x0e, 0x4a],
);

/// `ID3D12PipelineState`.
pub const IID_ID3D12_PIPELINE_STATE: Guid = Guid::new(
    0x765a_30f3,
    0xf624,
    0x4c6f,
    [0xa8, 0x28, 0xac, 0xe9, 0x48, 0x62, 0x24, 0x45],
);

/// `ID3D12DescriptorHeap`.
pub const IID_ID3D12_DESCRIPTOR_HEAP: Guid = Guid::new(
    0x8efb_471d,
    0x616c,
    0x4f49,
    [0x90, 0xf7, 0x12, 0x7b, 0xb7, 0x63, 0xfa, 0x51],
);

/// `ID3D12QueryHeap`.
pub const IID_ID3D12_QUERY_HEAP: Guid = Guid::new(
    0x0d96_58ae,
    0xed45,
    0x469e,
    [0xa6, 0x1d, 0x97, 0x0e, 0xc5, 0x83, 0xca, 0xb4],
);

/// `ID3D12CommandSignature`.
pub const IID_ID3D12_COMMAND_SIGNATURE: Guid = Guid::new(
    0xc36a_797c,
    0xec80,
    0x4f0a,
    [0x89, 0x85, 0xa7, 0xb2, 0x47, 0x50, 0x82, 0xd1],
);

/// `ID3D12CommandList`.
pub const IID_ID3D12_COMMAND_LIST: Guid = Guid::new(
    0x7116_d91c,
    0xe7e4,
    0x47ce,
    [0xb8, 0xc6, 0xec, 0x81, 0x68, 0xf4, 0x37, 0xe5],
);

/// `ID3D12GraphicsCommandList`.
pub const IID_ID3D12_GRAPHICS_COMMAND_LIST: Guid = Guid::new(
    0x5b16_0d0f,
    0xac1b,
    0x4185,
    [0x8b, 0xa8, 0xb3, 0xae, 0x42, 0xa5, 0xa4, 0x55],
);

/// `ID3D12GraphicsCommandList1`.
pub const IID_ID3D12_GRAPHICS_COMMAND_LIST1: Guid = Guid::new(
    0x5531_03fb,
    0x1fe7,
    0x4557,
    [0xbb, 0x38, 0x94, 0x6d, 0x7d, 0x0e, 0x7c, 0xa7],
);

/// `ID3D12GraphicsCommandList2`.
pub const IID_ID3D12_GRAPHICS_COMMAND_LIST2: Guid = Guid::new(
    0x38c3_e585,
    0xff17,
    0x412c,
    [0x91, 0x50, 0x4f, 0xc6, 0xf9, 0xd7, 0x2a, 0x28],
);

/// `ID3D12GraphicsCommandList3`.
pub const IID_ID3D12_GRAPHICS_COMMAND_LIST3: Guid = Guid::new(
    0x6fda_83a7,
    0xb84c,
    0x4e38,
    [0x9a, 0xc8, 0xc7, 0xbd, 0x22, 0x01, 0x6b, 0x3d],
);

/// `ID3D12GraphicsCommandList4`.
pub const IID_ID3D12_GRAPHICS_COMMAND_LIST4: Guid = Guid::new(
    0x8754_318e,
    0xd3a9,
    0x4541,
    [0x98, 0xcf, 0x64, 0x5b, 0x50, 0xdc, 0x48, 0x74],
);

/// `ID3D12GraphicsCommandList5`.
pub const IID_ID3D12_GRAPHICS_COMMAND_LIST5: Guid = Guid::new(
    0x5505_0859,
    0x4024,
    0x474c,
    [0x87, 0xf5, 0x64, 0x72, 0xea, 0xee, 0x44, 0xea],
);

/// `ID3D12GraphicsCommandList6`.
pub const IID_ID3D12_GRAPHICS_COMMAND_LIST6: Guid = Guid::new(
    0xc382_7890,
    0xe548,
    0x4cfa,
    [0x96, 0xcf, 0x56, 0x89, 0xa9, 0x37, 0x0f, 0x80],
);

/// `ID3D12GraphicsCommandList7`.
pub const IID_ID3D12_GRAPHICS_COMMAND_LIST7: Guid = Guid::new(
    0xdd17_1223,
    0x8b61,
    0x4769,
    [0x90, 0xe3, 0x16, 0x0c, 0xcd, 0xe4, 0xe2, 0xc1],
);

/// `ID3D12CommandQueue`.
pub const IID_ID3D12_COMMAND_QUEUE: Guid = Guid::new(
    0x0ec8_70a6,
    0x5d7e,
    0x4c22,
    [0x8c, 0xfc, 0x5b, 0xaa, 0xe0, 0x76, 0x16, 0xed],
);

/// `ID3D12PipelineLibrary`.
pub const IID_ID3D12_PIPELINE_LIBRARY: Guid = Guid::new(
    0xc642_26a8,
    0x9201,
    0x46af,
    [0xb4, 0xcc, 0x53, 0xfb, 0x9f, 0xf7, 0x41, 0x4f],
);

/// `ID3D12PipelineLibrary1`.
pub const IID_ID3D12_PIPELINE_LIBRARY1: Guid = Guid::new(
    0x80ea_bf42,
    0x2568,
    0x4e5e,
    [0xbd, 0x82, 0xc3, 0x7f, 0x86, 0x96, 0x1d, 0xc3],
);

/// `ID3D12Device`.
pub const IID_ID3D12_DEVICE: Guid = Guid::new(
    0x1898_19f1,
    0x1db6,
    0x4b57,
    [0xbe, 0x54, 0x18, 0x21, 0x33, 0x9b, 0x85, 0xf7],
);

/// `ID3D12Device1`.
pub const IID_ID3D12_DEVICE1: Guid = Guid::new(
    0x77ac_ce80,
    0x638e,
    0x4e65,
    [0x88, 0x95, 0xc1, 0xf2, 0x33, 0x86, 0x86, 0x3e],
);

/// `ID3D12Device2`.
pub const IID_ID3D12_DEVICE2: Guid = Guid::new(
    0x30ba_a41e,
    0xb15b,
    0x475c,
    [0xa0, 0xbb, 0x1a, 0xf5, 0xc5, 0xb6, 0x43, 0x28],
);

/// `ID3D12Device3`.
pub const IID_ID3D12_DEVICE3: Guid = Guid::new(
    0x81da_dc15,
    0x2bad,
    0x4392,
    [0x93, 0xc5, 0x10, 0x13, 0x45, 0xc4, 0xaa, 0x98],
);

/// `ID3D12Device4`.
pub const IID_ID3D12_DEVICE4: Guid = Guid::new(
    0xe865_df17,
    0xa9ee,
    0x46f9,
    [0xa4, 0x63, 0x30, 0x98, 0x31, 0x5a, 0xa2, 0xe5],
);

/// `ID3D12Device5`.
pub const IID_ID3D12_DEVICE5: Guid = Guid::new(
    0x8b4f_173b,
    0x2fea,
    0x4b80,
    [0x8f, 0x58, 0x43, 0x07, 0x19, 0x1a, 0xb9, 0x5d],
);

/// `ID3D12Device6`.
pub const IID_ID3D12_DEVICE6: Guid = Guid::new(
    0xc70b_221b,
    0x40e4,
    0x4a17,
    [0x89, 0xaf, 0x02, 0x5a, 0x07, 0x27, 0xa6, 0xdc],
);

/// `ID3D12Device7`.
pub const IID_ID3D12_DEVICE7: Guid = Guid::new(
    0x5c01_4b53,
    0x68a1,
    0x4b9b,
    [0x8b, 0xd1, 0xdd, 0x60, 0x46, 0xb9, 0x35, 0x8b],
);

/// `ID3D12Device8`.
pub const IID_ID3D12_DEVICE8: Guid = Guid::new(
    0x9218_e6bb,
    0xf944,
    0x4f7e,
    [0xa7, 0x5c, 0xb1, 0xb2, 0xc7, 0xb7, 0x01, 0xf3],
);

/// `ID3D12Device9`.
pub const IID_ID3D12_DEVICE9: Guid = Guid::new(
    0x4c80_e962,
    0xf032,
    0x4f60,
    [0xbc, 0x9e, 0xeb, 0xc2, 0xcf, 0xa1, 0xd8, 0x3c],
);

/// `ID3D12Device10`.
pub const IID_ID3D12_DEVICE10: Guid = Guid::new(
    0x517f_8718,
    0xaa66,
    0x49f9,
    [0xb0, 0x2b, 0xa7, 0xab, 0x89, 0xc0, 0x60, 0x31],
);

/// Downlevel device query UUID (`d3d12_device.cpp:40`), refused silently without a warning.
pub const IID_D3D12_DEVICE_DOWNLEVEL: Guid = Guid::new(
    0x74ea_ee3f,
    0x2f4b,
    0x476d,
    [0x82, 0xba, 0x2b, 0x85, 0xcb, 0x49, 0xe3, 0x10],
);

/// Downlevel command-queue query UUID (`d3d12_command_queue.cpp:40`), refused silently.
pub const IID_D3D12_COMMAND_QUEUE_DOWNLEVEL: Guid = Guid::new(
    0x38a8_c5ef,
    0x7ccb,
    0x4e81,
    [0x91, 0x4f, 0xa6, 0xe9, 0xd0, 0x72, 0xc4, 0x94],
);

// ---- Capture tools ----

/// `IID_RENDERDOC`.
pub const IID_RENDERDOC: Guid = Guid::new(
    0xa7aa_6116,
    0x9c8d,
    0x4bba,
    [0x90, 0x83, 0xb4, 0xd8, 0x16, 0xb7, 0x1b, 0x78],
);

/// `IID_PIX`.
pub const IID_PIX: Guid = Guid::new(
    0x9f25_1514,
    0x9d4d,
    0x4902,
    [0x9d, 0x60, 0x18, 0x98, 0x8a, 0xb7, 0xd4, 0xb5],
);

/// `IID_GPA`.
pub const IID_GPA: Guid = Guid::new(
    0xccff_ef16,
    0x7b69,
    0x468f,
    [0xbc, 0xe3, 0xcd, 0x95, 0x33, 0x69, 0xa3, 0x9a],
);

/// Every canonical IID above, for the test that no two are the same.
#[cfg(test)]
const ALL: &[Guid] = &[
    IID_IDXGI_OBJECT,
    IID_IDXGI_DEVICE_SUB_OBJECT,
    IID_IDXGI_DEVICE,
    IID_IDXGI_DEVICE_1,
    IID_IDXGI_DEVICE_2,
    IID_IDXGI_DEVICE_3,
    IID_IDXGI_RESOURCE,
    IID_IDXGI_RESOURCE_1,
    IID_IDXGI_SURFACE,
    IID_IDXGI_SURFACE_1,
    IID_IDXGI_SURFACE_2,
    IID_IDXGI_KEYED_MUTEX,
    IID_IDXGI_SWAP_CHAIN,
    IID_IDXGI_SWAP_CHAIN_1,
    IID_IDXGI_SWAP_CHAIN_2,
    IID_IDXGI_SWAP_CHAIN_3,
    IID_IDXGI_SWAP_CHAIN_4,
    IID_ID3D_DEVICE_CONTEXT_STATE,
    IID_ID3D_USER_DEFINED_ANNOTATION,
    IID_IDXGI_FACTORY,
    IID_IDXGI_FACTORY1,
    IID_IDXGI_FACTORY2,
    IID_IDXGI_FACTORY3,
    IID_IDXGI_FACTORY4,
    IID_IDXGI_FACTORY5,
    IID_IDXGI_FACTORY6,
    IID_IDXGI_FACTORY7,
    IID_IDXGI_ADAPTER,
    IID_IDXGI_ADAPTER1,
    IID_IDXGI_ADAPTER2,
    IID_IDXGI_ADAPTER3,
    IID_IDXGI_ADAPTER4,
    IID_IDXGI_OUTPUT,
    IID_IDXGI_OUTPUT1,
    IID_IDXGI_OUTPUT2,
    IID_IDXGI_OUTPUT3,
    IID_IDXGI_OUTPUT4,
    IID_IDXGI_OUTPUT5,
    IID_IDXGI_OUTPUT6,
    IID_WINE_DXGI_ADAPTER,
    IID_IDXGI_DEVICE4,
    IID_ID3D10_DEVICE_CHILD,
    IID_ID3D10_RESOURCE,
    IID_ID3D10_BUFFER,
    IID_ID3D10_TEXTURE_1D,
    IID_ID3D10_TEXTURE_2D,
    IID_ID3D10_TEXTURE_3D,
    IID_ID3D10_VIEW,
    IID_ID3D10_SHADER_RESOURCE_VIEW,
    IID_ID3D10_SHADER_RESOURCE_VIEW_1,
    IID_ID3D10_RENDER_TARGET_VIEW,
    IID_ID3D10_DEPTH_STENCIL_VIEW,
    IID_ID3D10_BLEND_STATE,
    IID_ID3D10_BLEND_STATE_1,
    IID_ID3D10_DEPTH_STENCIL_STATE,
    IID_ID3D10_RASTERIZER_STATE,
    IID_ID3D10_SAMPLER_STATE,
    IID_ID3D10_INPUT_LAYOUT,
    IID_ID3D10_VERTEX_SHADER,
    IID_ID3D10_GEOMETRY_SHADER,
    IID_ID3D10_PIXEL_SHADER,
    IID_ID3D10_ASYNCHRONOUS,
    IID_ID3D10_QUERY,
    IID_ID3D10_PREDICATE,
    IID_ID3D10_COUNTER,
    IID_ID3D10_DEVICE,
    IID_ID3D10_DEVICE1,
    IID_ID3D10_BLOB,
    IID_ID3D11_DEVICE_CHILD,
    IID_ID3D11_RESOURCE,
    IID_ID3D11_BUFFER,
    IID_ID3D11_TEXTURE_1D,
    IID_ID3D11_TEXTURE_2D,
    IID_ID3D11_TEXTURE_2D1,
    IID_ID3D11_TEXTURE_3D,
    IID_ID3D11_TEXTURE_3D1,
    IID_ID3D11_VIEW,
    IID_ID3D11_SHADER_RESOURCE_VIEW,
    IID_ID3D11_SHADER_RESOURCE_VIEW_1,
    IID_ID3D11_RENDER_TARGET_VIEW,
    IID_ID3D11_RENDER_TARGET_VIEW_1,
    IID_ID3D11_DEPTH_STENCIL_VIEW,
    IID_ID3D11_UNORDERED_ACCESS_VIEW,
    IID_ID3D11_UNORDERED_ACCESS_VIEW_1,
    IID_ID3D11_BLEND_STATE,
    IID_ID3D11_BLEND_STATE_1,
    IID_ID3D11_DEPTH_STENCIL_STATE,
    IID_ID3D11_RASTERIZER_STATE,
    IID_ID3D11_RASTERIZER_STATE_1,
    IID_ID3D11_RASTERIZER_STATE_2,
    IID_ID3D11_SAMPLER_STATE,
    IID_ID3D11_INPUT_LAYOUT,
    IID_ID3D11_VERTEX_SHADER,
    IID_ID3D11_HULL_SHADER,
    IID_ID3D11_DOMAIN_SHADER,
    IID_ID3D11_GEOMETRY_SHADER,
    IID_ID3D11_PIXEL_SHADER,
    IID_ID3D11_COMPUTE_SHADER,
    IID_ID3D11_ASYNCHRONOUS,
    IID_ID3D11_QUERY,
    IID_ID3D11_QUERY_1,
    IID_ID3D11_PREDICATE,
    IID_ID3D11_COUNTER,
    IID_ID3D11_CLASS_INSTANCE,
    IID_ID3D11_CLASS_LINKAGE,
    IID_ID3D11_COMMAND_LIST,
    IID_ID3D11_DEVICE_CONTEXT,
    IID_ID3D11_DEVICE_CONTEXT_1,
    IID_ID3D11_DEVICE_CONTEXT_2,
    IID_ID3D11_DEVICE_CONTEXT_3,
    IID_ID3D11_DEVICE_CONTEXT_4,
    IID_ID3D11_FENCE,
    IID_ID3D11_DEVICE,
    IID_ID3D11_DEVICE_1,
    IID_ID3D11_DEVICE_2,
    IID_ID3D11_DEVICE_3,
    IID_ID3D11_DEVICE_4,
    IID_ID3D11_DEVICE_5,
    IID_ID3D11_MULTITHREAD,
    IID_ID3D11_DEBUG,
    IID_ID3D11_INFO_QUEUE,
    IID_ID3D12_OBJECT,
    IID_ID3D12_DEVICE_CHILD,
    IID_ID3D12_PAGEABLE,
    IID_ID3D12_ROOT_SIGNATURE,
    IID_ID3D12_ROOT_SIGNATURE_DESERIALIZER,
    IID_ID3D12_VERSIONED_ROOT_SIGNATURE_DESERIALIZER,
    IID_ID3D12_HEAP,
    IID_ID3D12_HEAP1,
    IID_ID3D12_RESOURCE,
    IID_ID3D12_RESOURCE1,
    IID_ID3D12_RESOURCE2,
    IID_ID3D12_COMMAND_ALLOCATOR,
    IID_ID3D12_FENCE,
    IID_ID3D12_FENCE1,
    IID_ID3D12_PIPELINE_STATE,
    IID_ID3D12_DESCRIPTOR_HEAP,
    IID_ID3D12_QUERY_HEAP,
    IID_ID3D12_COMMAND_SIGNATURE,
    IID_ID3D12_COMMAND_LIST,
    IID_ID3D12_GRAPHICS_COMMAND_LIST,
    IID_ID3D12_GRAPHICS_COMMAND_LIST1,
    IID_ID3D12_GRAPHICS_COMMAND_LIST2,
    IID_ID3D12_GRAPHICS_COMMAND_LIST3,
    IID_ID3D12_GRAPHICS_COMMAND_LIST4,
    IID_ID3D12_GRAPHICS_COMMAND_LIST5,
    IID_ID3D12_GRAPHICS_COMMAND_LIST6,
    IID_ID3D12_GRAPHICS_COMMAND_LIST7,
    IID_ID3D12_COMMAND_QUEUE,
    IID_ID3D12_PIPELINE_LIBRARY,
    IID_ID3D12_PIPELINE_LIBRARY1,
    IID_ID3D12_DEVICE,
    IID_ID3D12_DEVICE1,
    IID_ID3D12_DEVICE2,
    IID_ID3D12_DEVICE3,
    IID_ID3D12_DEVICE4,
    IID_ID3D12_DEVICE5,
    IID_ID3D12_DEVICE6,
    IID_ID3D12_DEVICE7,
    IID_ID3D12_DEVICE8,
    IID_ID3D12_DEVICE9,
    IID_ID3D12_DEVICE10,
    IID_D3D12_DEVICE_DOWNLEVEL,
    IID_D3D12_COMMAND_QUEUE_DOWNLEVEL,
    IID_RENDERDOC,
    IID_PIX,
    IID_GPA,
];

#[cfg(test)]
mod tests;
