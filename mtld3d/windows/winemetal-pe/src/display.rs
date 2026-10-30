//! Layers, views, displays, colour spaces and the developer HUD.

use winemetal_wire::{
    UnixcallCreateMetalViewFromHwnd, UnixcallGenericObjConstptrNoret, UnixcallGenericObjNoret,
    UnixcallGenericObjObjNoret, UnixcallGenericObjObjObjNoret, UnixcallGenericObjObjObjUint64Ret,
    UnixcallGenericObjObjRet, UnixcallGenericObjPtrNoret, UnixcallGenericObjRet, UnixcallGenericObjUint64Ret,
    UnixcallGenericObjUint64Uint64Ret, UnixcallQueryDisplaySetting, UnixcallQueryDisplaySettingForLayer,
    UnixcallUpdateDisplaySetting, WMTDisplayDescription, WMTEDRValue, WMTHDRMetadata, WMTLayerProps, slot,
};

use crate::transport::{call, low_half, mem_ptr};

exports! {
    fn DeveloperHUDProperties_instance() -> u64 {
        call(slot::DEVELOPER_HUD_PROPERTIES_INSTANCE, |_: &mut UnixcallGenericObjRet| ()).ret
    }

    fn DeveloperHUDProperties_addLabel(obj: u64, label: u64, after: u64) -> bool {
        call(slot::DEVELOPER_HUD_PROPERTIES_ADD_LABEL, |p: &mut UnixcallGenericObjObjObjUint64Ret| {
            p.handle = obj;
            p.arg0 = label;
            p.arg1 = after;
        })
        .ret
            != 0
    }

    fn DeveloperHUDProperties_updateLabel(obj: u64, label: u64, value: u64) {
        call(slot::DEVELOPER_HUD_PROPERTIES_UPDATE_LABEL, |p: &mut UnixcallGenericObjObjObjNoret| {
            p.handle = obj;
            p.arg0 = label;
            p.arg1 = value;
        });
    }

    fn DeveloperHUDProperties_remove(obj: u64, label: u64) {
        call(slot::DEVELOPER_HUD_PROPERTIES_REMOVE, |p: &mut UnixcallGenericObjObjNoret| {
            p.handle = obj;
            p.arg = label;
        });
    }

    fn MetalDrawable_texture(drawable: u64) -> u64 {
        call(slot::METAL_DRAWABLE_TEXTURE, |p: &mut UnixcallGenericObjObjRet| p.handle = drawable).ret
    }

    fn MetalLayer_nextDrawable(layer: u64) -> u64 {
        call(slot::METAL_LAYER_NEXT_DRAWABLE, |p: &mut UnixcallGenericObjObjRet| p.handle = layer).ret
    }

    fn MetalLayer_setProps(layer: u64, props: *const WMTLayerProps) {
        call(slot::METAL_LAYER_SET_PROPS, |p: &mut UnixcallGenericObjConstptrNoret| {
            p.handle = layer;
            p.arg = mem_ptr(props);
        });
    }

    fn MetalLayer_getProps(layer: u64, props: *mut WMTLayerProps) {
        call(slot::METAL_LAYER_GET_PROPS, |p: &mut UnixcallGenericObjPtrNoret| {
            p.handle = layer;
            p.arg = mem_ptr(props);
        });
    }

    fn CreateMetalViewFromHWND(hwnd: isize, device: u64, layer: *mut u64) -> u64 {
        let p = call(slot::CREATE_METAL_VIEW_FROM_HWND, |p: &mut UnixcallCreateMetalViewFromHwnd| {
            // `(uint64_t)(uintptr_t)hwnd`: the address bits, zero-extended.
            p.hwnd = hwnd.cast_unsigned() as u64;
            p.device = device;
        });
        // SAFETY: valid for a write (the function's contract; the C never checked it).
        unsafe { layer.write(p.ret_layer) };
        p.ret_view
    }

    fn ReleaseMetalView(view: u64) {
        call(slot::RELEASE_METAL_VIEW, |p: &mut UnixcallGenericObjNoret| p.handle = view);
    }

    fn CGColorSpace_checkColorSpaceSupported(colorspace: u64) -> bool {
        call(slot::CG_COLOR_SPACE_CHECK_COLOR_SPACE_SUPPORTED, |p: &mut UnixcallGenericObjUint64Ret| {
            p.handle = colorspace;
        })
        .ret
            != 0
    }

    fn MetalLayer_setColorSpace(layer: u64, colorspace: u64) -> bool {
        call(slot::METAL_LAYER_SET_COLOR_SPACE, |p: &mut UnixcallGenericObjUint64Uint64Ret| {
            p.handle = layer;
            p.arg = colorspace;
        })
        .ret
            != 0
    }

    fn WMTGetPrimaryDisplayId() -> u32 {
        low_half(call(slot::WMT_GET_PRIMARY_DISPLAY_ID, |_: &mut UnixcallGenericObjRet| ()).ret)
    }

    fn WMTGetSecondaryDisplayId() -> u32 {
        low_half(call(slot::WMT_GET_SECONDARY_DISPLAY_ID, |_: &mut UnixcallGenericObjRet| ()).ret)
    }

    fn WMTGetDisplayDescription(display_id: u32, desc: *mut WMTDisplayDescription) {
        call(slot::WMT_GET_DISPLAY_DESCRIPTION, |p: &mut UnixcallGenericObjPtrNoret| {
            p.handle = u64::from(display_id);
            p.arg = mem_ptr(desc);
        });
    }

    fn MetalLayer_getEDRValue(layer: u64, value: *mut WMTEDRValue) {
        call(slot::METAL_LAYER_GET_EDR_VALUE, |p: &mut UnixcallGenericObjConstptrNoret| {
            p.handle = layer;
            p.arg = mem_ptr(value);
        });
    }

    fn WMTQueryDisplaySetting(display_id: u32, colorspace: *mut u64, metadata: *mut WMTHDRMetadata) -> bool {
        let p = call(slot::WMT_QUERY_DISPLAY_SETTING, |p: &mut UnixcallQueryDisplaySetting| {
            p.display_id = u64::from(display_id);
            p.hdr_metadata = mem_ptr(metadata);
        });
        // SAFETY: valid for a write (the function's contract; the C never checked it).
        unsafe { colorspace.write(p.colorspace) };
        p.ret != 0
    }

    fn WMTUpdateDisplaySetting(display_id: u32, colorspace: u64, metadata: *const WMTHDRMetadata) {
        call(slot::WMT_UPDATE_DISPLAY_SETTING, |p: &mut UnixcallUpdateDisplaySetting| {
            p.display_id = u64::from(display_id);
            p.colorspace = colorspace;
            p.hdr_metadata = mem_ptr(metadata);
        });
    }

    fn WMTQueryDisplaySettingForLayer(
        layer: u64,
        version: *mut u64,
        colorspace: *mut u64,
        metadata: *mut WMTHDRMetadata,
        edr_value: *mut WMTEDRValue,
    ) {
        let p = call(slot::WMT_QUERY_DISPLAY_SETTING_FOR_LAYER, |p: &mut UnixcallQueryDisplaySettingForLayer| {
            p.layer = layer;
            p.hdr_metadata = mem_ptr(metadata);
        });
        // SAFETY: valid for a write (the function's contract; the C never checked it).
        unsafe { version.write(p.version) };
        // SAFETY: as above.
        unsafe { colorspace.write(p.colorspace) };
        // SAFETY: as above.
        unsafe { edr_value.write(p.edr_value) };
    }
}
