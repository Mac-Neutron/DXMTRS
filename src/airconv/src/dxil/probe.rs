//! `DXMT_PROBE`: three of a pixel shader's own values written as its first render target.
//!
//! Ported from `A/dxil/dxil_converter.cpp:130-226` (`ApplyProbe`). `DXMT_PROBE=<capture
//! hash>:<a>,<b>,<c>` makes the pixel shader capture mode saves as `ps-<hash>.dxil` output three
//! of its own DXIL values, numbered as `dxc -dumpbin` prints them (`%a`, `%b`, `%c`), as
//! `SV_Target0`'s red, green and blue, alpha 1: a frame dump then shows a shader's intermediate
//! values. A value defined on a path the pixel did not take reads -12345; NaN reads -7777 and
//! infinities +-8888. Scalars only: probe a sample's or a cbuffer load's `extractvalue`. A channel
//! written `-` keeps the shader's own value; a fourth field `a` keeps its own alpha too.

use std::{
    env,
    io::{self, Write as _},
    os::unix::ffi::OsStrExt as _,
    sync::LazyLock,
};

use super::{
    initialize::DxilShader,
    lower::{lift, walk_blocks},
    metadata::{arg, const_u32, is_call, op_code},
    types::{op, semantic_kind, shader_kind},
};
use crate::{
    api::Error,
    dxbc::converter::invalid,
    llvm::{Builder, Context, TyKind, Val, sys},
};

/// Applies the probe to `main`, the DXIL entry function, when its spec names this shader.
///
/// # Errors
///
/// A value of a type the translator never makes (none in a DXIL module).
pub fn apply(dxil: &DxilShader, cx: &Context, main: sys::ValueRef) -> Result<(), Error> {
    /// The process's environment, latched once and immutable after.
    ///
    /// The C++'s function static: one spec for every compile of the process.
    static SPEC: LazyLock<Vec<u8>> = LazyLock::new(|| {
        env::var_os("DXMT_PROBE").map_or_else(Vec::new, |v| v.as_bytes().to_vec())
    });
    if dxil.entry.kind != shader_kind::PIXEL {
        return Ok(());
    }
    let Some((hash, numbers, keep_alpha)) = parse(&SPEC) else {
        return Ok(());
    };
    if hash != dxil.capture_hash {
        return Ok(());
    }
    let tcx = cx.tcx();
    let f32_ty = tcx.float();
    let lift = |raw| lift(cx, raw);
    // The instructions numbered as the printer numbers them before anything is added (the
    // C++'s slot tracker, made once), and their text as it printed them then. A number of
    // `u32::MAX` matches the last unnumbered instruction, as the C++'s `(int)~0u` did.
    let mut probed = [None; 3];
    let mut texts: [Vec<u8>; 3] = Default::default();
    for ((slot, text), &number) in probed.iter_mut().zip(&mut texts).zip(&numbers) {
        // SAFETY: a defined function of the module.
        let inst = unsafe { sys::airconv_shim_local_slot(main, number) };
        if !inst.is_null() {
            let inst = lift(inst)?;
            *slot = Some(inst);
            *text = inst.print();
        }
    }

    // SAFETY: a defined function of the module.
    let entry = unsafe { sys::get_entry_basic_block(main) };
    let at_entry = Builder::new(cx);
    // SAFETY: the entry block of a function holds an instruction (its terminator at least).
    at_entry.position_before(lift(unsafe { sys::get_first_instruction(entry) })?);
    let mut slots = Vec::with_capacity(3);
    for _ in 0..3 {
        let slot = at_entry.alloca(f32_ty, c"");
        at_entry.store(cx.const_real(f32_ty, -12345.0), slot);
        slots.push(slot);
    }
    for (k, ((inst, printed), &slot)) in probed.iter().zip(texts).zip(&slots).enumerate() {
        let mut text = b"missing".to_vec();
        if let Some(inst) = *inst {
            text = printed;
            let at = Builder::new(cx);
            position_after(&at, inst)?;
            let mut value = inst;
            match tcx.kind(value.ty()) {
                TyKind::Int(_) => value = at.uitofp(value, f32_ty),
                TyKind::Half | TyKind::Double => value = at.fp_cast(value, f32_ty),
                _ => {}
            }
            if tcx.kind(value.ty()) == TyKind::Float {
                // NaN and infinities as markers: blending would turn them into 0 on the way to
                // the dump.
                let inf = cx.const_real(f32_ty, f64::INFINITY);
                let nan = at.fcmp(sys::REAL_UNO, value, value);
                let mut marked = at.select(nan, cx.const_real(f32_ty, -7777.0), value);
                let positive = at.fcmp(sys::REAL_OEQ, value, inf);
                marked = at.select(positive, cx.const_real(f32_ty, 8888.0), marked);
                let negative = at.fcmp(sys::REAL_OEQ, value, cx.const_real(f32_ty, -f64::INFINITY));
                marked = at.select(negative, cx.const_real(f32_ty, -8888.0), marked);
                at.store(marked, slot);
            } else {
                text = [&b"not a scalar: "[..], &text].concat();
            }
        }
        if let Some(&number) = numbers.get(k).filter(|&&n| n != u32::MAX) {
            text.truncate(160);
            if let Some(nul) = text.iter().position(|&c| c == 0) {
                text.truncate(nul);
            }
            let line = [
                format!("DXMT_PROBE ps-{hash:016x} channel {k}: %{number} = ").as_bytes(),
                &text,
                b"\n",
            ]
            .concat();
            let _ = io::stderr().lock().write_all(&line);
        }
    }

    let target = dxil
        .entry
        .outputs
        .iter()
        .rev()
        .find(|e| {
            e.kind == semantic_kind::TARGET && e.semantic_indices.first().is_none_or(|&i| i == 0)
        })
        .map_or(u32::MAX, |e| e.id);
    // SAFETY: a defined function of the module.
    for bb in walk_blocks(unsafe { sys::get_first_basic_block(main) }) {
        // SAFETY: a block of the function.
        let mut inst = unsafe { sys::get_first_instruction(bb) };
        while !inst.is_null() {
            let call = inst;
            // SAFETY: an instruction of the block.
            inst = unsafe { sys::get_next_instruction(call) };
            let value = arg(call, 4);
            if !is_call(call)
                || op_code(call) != op::STORE_OUTPUT
                || const_u32(arg(call, 1), u32::MAX) != target
                || value.is_null()
                || lift(value)?.ty() != f32_ty
            {
                continue;
            }
            let col = const_u32(arg(call, 3), 0);
            let at = Builder::new(cx);
            at.position_before(lift(call)?);
            let kept = numbers.get(col as usize).copied().unwrap_or(u32::MAX);
            if col < 3 && kept != u32::MAX {
                let slot = slots
                    .get(col as usize)
                    .copied()
                    .ok_or_else(|| invalid("a probe slot"))?;
                // SAFETY: a store-output call, whose argument 4 is its value; a float load.
                unsafe { sys::set_operand(call, 4, at.load(f32_ty, slot).raw()) };
            } else if col == 3 && !keep_alpha {
                // SAFETY: as above; a float constant.
                unsafe { sys::set_operand(call, 4, cx.const_real(f32_ty, 1.0).raw()) };
            }
        }
    }
    Ok(())
}

/// The spec `<hash>:<a>,<b>,<c>[,a]`: the hash, three numbers (`u32::MAX` for `-`) and the alpha.
///
/// The hash reads as `sscanf("%llx")` does, each number as `strtoul(…, 10)` cast to 32 bits.
/// `None` without a colon or a hash.
fn parse(spec: &[u8]) -> Option<(u64, [u32; 3], bool)> {
    let colon = spec.iter().position(|&c| c == b':')?;
    let hash = scan_hex(spec)?;
    let mut numbers = [u32::MAX; 3];
    let mut keep_alpha = false;
    let mut fields = spec
        .get(colon + 1..)
        .unwrap_or_default()
        .split(|&c| c == b',');
    for k in 0..4 {
        let Some(field) = fields.next() else {
            break;
        };
        match numbers.get_mut(k) {
            None => keep_alpha = field == b"a",
            Some(number) if field != b"-" => *number = strtoul(field),
            Some(_) => {}
        }
    }
    Some((hash, numbers, keep_alpha))
}

/// `sscanf(s, "%llx")`: spaces, a sign, `0x`, hex digits (at least one); wraps past 64 bits.
fn scan_hex(s: &[u8]) -> Option<u64> {
    let mut rest = s.trim_ascii_start();
    let negative = rest.first() == Some(&b'-');
    if matches!(rest.first(), Some(b'+' | b'-')) {
        rest = rest.get(1..).unwrap_or_default();
    }
    if let [b'0', b'x' | b'X', next, ..] = rest
        && next.is_ascii_hexdigit()
    {
        rest = rest.get(2..).unwrap_or_default();
    }
    let digits: Vec<u32> = rest
        .iter()
        .map_while(|&c| char::from(c).to_digit(16))
        .collect();
    if digits.is_empty() {
        return None;
    }
    let value = digits
        .iter()
        .fold(0_u64, |v, &d| v.wrapping_mul(16).wrapping_add(d.into()));
    Some(if negative {
        value.wrapping_neg()
    } else {
        value
    })
}

/// `(unsigned)strtoul(s, nullptr, 10)`: spaces, a sign, decimal digits, saturated, low 32 bits.
fn strtoul(s: &[u8]) -> u32 {
    let mut rest = s.trim_ascii_start();
    let negative = rest.first() == Some(&b'-');
    if matches!(rest.first(), Some(b'+' | b'-')) {
        rest = rest.get(1..).unwrap_or_default();
    }
    let mut value = 0_u64;
    for &c in rest.iter().take_while(|c| c.is_ascii_digit()) {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add(u64::from(c - b'0')))
            .unwrap_or(u64::MAX);
    }
    let value = if negative && value != u64::MAX {
        value.wrapping_neg()
    } else {
        value
    };
    let [b0, b1, b2, b3, ..] = value.to_le_bytes();
    u32::from_le_bytes([b0, b1, b2, b3])
}

/// Positions `at` after `inst`.
///
/// Before the next instruction, or at its block's end; after a phi, at the block's first
/// insertion point (past its phis).
fn position_after<'cx>(at: &Builder<'cx>, inst: Val<'cx>) -> Result<(), Error> {
    // SAFETY: an instruction of the module.
    let block = unsafe { sys::get_instruction_parent(inst.raw()) };
    // SAFETY: as above.
    let phi = !unsafe { sys::is_aphi_node(inst.raw()) }.is_null();
    let mut next = if phi {
        // SAFETY: the instruction's block.
        unsafe { sys::get_first_instruction(block) }
    } else {
        // SAFETY: an instruction of the module.
        unsafe { sys::get_next_instruction(inst.raw()) }
    };
    // SAFETY: instructions of the block, walked forward.
    while phi && !next.is_null() && !unsafe { sys::is_aphi_node(next) }.is_null() {
        // SAFETY: as above.
        next = unsafe { sys::get_next_instruction(next) };
    }
    if next.is_null() {
        // SAFETY: a live block.
        at.position_at_end(unsafe { crate::llvm::Block::new(block) });
        return Ok(());
    }
    at.position_before(lift(at.cx(), next)?);
    Ok(())
}
