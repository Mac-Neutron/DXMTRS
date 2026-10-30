//! GPU capture, counter sample buffers and timestamps.

use core::ffi::c_void;

use winemetal_wire::{
    UnixcallGenericObjNoret, UnixcallGenericObjRet, UnixcallMtlcapturemanagerStartcapture,
    UnixcallMtlcountersamplebufferNewtimestampbuffer,
    UnixcallMtlcountersamplebufferResolvecounterrange, UnixcallMtldeviceSampletimestamps,
    WMTCaptureInfo, slot,
};

use crate::transport::{call, mem_ptr};

exports! {
    fn MTLCaptureManager_sharedCaptureManager() -> u64 {
        call(slot::MTL_CAPTURE_MANAGER_SHARED_CAPTURE_MANAGER, |_: &mut UnixcallGenericObjRet| ()).ret
    }

    fn MTLCaptureManager_startCapture(mgr: u64, info: *mut WMTCaptureInfo) -> bool {
        call(slot::MTL_CAPTURE_MANAGER_START_CAPTURE, |p: &mut UnixcallMtlcapturemanagerStartcapture| {
            p.capture_manager = mgr;
            p.info = mem_ptr(info);
        })
        .ret
            != 0
    }

    fn MTLCaptureManager_stopCapture(mgr: u64) {
        call(slot::MTL_CAPTURE_MANAGER_STOP_CAPTURE, |p: &mut UnixcallGenericObjNoret| p.handle = mgr);
    }

    fn MTLCounterSampleBuffer_newTimestampBuffer(device: u64, sample_count: u32, shared: bool) -> u64 {
        call(
            slot::MTL_COUNTER_SAMPLE_BUFFER_NEW_TIMESTAMP_BUFFER,
            |p: &mut UnixcallMtlcountersamplebufferNewtimestampbuffer| {
                p.device = device;
                p.sample_count = sample_count;
                p.shared = u32::from(shared);
            },
        )
        .ret
    }

    fn MTLCounterSampleBuffer_resolveCounterRange(
        sample_buffer: u64,
        start: u32,
        len: u32,
        data_out: *mut c_void,
        data_length: u64,
    ) {
        call(
            slot::MTL_COUNTER_SAMPLE_BUFFER_RESOLVE_COUNTER_RANGE,
            |p: &mut UnixcallMtlcountersamplebufferResolvecounterrange| {
                p.sample_buffer = sample_buffer;
                p.start = start;
                p.len = len;
                p.data_length = data_length;
                p.data_out = mem_ptr(data_out);
            },
        );
    }

    fn MTLDevice_sampleTimestamps(device: u64, cpu_timestamp: *mut u64, gpu_timestamp: *mut u64) {
        let p = call(slot::MTL_DEVICE_SAMPLE_TIMESTAMPS, |p: &mut UnixcallMtldeviceSampletimestamps| {
            p.device = device;
        });
        // SAFETY: valid for a write (the function's contract; the C never checked it).
        unsafe { cpu_timestamp.write(p.cpu) };
        // SAFETY: as above.
        unsafe { gpu_timestamp.write(p.gpu) };
    }
}
