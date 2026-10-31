//! A context that writes each call it gets as a line, and the resources to call it with.

use std::{ffi::CStr, sync::Arc};

use winemetal_sys::{
    Obj,
    wire::{WMTScissorRect, WMTSize, WMTTextureInfo, WMTViewport},
};

use super::{Library, ResourceAccess, SimpleCommandContext};
use crate::resource::{Buffer, BufferViewKey, Texture, TextureViewDescriptor, TextureViewKey};

pub fn obj(handle: u64) -> Obj {
    // SAFETY: a nonzero fake handle; the test binary's release fake takes it back.
    unsafe { Obj::adopt(handle) }.expect("a nonzero handle")
}

pub fn texture(
    texture_type: u32,
    format: u32,
    [width, height, depth]: [u32; 3],
    array_length: u32,
) -> Arc<Texture> {
    // The type, then three levels, one sample and no usage.
    let bits = texture_type | (3 << 8) | (1 << 16);
    let info = WMTTextureInfo {
        pixel_format: format,
        width,
        height,
        depth,
        array_length,
        bits,
        options: 0,
        flags: 0,
        mach_port: 0,
        gpu_resource_id: 0,
    };
    Arc::new(Texture::new(info, &obj(1)))
}

/// A context that writes each call it gets as a line.
pub struct Rec {
    library: Library,
    /// The pipelines `compute_pipeline` was asked for, by name; handle 101 is the first.
    pub names: Vec<String>,
    calls: Vec<String>,
}

impl Rec {
    pub fn new() -> Self {
        Self {
            library: Library { library: obj(2) },
            names: Vec::new(),
            calls: Vec::new(),
        }
    }

    fn say(&mut self, line: String) {
        self.calls.push(line);
    }

    /// The lines written since the last call.
    pub fn take(&mut self) -> Vec<String> {
        std::mem::take(&mut self.calls)
    }

    /// Asserts that nothing was written since the last call.
    pub fn assert_quiet(&mut self) {
        assert_eq!(self.take(), Vec::<String>::new());
    }
}

fn words(bytes: &[u8]) -> Vec<u32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|w| u32::from_le_bytes(*w))
        .collect()
}

impl SimpleCommandContext for Rec {
    fn compute_pipeline(&mut self, name: &CStr) -> Option<Obj> {
        self.names
            .push(name.to_str().expect("an ASCII name").to_owned());
        Some(obj(
            u64::try_from(self.names.len()).expect("a few names") + 100
        ))
    }

    fn start_compute_pass(&mut self) {
        self.say("start_compute_pass".to_owned());
    }

    fn end_pass(&mut self) {
        self.say("end_pass".to_owned());
    }

    fn set_compute_pso(&mut self, pso: &Obj, size: WMTSize) {
        let name = self.names[usize::try_from(pso.handle() - 101).expect("a small index")].clone();
        self.say(format!(
            "set_compute_pso {name} {}x{}x{}",
            size.width, size.height, size.depth
        ));
    }

    fn dispatch(&mut self, size: WMTSize) {
        self.say(format!(
            "dispatch {}x{}x{}",
            size.width, size.height, size.depth
        ));
    }

    fn set_compute_texture(
        &mut self,
        index: u32,
        _texture: &Arc<Texture>,
        view: TextureViewKey,
        access: ResourceAccess,
    ) {
        self.say(format!(
            "set_compute_texture {index} format={} type={} access={}",
            view.format(),
            view.texture_type(),
            access.bits()
        ));
    }

    fn set_compute_texel_buffer(
        &mut self,
        _index: u32,
        _buffer: &Arc<Buffer>,
        _view: BufferViewKey,
        _access: ResourceAccess,
    ) {
        self.say("set_compute_texel_buffer".to_owned());
    }

    fn set_compute_buffer(
        &mut self,
        _index: u32,
        _buffer: &Arc<Buffer>,
        _offset: u32,
        _length: u32,
        _access: ResourceAccess,
    ) {
        self.say("set_compute_buffer".to_owned());
    }

    fn set_compute_bytes(&mut self, index: u32, bytes: &[u8]) {
        self.say(format!("set_compute_bytes {index} {:x?}", words(bytes)));
    }

    fn default_library(&self) -> &Library {
        &self.library
    }

    fn start_render_pass(&mut self) {
        self.say("start_render_pass".to_owned());
    }

    fn set_color_attachment(
        &mut self,
        _index: u32,
        _texture: &Arc<Texture>,
        _view: TextureViewKey,
        _depth_plane: u32,
    ) {
        self.say("set_color_attachment".to_owned());
    }

    fn set_depth_stencil_attachment(
        &mut self,
        _texture: &Arc<Texture>,
        _view: TextureViewKey,
        _dsv_flag: u32,
    ) {
        self.say("set_depth_stencil_attachment".to_owned());
    }

    fn set_render_pso(&mut self, _pso: &Obj) {
        self.say("set_render_pso".to_owned());
    }

    fn set_viewport(&mut self, _viewport: WMTViewport) {
        self.say("set_viewport".to_owned());
    }

    fn set_depth_stencil_state(&mut self, _state: &Obj) {
        self.say("set_depth_stencil_state".to_owned());
    }

    fn set_scissor_rect(&mut self, rect: WMTScissorRect) {
        self.say(format!(
            "set_scissor_rect {} {} {} {}",
            rect.x, rect.y, rect.width, rect.height
        ));
    }

    fn set_stencil_reference(&mut self, stencil_ref: u8) {
        self.say(format!("set_stencil_reference {stencil_ref}"));
    }

    fn set_fragment_bytes(&mut self, index: u32, bytes: &[u8]) {
        self.say(format!("set_fragment_bytes {index} {:x?}", words(bytes)));
    }

    fn draw(&mut self, primitive: u8, start: u32, count: u32, base_instance: i32, instances: u32) {
        self.say(format!(
            "draw {primitive} {start} {count} {base_instance} {instances}"
        ));
    }
}

pub fn view(texture_type: u32, format: u32, first_mip: u32, array_size: u32) -> TextureViewKey {
    TextureViewDescriptor {
        format,
        texture_type,
        first_mip,
        mip_count: 1,
        first_slice: 0,
        array_size,
    }
    .key()
}
