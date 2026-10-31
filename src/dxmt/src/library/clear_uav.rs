//! Clearing an unordered-access view with a compute pass.

use std::sync::Arc;

use winemetal_sys::{
    Obj,
    wire::{
        WMT_PIXEL_FORMAT_A8_UNORM, WMT_PIXEL_FORMAT_BGR10_XR, WMT_PIXEL_FORMAT_BGR10_XR_S_RGB,
        WMT_PIXEL_FORMAT_BGR10A2_UNORM, WMT_PIXEL_FORMAT_BGRA8_UNORM,
        WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB, WMT_PIXEL_FORMAT_BGRX8_UNORM,
        WMT_PIXEL_FORMAT_BGRX8_UNORM_S_RGB, WMT_PIXEL_FORMAT_R8_SINT, WMT_PIXEL_FORMAT_R8_SNORM,
        WMT_PIXEL_FORMAT_R8_UINT, WMT_PIXEL_FORMAT_R8_UNORM, WMT_PIXEL_FORMAT_R8_UNORM_S_RGB,
        WMT_PIXEL_FORMAT_R16_FLOAT, WMT_PIXEL_FORMAT_R16_SINT, WMT_PIXEL_FORMAT_R16_SNORM,
        WMT_PIXEL_FORMAT_R16_UINT, WMT_PIXEL_FORMAT_R16_UNORM, WMT_PIXEL_FORMAT_R32_UINT,
        WMT_PIXEL_FORMAT_RG8_SINT, WMT_PIXEL_FORMAT_RG8_SNORM, WMT_PIXEL_FORMAT_RG8_UINT,
        WMT_PIXEL_FORMAT_RG8_UNORM, WMT_PIXEL_FORMAT_RG8_UNORM_S_RGB,
        WMT_PIXEL_FORMAT_RG11B10_FLOAT, WMT_PIXEL_FORMAT_RG16_FLOAT, WMT_PIXEL_FORMAT_RG16_SINT,
        WMT_PIXEL_FORMAT_RG16_SNORM, WMT_PIXEL_FORMAT_RG16_UINT, WMT_PIXEL_FORMAT_RG16_UNORM,
        WMT_PIXEL_FORMAT_RGB9E5_FLOAT, WMT_PIXEL_FORMAT_RGB10A2_UINT,
        WMT_PIXEL_FORMAT_RGB10A2_UNORM, WMT_PIXEL_FORMAT_RGBA8_SINT, WMT_PIXEL_FORMAT_RGBA8_SNORM,
        WMT_PIXEL_FORMAT_RGBA8_UINT, WMT_PIXEL_FORMAT_RGBA8_UNORM,
        WMT_PIXEL_FORMAT_RGBA8_UNORM_S_RGB, WMT_PIXEL_FORMAT_RGBA16_FLOAT,
        WMT_PIXEL_FORMAT_RGBA16_SINT, WMT_PIXEL_FORMAT_RGBA16_SNORM, WMT_PIXEL_FORMAT_RGBA16_UINT,
        WMT_PIXEL_FORMAT_RGBA16_UNORM, WMT_TEXTURE_TYPE_CUBE, WMT_TEXTURE_TYPE_CUBE_ARRAY,
        WMT_TEXTURE_TYPE1D_ARRAY, WMT_TEXTURE_TYPE2D_ARRAY, WMT_TEXTURE_TYPE2D_MULTISAMPLE,
        WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY, WMT_TEXTURE_TYPE3D, WMTSize,
    },
};

use super::{ResourceAccess, SimpleCommandContext};
use crate::{
    format,
    resource::{Buffer, BufferViewDescriptor, BufferViewKey, Texture, TextureViewKey},
};

/// The threadgroup of every clear: 32 threads in a row.
const THREADGROUP: WMTSize = WMTSize {
    width: 32,
    height: 1,
    depth: 1,
};

/// A clear of a texture view, a texel-buffer view or a buffer, one `begin` to its `end`.
///
/// `begin_*` starts the compute pass and picks the pipeline for the target's type and the colour's
/// kind; each [`Self::clear`] dispatches over one rectangle; [`Self::end`] ends the pass.
pub struct ClearUav {
    pipelines: Pipelines,
    target: Option<Target>,
    /// The dispatch's third dimension: the slices of an array, the depth of a volume, else 1.
    dispatch_depth: u32,
    /// The colour as the shader reads it: four words, floats as their bits.
    color: [u32; 4],
    offset: [u32; 2],
    size: [u32; 2],
}

impl ClearUav {
    /// A clear that makes its pipelines through `ctx`.
    ///
    /// One `ctx` cannot make leaves a clear that sets no pipeline.
    pub fn new<C: SimpleCommandContext>(ctx: &mut C) -> Self {
        let mut pipeline = |name| ctx.compute_pipeline(name);
        Self {
            pipelines: Pipelines {
                buffer_uint: pipeline(c"cs_clear_buffer_uint"),
                buffer_float: pipeline(c"cs_clear_buffer_float"),
                tbuffer_uint: pipeline(c"cs_clear_tbuffer_uint"),
                tbuffer_float: pipeline(c"cs_clear_tbuffer_float"),
                texture2d_uint: pipeline(c"cs_clear_texture2d_uint"),
                texture2d_float: pipeline(c"cs_clear_texture2d_float"),
                texture2d_array_uint: pipeline(c"cs_clear_texture2d_array_uint"),
                texture2d_array_float: pipeline(c"cs_clear_texture2d_array_float"),
                texture3d_uint: pipeline(c"cs_clear_texture3d_uint"),
                texture3d_float: pipeline(c"cs_clear_texture3d_float"),
            },
            target: None,
            dispatch_depth: 1,
            color: [0; 4],
            offset: [0; 2],
            size: [0; 2],
        }
    }

    /// Begins clearing `view` of `texture` to a float colour.
    pub fn begin_texture_f32<C: SimpleCommandContext>(
        &mut self,
        ctx: &mut C,
        color: [f32; 4],
        texture: &Arc<Texture>,
        view: TextureViewKey,
    ) {
        self.target = Some(Target::Texture {
            texture: Arc::clone(texture),
            view,
        });
        ctx.start_compute_pass();
        self.color = color.map(f32::to_bits);
        self.set_texture_pipeline(ctx, texture, view, false);
    }

    /// Begins clearing `view` of `texture` to an integer colour.
    ///
    /// The clear goes through the unsigned-integer view of the format.
    pub fn begin_texture_u32<C: SimpleCommandContext>(
        &mut self,
        ctx: &mut C,
        color: [u32; 4],
        texture: &Arc<Texture>,
        view: TextureViewKey,
    ) {
        self.target = Some(Target::Texture {
            texture: Arc::clone(texture),
            view,
        });
        ctx.start_compute_pass();
        let view_format = view.format();
        // These two have no unsigned form of their own: they are read as R32Uint.
        let uint_format = if view_format == WMT_PIXEL_FORMAT_RG11B10_FLOAT
            || view_format == WMT_PIXEL_FORMAT_RGB9E5_FLOAT
        {
            WMT_PIXEL_FORMAT_R32_UINT
        } else {
            format::unsigned_integer(view_format)
        };
        if view_format != uint_format {
            self.target = Some(Target::Texture {
                texture: Arc::clone(texture),
                view: view.with_format(uint_format),
            });
        }
        set_uint_color(&mut self.color, color, view_format);
        self.set_texture_pipeline(ctx, texture, view, true);
    }

    /// Begins clearing the texel-buffer `view` of `buffer` to a float colour.
    pub fn begin_texel_buffer_f32<C: SimpleCommandContext>(
        &mut self,
        ctx: &mut C,
        color: [f32; 4],
        buffer: &Arc<Buffer>,
        view: BufferViewKey,
    ) {
        self.target = Some(Target::Buffer {
            buffer: Arc::clone(buffer),
            view: Some(view),
        });
        ctx.start_compute_pass();
        self.color = color.map(f32::to_bits);
        set_pipeline(ctx, self.pipelines.tbuffer_float.as_ref());
    }

    /// Begins clearing the texel-buffer `view` of `buffer` to an integer colour.
    ///
    /// The clear goes through the unsigned-integer view of the format.
    pub fn begin_texel_buffer_u32<C: SimpleCommandContext>(
        &mut self,
        ctx: &mut C,
        color: [u32; 4],
        buffer: &Arc<Buffer>,
        view: BufferViewKey,
    ) {
        self.target = Some(Target::Buffer {
            buffer: Arc::clone(buffer),
            view: Some(view),
        });
        ctx.start_compute_pass();
        let view_format = view.format();
        let uint_format = if view_format == WMT_PIXEL_FORMAT_RG11B10_FLOAT {
            WMT_PIXEL_FORMAT_R32_UINT
        } else {
            format::unsigned_integer(view_format)
        };
        if view_format != uint_format {
            self.target = Some(Target::Buffer {
                buffer: Arc::clone(buffer),
                view: Some(
                    BufferViewDescriptor {
                        format: uint_format,
                        texel_size: format::texel_size(uint_format),
                    }
                    .key(),
                ),
            });
        }
        set_uint_color(&mut self.color, color, view_format);
        set_pipeline(ctx, self.pipelines.tbuffer_uint.as_ref());
    }

    /// Begins clearing `buffer` to a float colour, four floats to a texel.
    pub fn begin_buffer_f32<C: SimpleCommandContext>(
        &mut self,
        ctx: &mut C,
        color: [f32; 4],
        buffer: &Arc<Buffer>,
    ) {
        self.target = Some(Target::Buffer {
            buffer: Arc::clone(buffer),
            view: None,
        });
        ctx.start_compute_pass();
        self.color = color.map(f32::to_bits);
        set_pipeline(ctx, self.pipelines.buffer_float.as_ref());
    }

    /// Begins clearing `buffer` to an integer colour, four words to a texel.
    pub fn begin_buffer_u32<C: SimpleCommandContext>(
        &mut self,
        ctx: &mut C,
        color: [u32; 4],
        buffer: &Arc<Buffer>,
    ) {
        self.target = Some(Target::Buffer {
            buffer: Arc::clone(buffer),
            view: None,
        });
        ctx.start_compute_pass();
        set_uint_color(&mut self.color, color, 0);
        set_pipeline(ctx, self.pipelines.buffer_uint.as_ref());
    }

    /// Clears a rectangle of the target.
    ///
    /// It is `width` by `height` texels from (`offset_x`, `offset_y`), through every slice of an
    /// array or plane of a volume.
    ///
    /// For a buffer the rectangle is a row: `offset_x` and `width` count texels or words.
    pub fn clear<C: SimpleCommandContext>(
        &mut self,
        ctx: &mut C,
        offset_x: u32,
        offset_y: u32,
        width: u32,
        height: u32,
    ) {
        self.offset = [offset_x, offset_y];
        self.size = [width, height];
        match &self.target {
            Some(Target::Texture { texture, view }) => {
                ctx.set_compute_texture(0, texture, *view, ResourceAccess::WRITE);
            }
            Some(Target::Buffer {
                buffer,
                view: Some(view),
            }) => {
                let Some(allocation) = buffer.current() else {
                    return;
                };
                let texels = buffer.view(&allocation, *view).suballocation_texel;
                let sub_offset = allocation.current_suballocation_offset_in(texels);
                ctx.set_compute_texel_buffer(0, buffer, *view, ResourceAccess::WRITE);
                // The C++ asserts the offset is 0 and adds it to a 32-bit field all the same.
                debug_assert!(sub_offset == 0, "a texel buffer's suballocation offset");
                self.offset[0] = self.offset[0].wrapping_add(low_word(sub_offset));
            }
            Some(Target::Buffer { buffer, view: None }) => {
                let Some(allocation) = buffer.current() else {
                    return;
                };
                let sub_offset = allocation.current_suballocation_offset();
                ctx.set_compute_buffer(0, buffer, offset_x, width, ResourceAccess::WRITE);
                self.offset[0] = self.offset[0].wrapping_add(low_word(sub_offset));
            }
            None => return,
        }
        ctx.set_compute_bytes(1, self.metadata().as_flattened());
        ctx.dispatch(WMTSize {
            width: u64::from(width),
            height: u64::from(height),
            depth: u64::from(self.dispatch_depth),
        });
    }

    /// Ends the pass `begin_*` started; nothing if none did.
    pub fn end<C: SimpleCommandContext>(&mut self, ctx: &mut C) {
        if self.target.take().is_none() {
            return;
        }
        ctx.end_pass();
        self.dispatch_depth = 1;
    }

    /// The shader's constants: the colour, then the rectangle's offset and size.
    fn metadata(&self) -> [[u8; 4]; 8] {
        [
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
            self.offset[0],
            self.offset[1],
            self.size[0],
            self.size[1],
        ]
        .map(u32::to_le_bytes)
    }

    /// Sets the pipeline for `view` of `texture` and the dispatch's depth.
    ///
    /// Cube and multisampled views are no clear targets: no pipeline is set, as the C++ leaves it.
    fn set_texture_pipeline<C: SimpleCommandContext>(
        &mut self,
        ctx: &mut C,
        texture: &Texture,
        view: TextureViewKey,
        uint: bool,
    ) {
        let p = &self.pipelines;
        let pso = match shape(
            view.texture_type(),
            view.array_size(),
            texture.view_size(view)[2],
        ) {
            Shape::Invalid => return,
            Shape::Plane if uint => &p.texture2d_uint,
            Shape::Plane => &p.texture2d_float,
            Shape::Array(slices) => {
                self.dispatch_depth = slices;
                if uint {
                    &p.texture2d_array_uint
                } else {
                    &p.texture2d_array_float
                }
            }
            Shape::Volume(depth) => {
                self.dispatch_depth = depth;
                if uint {
                    &p.texture3d_uint
                } else {
                    &p.texture3d_float
                }
            }
        };
        set_pipeline(ctx, pso.as_ref());
    }
}

/// The compute pipelines of the clear shaders; `None` where the context made none.
struct Pipelines {
    buffer_uint: Option<Obj>,
    buffer_float: Option<Obj>,
    tbuffer_uint: Option<Obj>,
    tbuffer_float: Option<Obj>,
    texture2d_uint: Option<Obj>,
    texture2d_float: Option<Obj>,
    texture2d_array_uint: Option<Obj>,
    texture2d_array_float: Option<Obj>,
    texture3d_uint: Option<Obj>,
    texture3d_float: Option<Obj>,
}

/// What is being cleared.
enum Target {
    Texture {
        texture: Arc<Texture>,
        view: TextureViewKey,
    },
    Buffer {
        buffer: Arc<Buffer>,
        /// The texel-buffer view, or `None` for the raw buffer.
        view: Option<BufferViewKey>,
    },
}

/// How a texture view is dispatched over.
#[derive(PartialEq, Eq, Debug)]
enum Shape {
    /// One slice: a plane, 1D or 2D.
    Plane,
    /// This many slices of an array.
    Array(u32),
    /// This many planes of a volume.
    Volume(u32),
    /// A cube or a multisampled texture, which is no clear target.
    Invalid,
}

const fn shape(texture_type: u32, array_size: u32, depth: u32) -> Shape {
    match texture_type {
        WMT_TEXTURE_TYPE1D_ARRAY | WMT_TEXTURE_TYPE2D_ARRAY => Shape::Array(array_size),
        WMT_TEXTURE_TYPE3D => Shape::Volume(depth),
        WMT_TEXTURE_TYPE_CUBE
        | WMT_TEXTURE_TYPE_CUBE_ARRAY
        | WMT_TEXTURE_TYPE2D_MULTISAMPLE
        | WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY => Shape::Invalid,
        _ => Shape::Plane,
    }
}

fn set_pipeline<C: SimpleCommandContext>(ctx: &mut C, pso: Option<&Obj>) {
    if let Some(pso) = pso {
        ctx.set_compute_pso(pso, THREADGROUP);
    }
}

/// The low 32 bits of `value`: what a 32-bit field keeps of a sum with it.
fn low_word(value: u64) -> u32 {
    u32::try_from(value & u64::from(u32::MAX)).unwrap_or(0)
}

/// Sets `words` to the integer `color` as a texel of `format` holds it.
///
/// The fields the format has, in its channel order, cut to their widths.
///
/// A packed format fills the first word alone. Alpha-only fills it alone and leaves the rest as
/// they were, as the C++ does; any other format takes the four words as they are.
const fn set_uint_color(words: &mut [u32; 4], color: [u32; 4], format: u32) {
    let [r, g, b, a] = color;
    match format {
        WMT_PIXEL_FORMAT_A8_UNORM => words[0] = a & 0xff,
        WMT_PIXEL_FORMAT_R8_UNORM
        | WMT_PIXEL_FORMAT_R8_UNORM_S_RGB
        | WMT_PIXEL_FORMAT_R8_SNORM
        | WMT_PIXEL_FORMAT_R8_UINT
        | WMT_PIXEL_FORMAT_R8_SINT
        | WMT_PIXEL_FORMAT_RG8_UNORM_S_RGB
        | WMT_PIXEL_FORMAT_RG8_SNORM
        | WMT_PIXEL_FORMAT_RG8_UNORM
        | WMT_PIXEL_FORMAT_RG8_UINT
        | WMT_PIXEL_FORMAT_RG8_SINT
        | WMT_PIXEL_FORMAT_RGBA8_UNORM
        | WMT_PIXEL_FORMAT_RGBA8_UNORM_S_RGB
        | WMT_PIXEL_FORMAT_RGBA8_SNORM
        | WMT_PIXEL_FORMAT_RGBA8_UINT
        | WMT_PIXEL_FORMAT_RGBA8_SINT => *words = [r & 0xff, g & 0xff, b & 0xff, a & 0xff],
        WMT_PIXEL_FORMAT_BGRA8_UNORM
        | WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB
        | WMT_PIXEL_FORMAT_BGRX8_UNORM
        | WMT_PIXEL_FORMAT_BGRX8_UNORM_S_RGB => *words = [b & 0xff, g & 0xff, r & 0xff, a & 0xff],
        WMT_PIXEL_FORMAT_R16_UNORM
        | WMT_PIXEL_FORMAT_R16_SNORM
        | WMT_PIXEL_FORMAT_R16_UINT
        | WMT_PIXEL_FORMAT_R16_SINT
        | WMT_PIXEL_FORMAT_R16_FLOAT
        | WMT_PIXEL_FORMAT_RG16_UNORM
        | WMT_PIXEL_FORMAT_RG16_SNORM
        | WMT_PIXEL_FORMAT_RG16_UINT
        | WMT_PIXEL_FORMAT_RG16_SINT
        | WMT_PIXEL_FORMAT_RG16_FLOAT
        | WMT_PIXEL_FORMAT_RGBA16_UNORM
        | WMT_PIXEL_FORMAT_RGBA16_SNORM
        | WMT_PIXEL_FORMAT_RGBA16_UINT
        | WMT_PIXEL_FORMAT_RGBA16_SINT
        | WMT_PIXEL_FORMAT_RGBA16_FLOAT => {
            *words = [r & 0xffff, g & 0xffff, b & 0xffff, a & 0xffff];
        }
        WMT_PIXEL_FORMAT_RGB10A2_UNORM | WMT_PIXEL_FORMAT_RGB10A2_UINT => {
            *words = [r & 0x3ff, g & 0x3ff, b & 0x3ff, a & 0x3];
        }
        WMT_PIXEL_FORMAT_BGR10A2_UNORM
        | WMT_PIXEL_FORMAT_BGR10_XR
        | WMT_PIXEL_FORMAT_BGR10_XR_S_RGB => {
            *words = [b & 0x3ff, g & 0x3ff, r & 0x3ff, a & 0x3];
        }
        // Read as R32Uint.
        WMT_PIXEL_FORMAT_RG11B10_FLOAT => {
            *words = [
                (r & 0x7ff) | ((g & 0x7ff) << 11) | ((b & 0x3ff) << 22),
                0,
                0,
                0,
            ];
        }
        WMT_PIXEL_FORMAT_RGB9E5_FLOAT => {
            *words = [
                (r & 0x1ff) | ((g & 0x1ff) << 9) | ((b & 0x1ff) << 18) | ((a & 0x1f) << 27),
                0,
                0,
                0,
            ];
        }
        _ => *words = color,
    }
}

#[cfg(test)]
mod tests;
