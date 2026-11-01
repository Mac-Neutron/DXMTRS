use core::ptr;

use dxmt_core::capture::NextAction;
use winemetal_wire::{
    MemPtr, WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER, WMT_BLIT_COMMAND_COPY_TEXTURE,
    WMT_BLIT_COMMAND_GENERATE_MIPMAPS, WMT_BLIT_OPTION_DEPTH_FROM_DEPTH_STENCIL,
    WMT_BLIT_OPTION_NONE, WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8,
    WMT_PIXEL_FORMAT_RGBA8_UNORM, WMTBufferInfo, WmtcmdBlitCopyFromBufferToBuffer,
    WmtcmdBlitCopyTexture, WmtcmdBlitGenerateMipmaps, WmtcmdBlitNop, WmtcmdComputeNop,
    WmtcmdRenderNop,
};

use super::*;
use crate::encoder::{
    BlitEncoderData, EncoderData, EncoderType, RenderEncoderColorAttachmentData, RenderEncoderData,
    RenderEncoderDepthAttachmentData, RenderEncoderStencilAttachmentData,
};

#[cfg(target_pointer_width = "64")]
const fn null_wire_ptr() -> MemPtr {
    MemPtr { ptr: 0 }
}

#[cfg(target_pointer_width = "32")]
const fn null_wire_ptr() -> MemPtr {
    MemPtr { ptr: 0, high: 0 }
}

#[cfg(target_pointer_width = "64")]
fn wire_ptr<T>(p: *mut T) -> MemPtr {
    MemPtr {
        ptr: p.expose_provenance() as u64,
    }
}

#[cfg(target_pointer_width = "32")]
fn wire_ptr<T>(p: *mut T) -> MemPtr {
    MemPtr {
        ptr: u32::try_from(p.expose_provenance()).unwrap_or(0),
        high: 0,
    }
}

fn empty_render(id: u64) -> RenderEncoderData {
    RenderEncoderData {
        base: EncoderData::new(EncoderType::Render, id),
        colors: core::array::from_fn(|_| RenderEncoderColorAttachmentData::default()),
        depth: RenderEncoderDepthAttachmentData::default(),
        stencil: RenderEncoderStencilAttachmentData::default(),
        default_raster_sample_count: 1,
        render_target_array_length: 1,
        render_target_height: 1080,
        render_target_width: 1920,
        cmd_head: WmtcmdRenderNop {
            r#type: 0,
            reserved: [0; 3],
            next: null_wire_ptr(),
        },
        cmd_tail: ptr::null_mut(),
        dsv_planar_flags: 0,
        dsv_readonly_flags: 0,
        render_target_count: 0,
        use_visibility_result: false,
        visibility_buffer: 0,
        use_tessellation: false,
        use_geometry: false,
        uav_writes: false,
        pre_head: WmtcmdComputeNop {
            r#type: 0,
            reserved: [0; 3],
            next: null_wire_ptr(),
        },
        pre_tail: ptr::null_mut(),
    }
}

#[test]
fn passes_txt_as_cpp() {
    let mut dumps = PassDumps::for_range(0, 1);
    assert!(dumps.is_dumping());

    let null_pass = EncoderData::new(EncoderType::Null, 1);
    let upscale_pass = EncoderData::new(EncoderType::TemporalUpscale, 2);

    let mut mips = WmtcmdBlitGenerateMipmaps {
        r#type: WMT_BLIT_COMMAND_GENERATE_MIPMAPS,
        reserved: [0; 3],
        next: null_wire_ptr(),
        texture: 30,
    };
    let mut copy_tex = WmtcmdBlitCopyTexture {
        r#type: WMT_BLIT_COMMAND_COPY_TEXTURE,
        reserved: [0; 3],
        next: wire_ptr(&raw mut mips),
        src: 10,
        dst: 20,
    };
    let mut copy_buf = WmtcmdBlitCopyFromBufferToBuffer {
        r#type: WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER,
        reserved: [0; 3],
        next: wire_ptr(&raw mut copy_tex),
        src: 1,
        src_offset: 0,
        dst: 2,
        dst_offset: 0,
        copy_length: 4096,
    };

    let blit_pass = BlitEncoderData {
        base: EncoderData::new(EncoderType::Blit, 3),
        cmd_head: WmtcmdBlitNop {
            r#type: 0,
            reserved: [0; 3],
            next: wire_ptr(&raw mut copy_buf),
        },
        cmd_tail: ptr::null_mut(),
    };

    let mut ts_blit = BlitEncoderData {
        base: EncoderData::new(EncoderType::Blit, 4),
        cmd_head: WmtcmdBlitNop {
            r#type: 0,
            reserved: [0; 3],
            next: null_wire_ptr(),
        },
        cmd_tail: ptr::null_mut(),
    };
    ts_blit.base.timestamp_only = true;
    ts_blit.base.query_resolve_only = true;

    let clear_pass = EncoderData::new(EncoderType::Clear, 5);
    let compute_pass = EncoderData::new(EncoderType::Compute, 6);
    let resolve_pass = EncoderData::new(EncoderType::Resolve, 7);
    let render_pass = empty_render(8);

    assert_eq!(pass_label(0, &null_pass), "pass-0 null");
    assert_eq!(pass_label(1, &upscale_pass), "pass-1 upscale");
    assert_eq!(
        pass_label(2, &blit_pass.base),
        "pass-2 blit buf>buf:4096 copytex mips"
    );
    assert_eq!(
        pass_label(3, &ts_blit.base),
        "pass-3 blit timestamps query-resolve"
    );
    assert_eq!(pass_label(4, &clear_pass), "pass-4 clear");
    assert_eq!(pass_label(5, &compute_pass), "pass-5 compute");
    assert_eq!(pass_label(6, &resolve_pass), "pass-6 resolve");
    assert_eq!(pass_label(7, &render_pass.base), "pass-7 render");
    assert_eq!(
        format_encoder_label(7, "render", Some(&render_pass)),
        "pass-7 render 0c"
    );

    dump_pass(&mut dumps, &null_pass, 0, 12, None, None);
    dump_pass(&mut dumps, &upscale_pass, 0, 12, None, None);
    dump_pass(&mut dumps, &blit_pass.base, 0, 12, None, None);
    dump_pass(&mut dumps, &ts_blit.base, 1, 3, None, None);

    let c0_meta = format_attachment_meta(&AttachmentSpec {
        name: "pass-4-c0",
        tex_width: 1920,
        tex_height: 1080,
        samples: 1,
        format: WMT_PIXEL_FORMAT_BGRA8_UNORM,
        level: 0,
        slice: 0,
        depth_plane: 0,
        option: WMT_BLIT_OPTION_NONE,
        alloc_tex: 42,
    });
    assert_eq!(c0_meta.file, "pass-4-c0-1920x1080-80");
    assert_eq!(c0_meta.tag, " pass-4-c0-1920x1080-80@42");
    assert_eq!(c0_meta.texel_bytes, 4);
    assert_eq!(c0_meta.size, 1920 * 1080 * 4);

    let d_meta = format_attachment_meta(&AttachmentSpec {
        name: "pass-4-d",
        tex_width: 1920,
        tex_height: 1080,
        samples: 1,
        format: WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8,
        level: 1,
        slice: 2,
        depth_plane: 0,
        option: WMT_BLIT_OPTION_DEPTH_FROM_DEPTH_STENCIL,
        alloc_tex: 99,
    });
    assert_eq!(d_meta.file, "pass-4-d-960x540-260");
    assert_eq!(d_meta.tag, " pass-4-d-960x540-260@99");
    assert_eq!(d_meta.texel_bytes, 4);
    assert_eq!(d_meta.level, 1);
    assert_eq!(d_meta.slice, 2);

    let msaa_meta = format_attachment_meta(&AttachmentSpec {
        name: "pass-4-c1",
        tex_width: 1920,
        tex_height: 1080,
        samples: 4,
        format: WMT_PIXEL_FORMAT_RGBA8_UNORM,
        level: 0,
        slice: 0,
        depth_plane: 0,
        option: WMT_BLIT_OPTION_NONE,
        alloc_tex: 77,
    });
    assert_eq!(msaa_meta.tag, " pass-4-c1-1920x1080-70-msaa@77");

    dumps.note_present();
    assert_eq!(
        dumps.log,
        concat!(
            "pass-0 q0 x12 null\n",
            "pass-1 q0 x12 upscale\n",
            "pass-2 q0 x12 blit buf>buf:4096 copytex mips\n",
            "pass-3 q1 x3 blit timestamps query-resolve\n",
            "# frame 0 presented\n",
        )
    );

    let parsed =
        PassDumps::from_env_vars(true, Some("10"), Some("3"), Some("100,200+300,400,2,8,seq"));
    assert_eq!(parsed.frame, 10);
    assert_eq!(parsed.count, 3);
    assert!(!parsed.on_key);
    assert!(parsed.pixel_on);
    assert!(parsed.pixel_seq);
    assert_eq!(parsed.pixels, vec![(100, 200), (300, 400)]);
    assert_eq!(parsed.pixel_first, 2);
    assert_eq!(parsed.pixel_last, 8);

    let mut pix_dumps = PassDumps::for_range(0, 1);
    pix_dumps.pixel_on = true;
    pix_dumps.pixel_notes = String::from("# pass-2: 2 draws redrawn alone\n");
    let mut raw_slots = [0u8; 3 * 16];
    raw_slots[..4].copy_from_slice(&[0x10, 0x20, 0x30, 0x40]);
    raw_slots[16..20].copy_from_slice(&[0x10, 0x20, 0x30, 0x40]);
    raw_slots[32..36].copy_from_slice(&[0xff, 0x00, 0x80, 0x7f]);
    pix_dumps.pixel_passes.push(PixelPass {
        pass: String::from("pass-2"),
        seq: false,
        points: vec![(100, 200)],
        draws: vec![String::from("gfx vs=1"), String::from("gfx vs=2")],
        colors: vec![0],
        texels: vec![4],
        buffer: None,
        info: WMTBufferInfo {
            length: 48,
            options: 0,
            memory: wire_ptr(raw_slots.as_mut_ptr()),
            gpu_address: 0,
        },
    });
    assert_eq!(
        pix_dumps.format_pixel_history(),
        concat!(
            "# pass-2: 2 draws redrawn alone\n",
            "pass-2 draw-1 gfx vs=2 c0 10203040->ff00807f at 100,200\n",
        )
    );
}

#[test]
fn f9_folder_numbering() {
    let mut dumps = PassDumps::for_f9(2);
    assert!(!dumps.is_dumping());
    assert!(dumps.on_key);
    assert_eq!(dumps.frame, u64::MAX);

    let mut created = Vec::<String>::new();
    let mut make_dir = |name: &str| -> bool {
        created.push(name.to_owned());
        name != "f9-1" && name != "f9-2"
    };

    assert!(!dumps.step_present(false, &mut make_dir));
    assert_eq!(dumps.frames, 1);
    assert!(!dumps.is_dumping());

    assert!(!dumps.step_present(true, &mut make_dir));
    assert_eq!(dumps.frame, 2);
    assert_eq!(dumps.frames, 2);
    assert!(dumps.is_dumping());

    for expected_frames in 3..=10 {
        assert!(!dumps.step_present(false, &mut make_dir));
        assert_eq!(dumps.frames, expected_frames);
    }

    let mut saved_prefix = None;
    let should_save = dumps.step_present(false, &mut |name| {
        let ok = make_dir(name);
        if ok {
            saved_prefix = Some(format!("{name}\\"));
        }
        ok
    });
    assert!(should_save);
    assert_eq!(dumps.frames, 11);
    assert_eq!(
        created,
        vec!["f9-1".to_owned(), "f9-2".to_owned(), "f9-3".to_owned()]
    );
    assert_eq!(saved_prefix.as_deref(), Some("f9-3\\"));
    assert_eq!(dumps.saved, 3);
    assert_eq!(dumps.frame, u64::MAX);
    assert!(!dumps.is_dumping());

    let mut next_created = Vec::<String>::new();
    let prefix = dumps.next_f9_dir(|name| {
        next_created.push(name.to_owned());
        true
    });
    assert_eq!(prefix, "f9-4\\");
    assert_eq!(next_created, vec!["f9-4".to_owned()]);
    assert_eq!(dumps.saved, 4);

    dumps.frame = 11;
    dumps.log.push_str("pass-0 q0 x1 null\n");
    let mut saved_files = Vec::<(String, String)>::new();
    dumps.save_with(
        |_| true,
        |path, bytes| {
            saved_files.push((path.to_owned(), String::from_utf8_lossy(bytes).into_owned()));
        },
    );
    assert_eq!(
        saved_files,
        vec![(
            "f9-5\\passes.txt".to_owned(),
            "pass-0 q0 x1 null\n".to_owned()
        )]
    );
    assert_eq!(dumps.saved, 5);
    assert_eq!(dumps.frame, u64::MAX);
}

#[test]
fn capture_schedule_as_cpp() {
    let mut sched = CaptureSchedule::with_env(Some("2"));
    assert_eq!(sched.presented_frames(), 0);

    assert_eq!(sched.step_present(false), NextAction::Nothing);
    assert_eq!(sched.presented_frames(), 1);

    assert_eq!(sched.step_present(false), NextAction::StartCapture);
    assert_eq!(sched.presented_frames(), 2);

    assert_eq!(sched.step_present(false), NextAction::StopCapture);
    assert_eq!(sched.presented_frames(), 3);

    assert_eq!(sched.step_present(false), NextAction::Nothing);
    assert_eq!(sched.presented_frames(), 4);

    assert_eq!(sched.step_present(true), NextAction::Nothing);
    assert_eq!(sched.presented_frames(), 5);

    assert_eq!(sched.step_present(false), NextAction::StartCapture);
    assert_eq!(sched.presented_frames(), 6);

    assert_eq!(sched.step_present(false), NextAction::StopCapture);
    assert_eq!(sched.presented_frames(), 7);
}
