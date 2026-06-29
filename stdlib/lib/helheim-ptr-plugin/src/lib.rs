use std::ffi::{c_char, c_int, c_void};

// Minimal re-definition of Helheim FFI types for this plugin
pub const HEL_ABI_VERSION: u32 = 1;
pub const HEL_ERR_OK: c_int = 0;
pub const HEL_ERR_INVALID_ARG: c_int = -2;

#[repr(u32)]
#[derive(Clone, Copy)]
pub enum HelValueTag {
    Null = 0,
    Int = 1,
    Float = 2,
    String = 3,
    List = 4,
    ResourceHandle = 5,
    Pointer = 6,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct HelString { pub ptr: *const u8, pub len: usize }

#[repr(C)]
#[derive(Clone, Copy)]
pub struct HelList { pub ptr: *const HelValue, pub len: usize }

#[repr(C)]
#[derive(Clone, Copy)]
pub struct HelResourceHandle { pub kind: *const c_char, pub id: u64 }

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
    pub ptr: u64,
    pub _pad: [u8; 16],
}

impl HelValue {
    pub const NULL: HelValue = HelValue { tag: HelValueTag::Null, data: HelValueData { i: 0 } };
    pub fn int(i: i64) -> Self { HelValue { tag: HelValueTag::Int, data: HelValueData { i } } }
    pub fn pointer(p: u64) -> Self { HelValue { tag: HelValueTag::Pointer, data: HelValueData { ptr: p } } }
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

pub type HelFunctionCall = extern "C" fn(*mut HelFFIContext, *const HelValue, u32, *mut HelValue) -> c_int;

#[repr(C)]
pub struct HelFunctionDesc {
    pub name: *const c_char,
    pub arity: u32,
    pub call: HelFunctionCall,
}

unsafe impl Sync for HelFunctionDesc {}
unsafe impl Send for HelFunctionDesc {}

#[repr(C)]
pub struct HelFunctionTable {
    pub count: u32,
    pub entries: *const HelFunctionDesc,
}

unsafe impl Sync for HelFunctionTable {}
unsafe impl Send for HelFunctionTable {}

// --- Functions ---

extern "C" fn ptr_cast_from_handle(
    _ctx: *mut HelFFIContext,
    args: *const HelValue,
    arity: u32,
    out: *mut HelValue,
) -> c_int {
    if arity != 1 { return HEL_ERR_INVALID_ARG; }
    let arg = unsafe { &*args };
    match arg.tag {
        HelValueTag::ResourceHandle => {
            let id = unsafe { arg.data.res.id };
            unsafe { *out = HelValue::pointer(id); }
            HEL_ERR_OK
        }
        _ => HEL_ERR_INVALID_ARG,
    }
}

extern "C" {
    fn malloc(size: usize) -> *mut c_void;
    fn free(ptr: *mut c_void);
}

extern "C" fn ptr_alloc(
    _ctx: *mut HelFFIContext,
    args: *const HelValue,
    arity: u32,
    out: *mut HelValue,
) -> c_int {
    if arity != 1 { return HEL_ERR_INVALID_ARG; }
    let arg = unsafe { &*args };
    match arg.tag {
        HelValueTag::Int => {
            let size = unsafe { arg.data.i } as usize;
            if size == 0 {
                unsafe { *out = HelValue::NULL; }
                return HEL_ERR_OK;
            }
            let p = unsafe { malloc(size) };
            unsafe { *out = HelValue::pointer(p as u64); }
            HEL_ERR_OK
        }
        _ => HEL_ERR_INVALID_ARG,
    }
}

extern "C" fn ptr_free(
    _ctx: *mut HelFFIContext,
    args: *const HelValue,
    arity: u32,
    out: *mut HelValue,
) -> c_int {
    if arity != 1 { return HEL_ERR_INVALID_ARG; }
    let arg_ptr = unsafe { &*args };
    
    if matches!(arg_ptr.tag, HelValueTag::Pointer) {
        let p = unsafe { arg_ptr.data.ptr };
        if p != 0 {
            unsafe { free(p as *mut c_void); }
        }
        unsafe { *out = HelValue::NULL; }
        HEL_ERR_OK
    } else {
        HEL_ERR_INVALID_ARG
    }
}

extern "C" fn ptr_read_u8(
    _ctx: *mut HelFFIContext,
    args: *const HelValue,
    arity: u32,
    out: *mut HelValue,
) -> c_int {
    if arity != 1 { return HEL_ERR_INVALID_ARG; }
    let arg = unsafe { &*args };
    match arg.tag {
        HelValueTag::Pointer => {
            let p = unsafe { arg.data.ptr };
            if p == 0 { return HEL_ERR_INVALID_ARG; }
            let val = unsafe { *(p as *const u8) };
            unsafe { *out = HelValue::int(val as i64); }
            HEL_ERR_OK
        }
        _ => HEL_ERR_INVALID_ARG,
    }
}

extern "C" fn ptr_write_u8(
    _ctx: *mut HelFFIContext,
    args: *const HelValue,
    arity: u32,
    out: *mut HelValue,
) -> c_int {
    if arity != 2 { return HEL_ERR_INVALID_ARG; }
    let arg_ptr = unsafe { &*args };
    let arg_val = unsafe { &*args.add(1) };
    
    if matches!(arg_ptr.tag, HelValueTag::Pointer) && matches!(arg_val.tag, HelValueTag::Int) {
        let p = unsafe { arg_ptr.data.ptr };
        if p == 0 { return HEL_ERR_INVALID_ARG; }
        let val = unsafe { arg_val.data.i } as u8;
        unsafe { *(p as *mut u8) = val; }
        unsafe { *out = HelValue::NULL; }
        HEL_ERR_OK
    } else {
        HEL_ERR_INVALID_ARG
    }
}

static FUNCTIONS: [HelFunctionDesc; 5] = [
    HelFunctionDesc {
        name: b"cast_from_handle\0".as_ptr() as *const c_char,
        arity: 1,
        call: ptr_cast_from_handle,
    },
    HelFunctionDesc {
        name: b"alloc\0".as_ptr() as *const c_char,
        arity: 1,
        call: ptr_alloc,
    },
    HelFunctionDesc {
        name: b"free\0".as_ptr() as *const c_char,
        arity: 1,
        call: ptr_free,
    },
    HelFunctionDesc {
        name: b"read_u8\0".as_ptr() as *const c_char,
        arity: 1,
        call: ptr_read_u8,
    },
    HelFunctionDesc {
        name: b"write_u8\0".as_ptr() as *const c_char,
        arity: 2,
        call: ptr_write_u8,
    },
];

static TABLE: HelFunctionTable = HelFunctionTable {
    count: FUNCTIONS.len() as u32,
    entries: FUNCTIONS.as_ptr(),
};

#[no_mangle]
pub extern "C" fn helheim_module_init(ctx: *mut HelFFIContext) -> c_int {
    if ctx.is_null() { return -2; }
    if unsafe { (*ctx).abi_version } != HEL_ABI_VERSION {
        return -5; // HEL_ERR_ABI_MISMATCH
    }
    HEL_ERR_OK
}

#[no_mangle]
pub extern "C" fn helheim_get_function_table() -> *const HelFunctionTable {
    &TABLE
}
