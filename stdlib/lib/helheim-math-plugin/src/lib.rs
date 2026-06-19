//! Example Helheim Math Plugin (C-ABI native module)
//!
//! Build with:
//!   cargo build -p helheim-math-plugin --release
//!
//! The resulting library (libhelheim_math_plugin.so / .dylib / .dll)
//! can be renamed to libmath.so (or placed where the NativeModuleLoader
//! will find it as "math") and loaded via `gebruik "math"` in a .hel script.
//!
//! This demonstrates a minimal but correct implementation of the Helheim FFI.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::ptr;

// =============================================================================
// Local copy of the minimal C-ABI (for a standalone example crate)
// In a real module you would #include a provided helheim_ffi.h
// =============================================================================

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelValueTag {
    Null = 0,
    Int = 1,
    Float = 2,
    String = 3,
    List = 4,
    ResourceHandle = 5,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct HelString {
    pub ptr: *const u8,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct HelList {
    pub ptr: *const HelValue,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct HelResourceHandle {
    pub kind: *const c_char,
    pub id: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct HelValue {
    pub tag: HelValueTag,
    pub data: HelValueData,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union HelValueData {
    pub i: i64,
    pub f: f64,
    pub str: HelString,
    pub list: HelList,
    pub res: HelResourceHandle,
    pub _pad: [u8; 16],
}

#[repr(C)]
pub struct HelFFIContext {
    pub abi_version: u32,
    pub alloc: Option<extern "C" fn(usize, usize) -> *mut c_void>,
    pub free: Option<extern "C" fn(*mut c_void)>,
    pub report_error: Option<extern "C" fn(*mut HelFFIContext, c_int, *const c_char)>,
    pub log: Option<extern "C" fn(*const c_char)>,
    pub user_data: *mut c_void,
    pub last_error_code: c_int,
    pub last_error_message: *const c_char,
    pub owned_last_error_message: bool,
}

pub type HelFunctionCall = extern "C" fn(
    ctx: *mut HelFFIContext,
    args: *const HelValue,
    arity: u32,
    out: *mut HelValue,
) -> c_int;

#[repr(C)]
pub struct HelFunctionDesc {
    pub name: *const c_char,
    pub arity: u32,
    pub call: HelFunctionCall,
}

#[repr(C)]
pub struct HelFunctionTable {
    pub count: u32,
    pub entries: *const HelFunctionDesc,
}

// =============================================================================
// Helper macros / utils for the plugin
// =============================================================================

const HEL_ABI_VERSION: u32 = 1;
const HEL_ERR_OK: c_int = 0;
const HEL_ERR_INVALID_ARG: c_int = -2;

unsafe fn report_error(ctx: *mut HelFFIContext, code: c_int, msg: &str) {
    if ctx.is_null() {
        return;
    }
    let cmsg = CString::new(msg).unwrap();
    if let Some(reporter) = (*ctx).report_error {
        reporter(ctx, code, cmsg.as_ptr());
    }
    (*ctx).last_error_code = code;
    (*ctx).last_error_message = cmsg.as_ptr();
    // Note: we leak the CString here on purpose for the error lifetime.
    // Real production code should use ctx->alloc for the message buffer.
    std::mem::forget(cmsg);
}

fn hel_value_int(v: i64) -> HelValue {
    HelValue {
        tag: HelValueTag::Int,
        data: HelValueData { i: v },
    }
}

fn hel_value_float(v: f64) -> HelValue {
    HelValue {
        tag: HelValueTag::Float,
        data: HelValueData { f: v },
    }
}

unsafe fn get_arg_float(args: *const HelValue, idx: usize, arity: u32) -> Result<f64, c_int> {
    if (idx as u32) >= arity || args.is_null() {
        return Err(HEL_ERR_INVALID_ARG);
    }
    let val = *args.add(idx);
    if val.tag != HelValueTag::Float {
        return Err(HEL_ERR_INVALID_ARG);
    }
    Ok(val.data.f)
}

unsafe fn get_arg_int(args: *const HelValue, idx: usize, arity: u32) -> Result<i64, c_int> {
    if (idx as u32) >= arity || args.is_null() {
        return Err(HEL_ERR_INVALID_ARG);
    }
    let val = *args.add(idx);
    if val.tag != HelValueTag::Int {
        return Err(HEL_ERR_INVALID_ARG);
    }
    Ok(val.data.i)
}

// =============================================================================
// Actual math functions
// =============================================================================

extern "C" fn math_pi(
    _ctx: *mut HelFFIContext,
    _args: *const HelValue,
    _arity: u32,
    out: *mut HelValue,
) -> c_int {
    if out.is_null() {
        return HEL_ERR_INVALID_ARG;
    }
    unsafe {
        *out = hel_value_float(std::f64::consts::PI);
    }
    HEL_ERR_OK
}

extern "C" fn math_sin(
    ctx: *mut HelFFIContext,
    args: *const HelValue,
    arity: u32,
    out: *mut HelValue,
) -> c_int {
    match unsafe { get_arg_float(args, 0, arity) } {
        Ok(x) => {
            if !out.is_null() {
                unsafe { *out = hel_value_float(x.sin()) };
            }
            HEL_ERR_OK
        }
        Err(code) => {
            unsafe { report_error(ctx, code, "math::sin expects one float argument") };
            code
        }
    }
}

extern "C" fn math_cos(
    ctx: *mut HelFFIContext,
    args: *const HelValue,
    arity: u32,
    out: *mut HelValue,
) -> c_int {
    match unsafe { get_arg_float(args, 0, arity) } {
        Ok(x) => {
            if !out.is_null() {
                unsafe { *out = hel_value_float(x.cos()) };
            }
            HEL_ERR_OK
        }
        Err(code) => {
            unsafe { report_error(ctx, code, "math::cos expects one float argument") };
            code
        }
    }
}

extern "C" fn math_sqrt(
    ctx: *mut HelFFIContext,
    args: *const HelValue,
    arity: u32,
    out: *mut HelValue,
) -> c_int {
    match unsafe { get_arg_float(args, 0, arity) } {
        Ok(x) => {
            if x < 0.0 {
                unsafe { report_error(ctx, HEL_ERR_INVALID_ARG, "math::sqrt domain error (negative)") };
                return HEL_ERR_INVALID_ARG;
            }
            if !out.is_null() {
                unsafe { *out = hel_value_float(x.sqrt()) };
            }
            HEL_ERR_OK
        }
        Err(code) => {
            unsafe { report_error(ctx, code, "math::sqrt expects one non-negative float") };
            code
        }
    }
}

extern "C" fn math_add(
    ctx: *mut HelFFIContext,
    args: *const HelValue,
    arity: u32,
    out: *mut HelValue,
) -> c_int {
    let a = match unsafe { get_arg_int(args, 0, arity) } {
        Ok(v) => v,
        Err(code) => {
            unsafe { report_error(ctx, code, "math::add expects two int arguments") };
            return code;
        }
    };
    let b = match unsafe { get_arg_int(args, 1, arity) } {
        Ok(v) => v,
        Err(code) => {
            unsafe { report_error(ctx, code, "math::add expects two int arguments") };
            return code;
        }
    };

    if !out.is_null() {
        unsafe { *out = hel_value_int(a + b) };
    }
    HEL_ERR_OK
}

// =============================================================================
// Function table (exported)
// =============================================================================

static MATH_FUNCTIONS: [HelFunctionDesc; 5] = [
    HelFunctionDesc {
        name: b"pi\0".as_ptr() as *const c_char,
        arity: 0,
        call: math_pi,
    },
    HelFunctionDesc {
        name: b"sin\0".as_ptr() as *const c_char,
        arity: 1,
        call: math_sin,
    },
    HelFunctionDesc {
        name: b"cos\0".as_ptr() as *const c_char,
        arity: 1,
        call: math_cos,
    },
    HelFunctionDesc {
        name: b"sqrt\0".as_ptr() as *const c_char,
        arity: 1,
        call: math_sqrt,
    },
    HelFunctionDesc {
        name: b"add\0".as_ptr() as *const c_char,
        arity: 2,
        call: math_add,
    },
];

unsafe impl Sync for HelFunctionDesc {}
unsafe impl Sync for HelFunctionTable {}

static MATH_TABLE: HelFunctionTable = HelFunctionTable {
    count: 5,
    entries: MATH_FUNCTIONS.as_ptr(),
};

#[no_mangle]
pub extern "C" fn helheim_get_function_table() -> *const HelFunctionTable {
    &MATH_TABLE as *const HelFunctionTable
}

// =============================================================================
// Module initialization
// =============================================================================

#[no_mangle]
pub extern "C" fn helheim_module_init(ctx: *mut HelFFIContext) -> c_int {
    if ctx.is_null() {
        return -1;
    }

    unsafe {
        if (*ctx).abi_version != HEL_ABI_VERSION {
            if let Some(reporter) = (*ctx).report_error {
                let msg = b"ABI version mismatch\0".as_ptr() as *const c_char;
                reporter(ctx, -5, msg);
            }
            return -5;
        }

        // Optional: log that we loaded
        if let Some(log) = (*ctx).log {
            let msg = b"[helheim-math-plugin] initialized successfully\0".as_ptr() as *const c_char;
            log(msg);
        }
    }

    HEL_ERR_OK
}