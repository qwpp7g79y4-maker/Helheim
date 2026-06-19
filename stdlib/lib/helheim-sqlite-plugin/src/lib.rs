//! SQLite Native Plugin for Helheim
use std::ffi::{c_char, c_int, c_void, CString};
use rusqlite::Connection;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use lazy_static::lazy_static;

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelValueTag { Null = 0, Int = 1, Float = 2, String = 3, List = 4, ResourceHandle = 5 }

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
pub struct HelValue { pub tag: HelValueTag, pub data: HelValueData }

#[repr(C)]
#[derive(Clone, Copy)]
pub union HelValueData { pub i: i64, pub f: f64, pub str: HelString, pub list: HelList, pub res: HelResourceHandle, pub _pad: [u8; 16] }

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
pub struct HelFunctionDesc { pub name: *const c_char, pub arity: u32, pub call: HelFunctionCall }
unsafe impl Sync for HelFunctionDesc {}

#[repr(C)]
pub struct HelFunctionTable { pub count: u32, pub entries: *const HelFunctionDesc }
unsafe impl Sync for HelFunctionTable {}

const HEL_ABI_VERSION: u32 = 1;
const HEL_ERR_OK: c_int = 0;
const HEL_ERR_INVALID_ARG: c_int = -2;

unsafe fn report_error(ctx: *mut HelFFIContext, code: c_int, msg: &str) {
    if ctx.is_null() { return; }
    let cmsg = CString::new(msg).unwrap();
    if let Some(reporter) = (*ctx).report_error { reporter(ctx, code, cmsg.as_ptr()); }
    (*ctx).last_error_code = code;
    if !(*ctx).last_error_message.is_null() {
        drop(CString::from_raw((*ctx).last_error_message as *mut c_char));
    }
    (*ctx).last_error_message = cmsg.into_raw();
}

// Global state securely synchronized for Tokio async environment
lazy_static! {
    static ref NEXT_ID: AtomicU64 = AtomicU64::new(1);
    static ref DB_CONNECTIONS: Mutex<std::collections::HashMap<u64, Connection>> = Mutex::new(std::collections::HashMap::new());
}

extern "C" fn sqlite_open(
    ctx: *mut HelFFIContext,
    args: *const HelValue,
    arity: u32,
    out: *mut HelValue,
) -> c_int {
    if arity < 1 || args.is_null() {
        unsafe { report_error(ctx, HEL_ERR_INVALID_ARG, "sqlite::open expects 1 string argument"); }
        return HEL_ERR_INVALID_ARG;
    }
    unsafe {
        let val = *args.add(0);
        if val.tag != HelValueTag::String {
            report_error(ctx, HEL_ERR_INVALID_ARG, "sqlite::open path must be a string");
            return HEL_ERR_INVALID_ARG;
        }
        let path_bytes = std::slice::from_raw_parts(val.data.str.ptr, val.data.str.len);
        let path = match std::str::from_utf8(path_bytes) {
            Ok(p) => p,
            Err(_) => {
                report_error(ctx, HEL_ERR_INVALID_ARG, "sqlite::open path is not valid utf-8");
                return HEL_ERR_INVALID_ARG;
            }
        };

        match Connection::open(path) {
            Ok(conn) => {
                let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
                DB_CONNECTIONS.lock().unwrap().insert(id, conn);
                
                if !out.is_null() {
                    let kind_ptr = b"sqlite\0".as_ptr() as *const c_char;
                    *out = HelValue {
                        tag: HelValueTag::ResourceHandle,
                        data: HelValueData { res: HelResourceHandle { kind: kind_ptr, id } },
                    };
                }
                HEL_ERR_OK
            }
            Err(e) => {
                report_error(ctx, -1, &format!("sqlite::open failed: {}", e));
                -1
            }
        }
    }
}

extern "C" fn sqlite_exec(
    ctx: *mut HelFFIContext,
    args: *const HelValue,
    arity: u32,
    _out: *mut HelValue,
) -> c_int {
    if arity < 2 || args.is_null() {
        unsafe { report_error(ctx, HEL_ERR_INVALID_ARG, "sqlite::exec expects handle and sql"); }
        return HEL_ERR_INVALID_ARG;
    }
    unsafe {
        let handle_val = *args.add(0);
        let sql_val = *args.add(1);

        if handle_val.tag != HelValueTag::ResourceHandle || sql_val.tag != HelValueTag::String {
            report_error(ctx, HEL_ERR_INVALID_ARG, "Invalid types for sqlite::exec");
            return HEL_ERR_INVALID_ARG;
        }

        let id = handle_val.data.res.id;
        let sql_bytes = std::slice::from_raw_parts(sql_val.data.str.ptr, sql_val.data.str.len);
        let sql = match std::str::from_utf8(sql_bytes) {
            Ok(s) => s,
            Err(_) => {
                report_error(ctx, HEL_ERR_INVALID_ARG, "sqlite::exec sql is not valid utf-8");
                return HEL_ERR_INVALID_ARG;
            }
        };

        // Release lock before calling report_error — prevents deadlock if reporter callback
        // re-enters the plugin (e.g. via another sqlite:: call on the same thread).
        let result = {
            let mut conns = DB_CONNECTIONS.lock().unwrap();
            conns.get_mut(&id).map(|conn| conn.execute_batch(sql).map_err(|e| e.to_string()))
        };

        match result {
            Some(Ok(())) => HEL_ERR_OK,
            Some(Err(e)) => {
                report_error(ctx, -1, &format!("sqlite::exec failed: {}", e));
                -1
            }
            None => {
                report_error(ctx, -1, "Invalid or closed sqlite connection");
                -1
            }
        }
    }
}

static SQLITE_FUNCTIONS: [HelFunctionDesc; 2] = [
    HelFunctionDesc { name: b"open\0".as_ptr() as *const c_char, arity: 1, call: sqlite_open },
    HelFunctionDesc { name: b"exec\0".as_ptr() as *const c_char, arity: 2, call: sqlite_exec },
];

static SQLITE_TABLE: HelFunctionTable = HelFunctionTable {
    count: 2, entries: SQLITE_FUNCTIONS.as_ptr(),
};

#[no_mangle]
pub extern "C" fn helheim_get_function_table() -> *const HelFunctionTable {
    &SQLITE_TABLE as *const HelFunctionTable
}

#[no_mangle]
pub extern "C" fn helheim_module_init(ctx: *mut HelFFIContext) -> c_int {
    if ctx.is_null() { return -1; }
    unsafe {
        if (*ctx).abi_version != HEL_ABI_VERSION { return -5; }
        if let Some(log) = (*ctx).log { log(b"[helheim-sqlite-plugin] initialized\0".as_ptr() as *const c_char); }
    }
    HEL_ERR_OK
}
