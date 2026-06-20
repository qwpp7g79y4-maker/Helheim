//! Wasm Memory Marshalling Math Plugin

use std::mem::ManuallyDrop;
use std::slice;

// =============================================================================
// Wasm Memory Allocator Exports
// =============================================================================

/// Allocates memory in the Wasm guest and returns the pointer.
/// The host (Helheim) calls this to push arguments (strings) into the sandbox.
#[no_mangle]
pub extern "C" fn ffi_alloc(size: u32) -> *mut u8 {
    let mut buf = Vec::with_capacity(size as usize);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf); // Leak it so the host can write to it
    ptr
}

/// Frees memory allocated by `ffi_alloc`.
#[no_mangle]
pub extern "C" fn ffi_free(ptr: *mut u8, size: u32) {
    if !ptr.is_null() && size > 0 {
        unsafe {
            let _ = Vec::from_raw_parts(ptr, size as usize, size as usize);
        }
    }
}

// =============================================================================
// Helper to parse arguments and return results
// =============================================================================

fn parse_args(ptr: *const u8, size: u32) -> Vec<String> {
    if ptr.is_null() || size == 0 {
        return vec![];
    }
    let bytes = unsafe { slice::from_raw_parts(ptr, size as usize) };
    if let Ok(s) = std::str::from_utf8(bytes) {
        s.split('\x1E').map(|s| s.to_string()).collect()
    } else {
        vec![]
    }
}

fn write_out(out_ptr: *mut u8, val: &str) {
    if out_ptr.is_null() { return; }
    let bytes = val.as_bytes();
    let len = (bytes.len() as u32).min(1020); // max 1KB
    unsafe {
        // Write length prefix (4 bytes, little endian)
        let len_bytes = len.to_le_bytes();
        std::ptr::copy_nonoverlapping(len_bytes.as_ptr(), out_ptr, 4);
        // Write string content
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_ptr.add(4), len as usize);
    }
}

// =============================================================================
// Math Functions
// =============================================================================

#[no_mangle]
pub extern "C" fn math_sin(_ctx: i32, args_ptr: i32, args_len: i32, out_ptr: i32) -> i32 {
    let args = parse_args(args_ptr as *const u8, args_len as u32);
    if args.is_empty() { return -2; }
    
    if let Ok(v) = args[0].parse::<f64>() {
        let res = v.sin().to_string();
        write_out(out_ptr as *mut u8, &res);
        0 // HEL_ERR_OK
    } else {
        -2
    }
}

#[no_mangle]
pub extern "C" fn math_add(_ctx: i32, args_ptr: i32, args_len: i32, out_ptr: i32) -> i32 {
    let args = parse_args(args_ptr as *const u8, args_len as u32);
    if args.len() < 2 { return -2; }
    
    if let (Ok(a), Ok(b)) = (args[0].parse::<i64>(), args[1].parse::<i64>()) {
        let res = (a + b).to_string();
        write_out(out_ptr as *mut u8, &res);
        0
    } else {
        -2
    }
}