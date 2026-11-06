//! D3D9's crash reporter: what its lines say, and the address-space walk they carry.
//!
//! The reporter itself, a vectored exception handler and a panic hook, is
//! `dxmt_util::crash::veh`, which `init_logger` installs from `DllMain`'s
//! process attach with [`REPORTER`]; `DllMain`'s detach removes it. Its
//! functions run on the faulting thread, so none of them allocates or
//! touches a Rust thread-local: the log sink's synchronous thunks, the crumb
//! dump and [`free_mib`]'s walk of the address space.

use core::ffi::c_void;

use dxmt_util::crash::veh::Reporter;
use mtld3d_core::{
    address_space::FreeSpace,
    crash_report::{BASE_PREFIX, FATAL_PREFIX, FOREIGN_PREFIX, PANIC_HEADING, PANIC_PREFIX},
};

/// D3D9's crash reports: its literals, its log sink, its crumb trail and its address space.
pub static REPORTER: Reporter = Reporter {
    panic_prefix: PANIC_PREFIX,
    panic_heading: PANIC_HEADING,
    fatal_prefix: FATAL_PREFIX,
    foreign_prefix: FOREIGN_PREFIX,
    base_prefix: BASE_PREFIX,
    write_crash: crate::log_sink::write_crash,
    write_fault: crate::log_sink::write_fault,
    before_panic: crate::log_sink::forward_parked,
    trail: mtld3d_shared::crumb::dump_recent,
    free_mib,
    image_size: mtld3d_shared::identity::image_size,
};

/// `MEMORY_BASIC_INFORMATION`, one region of the address space.
#[repr(C)]
struct MemoryBasicInformation {
    base_address: *mut c_void,
    allocation_base: *mut c_void,
    allocation_protect: u32,
    region_size: usize,
    state: u32,
    protect: u32,
    kind: u32,
}

const MEM_FREE: u32 = 0x1_0000;

/// Free address space of this process: the sum of the usable free regions and the largest.
///
/// The free figure the address-space watch and the crash lines report. It is
/// not `GlobalMemoryStatusEx`'s `ullAvailVirtual`, which Wine computes as the
/// total minus the process working set (see `mtld3d_core::address_space`).
/// Free space that no single allocation can use is what fails a DLL load or a
/// game's streaming block long before the total runs out, so both come from
/// the same walk. Allocation-free, so the exception handler can call it.
pub fn free_space() -> FreeSpace {
    let mut space = FreeSpace::new();
    walk_regions(|addr, info| {
        space.add_region(info.state == MEM_FREE, addr as u64, info.region_size as u64);
    });
    space
}

/// Visit every region of the address space in address order, with its base address.
///
/// One `VirtualQuery` per region (a few thousand calls), stopping where the
/// query fails or the address would wrap.
fn walk_regions(mut visit: impl FnMut(usize, &MemoryBasicInformation)) {
    let mut addr = 0usize;
    loop {
        let mut info = MemoryBasicInformation {
            base_address: core::ptr::null_mut(),
            allocation_base: core::ptr::null_mut(),
            allocation_protect: 0,
            region_size: 0,
            state: 0,
            protect: 0,
            kind: 0,
        };
        // SAFETY: kernel32 export filling a struct of the size passed.
        let got = unsafe {
            VirtualQuery(
                addr as *const c_void,
                &raw mut info,
                size_of::<MemoryBasicInformation>(),
            )
        };
        if got == 0 || info.region_size == 0 {
            break;
        }
        visit(addr, &info);
        let Some(next) = addr.checked_add(info.region_size) else {
            break;
        };
        addr = next;
    }
}

const MEM_COMMIT: u32 = 0x1000;
const MEM_RESERVE: u32 = 0x2000;
const MEM_IMAGE: u32 = 0x100_0000;
const MEM_MAPPED: u32 = 0x4_0000;

/// A summary of the address space: region counts by state and the largest regions.
///
/// One line of text for the log, built when the free space crosses one of
/// the watch's thresholds, so the log names who owns the space (image,
/// mapped file, private commit, private reserve) and where the biggest holes
/// are.
pub fn address_space_map() -> String {
    let mut regions: Vec<(usize, usize, u32, u32)> = Vec::new();
    walk_regions(|addr, info| regions.push((addr, info.region_size, info.state, info.kind)));
    let mut free = 0usize;
    let mut committed = 0usize;
    let mut reserved = 0usize;
    let mut image = 0usize;
    let mut mapped = 0usize;
    let mut free_holes = 0usize;
    let mut used_regions = 0usize;
    for &(_, size, state, kind) in &regions {
        if state == MEM_FREE {
            free += size;
            free_holes += 1;
            continue;
        }
        used_regions += 1;
        if kind == MEM_IMAGE {
            image += size;
        } else if kind == MEM_MAPPED {
            mapped += size;
        } else if state == MEM_COMMIT {
            committed += size;
        } else if state == MEM_RESERVE {
            reserved += size;
        }
    }
    let mut out = format!(
        "regions used={used_regions} free_holes={free_holes}; MiB: free={} image={} mapped={} \
         private_commit={} private_reserve={}; largest used:",
        free >> 20,
        image >> 20,
        mapped >> 20,
        committed >> 20,
        reserved >> 20
    );
    let mut used: Vec<_> = regions
        .iter()
        .filter(|r| r.2 != MEM_FREE)
        .copied()
        .collect();
    used.sort_by_key(|r| core::cmp::Reverse(r.1));
    for (base, size, state, kind) in used.into_iter().take(12) {
        let what = if kind == MEM_IMAGE {
            "image"
        } else if kind == MEM_MAPPED {
            "mapped"
        } else if state == MEM_COMMIT {
            "commit"
        } else {
            "reserve"
        };
        let _ = core::fmt::Write::write_fmt(
            &mut out,
            format_args!(" {base:#010x}+{}M({what})", size >> 20),
        );
    }
    let mut holes: Vec<_> = regions
        .iter()
        .filter(|r| r.2 == MEM_FREE)
        .copied()
        .collect();
    holes.sort_by_key(|r| core::cmp::Reverse(r.1));
    out.push_str("; largest holes:");
    for (base, size, _, _) in holes.into_iter().take(6) {
        let _ =
            core::fmt::Write::write_fmt(&mut out, format_args!(" {base:#010x}+{}M", size >> 20));
    }
    out
}

unsafe extern "system" {
    fn VirtualQuery(
        address: *const c_void,
        buffer: *mut MemoryBasicInformation,
        length: usize,
    ) -> usize;
}

/// The free address space's total and largest region in MiB, as the crash lines carry them.
fn free_mib() -> (u64, u64) {
    let space = free_space();
    (space.total_mib(), space.largest_mib())
}
