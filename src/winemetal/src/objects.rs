//! Object model, strings and data: retain and release, arrays, strings, pools, dispatch data.

use core::ffi::{c_char, c_void};

use winemetal_wire::{
    UnixcallDispatchdataCopybytes, UnixcallEnumerate, UnixcallGenericObjObjRet,
    UnixcallGenericObjRet, UnixcallGenericObjUint64ObjRet, UnixcallGenericObjUint64Ret,
    UnixcallGenericObjUint64Uint64Ret, UnixcallNsstringGetcstring, UnixcallNsstringString, slot,
};

use crate::transport::{addr, call, mem_ptr};

exports! {
    fn NSObject_retain(obj: u64) {
        call(slot::NS_OBJECT_RETAIN, |p: &mut u64| *p = obj);
    }

    fn NSObject_release(obj: u64) {
        call(slot::NS_OBJECT_RELEASE, |p: &mut u64| *p = obj);
    }

    fn NSArray_object(array: u64, index: u64) -> u64 {
        call(slot::NS_ARRAY_OBJECT, |p: &mut UnixcallGenericObjUint64ObjRet| {
            p.handle = array;
            p.arg = index;
        })
        .ret
    }

    fn NSArray_count(array: u64) -> u64 {
        call(slot::NS_ARRAY_COUNT, |p: &mut UnixcallGenericObjUint64Ret| p.handle = array).ret
    }

    fn NSString_getCString(str: u64, buffer: *mut c_char, max_length: u64, encoding: u64) -> u32 {
        call(slot::NS_STRING_GET_C_STRING, |p: &mut UnixcallNsstringGetcstring| {
            p.str = str;
            p.buffer_ptr = addr(buffer);
            p.max_length = max_length;
            p.encoding = encoding;
        })
        .ret
    }

    fn NSAutoreleasePool_alloc_init() -> u64 {
        call(slot::NS_AUTORELEASE_POOL_ALLOC_INIT, |_: &mut UnixcallGenericObjRet| ()).ret
    }

    fn NSString_lengthOfBytesUsingEncoding(str: u64, encoding: u64) -> u64 {
        call(slot::NS_STRING_LENGTH_OF_BYTES_USING_ENCODING, |p: &mut UnixcallGenericObjUint64Uint64Ret| {
            p.handle = str;
            p.arg = encoding;
        })
        .ret
    }

    fn NSObject_description(nserror: u64) -> u64 {
        call(slot::NS_OBJECT_DESCRIPTION, |p: &mut UnixcallGenericObjObjRet| p.handle = nserror).ret
    }

    fn NSString_string(data: *const c_char, encoding: u64) -> u64 {
        call(slot::NS_STRING_STRING, |p: &mut UnixcallNsstringString| {
            p.buffer_ptr = mem_ptr(data);
            p.encoding = encoding;
        })
        .ret
    }

    fn NSString_alloc_init(data: *const c_char, encoding: u64) -> u64 {
        call(slot::NS_STRING_ALLOC_INIT, |p: &mut UnixcallNsstringString| {
            p.buffer_ptr = mem_ptr(data);
            p.encoding = encoding;
        })
        .ret
    }

    fn MTLLogContainer_enumerate(logs: u64, start: u64, buffer_size: u64, buffer: *mut u64) -> u64 {
        call(slot::MTL_LOG_CONTAINER_ENUMERATE, |p: &mut UnixcallEnumerate| {
            p.enumerable = logs;
            p.start = start;
            p.buffer_size = buffer_size;
            p.buffer = mem_ptr(buffer);
        })
        .ret_read
    }

    fn DispatchData_alloc_init(native_ptr: u64, length: u64) -> u64 {
        call(slot::DISPATCH_DATA_ALLOC_INIT, |p: &mut UnixcallGenericObjUint64ObjRet| {
            p.handle = native_ptr;
            p.arg = length;
        })
        .ret
    }

    fn DispatchData_copyBytes(data: u64, buffer: *mut c_void, capacity: u64) -> u64 {
        call(slot::DISPATCH_DATA_COPY_BYTES, |p: &mut UnixcallDispatchdataCopybytes| {
            p.data = data;
            p.buffer = mem_ptr(buffer);
            p.capacity = capacity;
        })
        .ret_size
    }
}
