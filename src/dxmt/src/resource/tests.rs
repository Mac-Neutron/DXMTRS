use rustc_hash::FxHashSet;
use winemetal_wire::{
    WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_PIXEL_FORMAT_RGBA8_UNORM,
    WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED, WMT_RESOURCE_OPTION_CPU_CACHE_MODE_WRITE_COMBINED,
    WMT_TEXTURE_TYPE_CUBE, WMT_TEXTURE_TYPE_CUBE_ARRAY, WMT_TEXTURE_TYPE_TEXTURE_BUFFER,
    WMT_TEXTURE_TYPE1D, WMT_TEXTURE_TYPE1D_ARRAY, WMT_TEXTURE_TYPE2D, WMT_TEXTURE_TYPE2D_ARRAY,
    WMT_TEXTURE_TYPE2D_MULTISAMPLE, WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY, WMT_TEXTURE_TYPE3D,
};

use super::{
    buffer::{Suballocation, over_info},
    *,
};

const fn desc(
    format: u32,
    texture_type: u32,
    first_mip: u32,
    mip_count: u32,
    first_slice: u32,
    array_size: u32,
) -> TextureViewDescriptor {
    TextureViewDescriptor {
        format,
        texture_type,
        first_mip,
        mip_count,
        first_slice,
        array_size,
    }
}

fn fields(d: &TextureViewDescriptor) -> [u32; 6] {
    [
        d.format,
        d.texture_type,
        d.first_mip,
        d.mip_count,
        d.first_slice,
        d.array_size,
    ]
}

#[test]
fn suballocation_stays_in_one_page() {
    for length in 1..=PAGE_SIZE {
        let s = Suballocation::of(length, BufferAllocationFlags::SUBALLOCATE_FROM_ONE_PAGE);
        assert_eq!(s.info_length, PAGE_SIZE, "length {length}");
        assert!(s.count >= 1, "length {length}");
        assert!(
            s.size >= length.max(256) && s.size.is_multiple_of(16),
            "length {length}"
        );
        for sub in 0..s.count {
            assert!(
                s.offset(sub) + s.size <= PAGE_SIZE,
                "length {length} sub {sub}"
            );
        }
        assert!(
            s.offset(s.count) + s.size > PAGE_SIZE,
            "length {length}: the page holds no more"
        );
    }
    let s = Suballocation::of(
        PAGE_SIZE + 1,
        BufferAllocationFlags::SUBALLOCATE_FROM_ONE_PAGE,
    );
    assert_eq!(
        (s.info_length, s.size, s.count),
        (PAGE_SIZE + 1, PAGE_SIZE + 1, 1)
    );
    let s = Suballocation::of(100, BufferAllocationFlags::empty());
    assert_eq!((s.info_length, s.size, s.count), (256, 256, 1));
}

#[test]
fn allocate_over_rounds_to_16k() {
    let mut page = vec![0u8; 3 * 16384];
    let ptr = page.as_mut_ptr();
    for (length, rounded) in [
        (1, 16384),
        (16383, 16384),
        (16384, 16384),
        (16385, 32768),
        (32768, 32768),
    ] {
        let external = External {
            ptr,
            len: page.len(),
        };
        let info = over_info(length, BufferAllocationFlags::CPU_WRITE_COMBINED, &external);
        assert_eq!(info.length, rounded, "length {length}");
        assert_eq!(
            info.options,
            WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED
                | WMT_RESOURCE_OPTION_CPU_CACHE_MODE_WRITE_COMBINED
        );
        assert_eq!(info.memory.as_ptr::<u8>(), ptr);
        assert_eq!(
            Suballocation::of(info.length, BufferAllocationFlags::empty()).info_length,
            rounded
        );
    }
    let info = over_info(
        1,
        BufferAllocationFlags::empty(),
        &External {
            ptr,
            len: page.len(),
        },
    );
    assert_eq!(info.options, WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED);
}

#[test]
fn view_keys_round_trip() {
    let descs = [
        desc(WMT_PIXEL_FORMAT_RGBA8_UNORM, WMT_TEXTURE_TYPE2D, 0, 1, 0, 1),
        desc(
            0x95_2105,
            WMT_TEXTURE_TYPE_TEXTURE_BUFFER,
            15,
            15,
            4095,
            4095,
        ),
        desc(0xFF_FFFF, 0xFF, 0, 0, 0, 0),
        desc(
            WMT_PIXEL_FORMAT_BGRA8_UNORM,
            WMT_TEXTURE_TYPE_CUBE_ARRAY,
            3,
            4,
            12,
            6,
        ),
    ];
    for d in &descs {
        let key = d.key();
        assert_eq!(fields(&key.descriptor()), fields(d));
        assert_eq!(key.descriptor().key(), key);
        assert_eq!(
            [
                key.format(),
                key.texture_type(),
                key.first_mip(),
                key.mip_count(),
                key.first_slice(),
                key.array_size()
            ],
            fields(d)
        );
    }
}

#[test]
fn distinct_descriptors_distinct_keys() {
    let base = desc(WMT_PIXEL_FORMAT_RGBA8_UNORM, WMT_TEXTURE_TYPE2D, 0, 1, 0, 1);
    let mut keys = FxHashSet::default();
    keys.insert(base.key());
    let mut count = 1;
    let mut add = |d: TextureViewDescriptor| {
        if fields(&d) != fields(&base) {
            assert!(keys.insert(d.key()), "{:?} collides", fields(&d));
            count += 1;
        }
    };
    for format in (0..1 << 24).step_by(4093).chain([0xFF_FFFF]) {
        add(TextureViewDescriptor { format, ..base });
    }
    for texture_type in 0..=0xFF {
        add(TextureViewDescriptor {
            texture_type,
            ..base
        });
    }
    for v in 0..16 {
        add(TextureViewDescriptor {
            first_mip: v,
            ..base
        });
        add(TextureViewDescriptor {
            mip_count: v,
            ..base
        });
    }
    for v in 0..4096 {
        add(TextureViewDescriptor {
            first_slice: v,
            ..base
        });
        add(TextureViewDescriptor {
            array_size: v,
            ..base
        });
    }
    assert!(count > 8000);
}

#[test]
fn retained_seq_is_the_latest() {
    let r = Retained::default();
    assert_eq!(r.last(), 0);
    assert!(!r.check(5), "a new sequence is not retained yet");
    assert!(r.check(5), "the same sequence again is");
    assert!(!r.check(7));
    assert_eq!(r.last(), 7);
    assert!(!r.check(5), "an older sequence takes over as the C++ does");
    assert_eq!(r.last(), 5);
}

#[test]
fn array_forms_as_cpp() {
    let cases = [
        (WMT_TEXTURE_TYPE1D, true, WMT_TEXTURE_TYPE1D_ARRAY, 1),
        (WMT_TEXTURE_TYPE1D_ARRAY, false, WMT_TEXTURE_TYPE1D, 1),
        (WMT_TEXTURE_TYPE2D, true, WMT_TEXTURE_TYPE2D_ARRAY, 1),
        (WMT_TEXTURE_TYPE2D_ARRAY, false, WMT_TEXTURE_TYPE2D, 1),
        (
            WMT_TEXTURE_TYPE2D_MULTISAMPLE,
            true,
            WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY,
            1,
        ),
        (
            WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY,
            false,
            WMT_TEXTURE_TYPE2D_MULTISAMPLE,
            1,
        ),
        (WMT_TEXTURE_TYPE_CUBE, true, WMT_TEXTURE_TYPE_CUBE_ARRAY, 6),
        (WMT_TEXTURE_TYPE_CUBE_ARRAY, false, WMT_TEXTURE_TYPE_CUBE, 6),
    ];
    for (from, is_array, to, array_size) in cases {
        let key = desc(WMT_PIXEL_FORMAT_RGBA8_UNORM, from, 2, 3, 4, 12).key();
        assert_eq!(key.with_array(!is_array), key, "type {from} already fits");
        let changed = key.with_array(is_array);
        assert_eq!(
            fields(&changed.descriptor()),
            [WMT_PIXEL_FORMAT_RGBA8_UNORM, to, 2, 3, 4, array_size],
            "type {from}"
        );
    }
    for other in [WMT_TEXTURE_TYPE3D, WMT_TEXTURE_TYPE_TEXTURE_BUFFER] {
        let key = desc(WMT_PIXEL_FORMAT_RGBA8_UNORM, other, 0, 1, 0, 1).key();
        assert_eq!(key.with_array(true), key);
        assert_eq!(key.with_array(false), key);
    }
    let key = desc(WMT_PIXEL_FORMAT_RGBA8_UNORM, WMT_TEXTURE_TYPE2D, 1, 2, 3, 4).key();
    assert_eq!(key.with_format(WMT_PIXEL_FORMAT_RGBA8_UNORM), key);
    assert_eq!(
        fields(&key.with_format(WMT_PIXEL_FORMAT_BGRA8_UNORM).descriptor()),
        [WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_TEXTURE_TYPE2D, 1, 2, 3, 4]
    );
}
