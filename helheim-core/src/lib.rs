pub mod cli;
pub mod common;
pub mod ffi; // Dynamic FFI / Native Module System (C-ABI)
pub mod gpu;
pub mod legacy; // The Archives
pub mod network;
pub mod orchestra;
pub mod shield;
pub mod std; // De Standaard Bibliotheek (New)

pub use common::rune::RuneEngine;
pub use shield::HelheimLock;
pub use shield::HelheimShield;

// FFI / Dynamic Modules
pub use ffi::{
    create_ffi_context, marshal_helheimtype_to_helvalue, report_error,
    unmarshal_helvalue_to_helheimtype, HelFFIContext, HelFunctionCall, HelFunctionDesc,
    HelFunctionTable, HelList, HelResourceHandle, HelString, HelValue, HelValueData,
    HelValueTag, LoadedWasmModule, WasmModuleLoader, HEL_ABI_VERSION, HEL_ERR_OK,
    HEL_ERR_GENERIC, HEL_ERR_INVALID_ARG, HEL_ERR_OUT_OF_MEMORY,
};

pub use orchestra::package_manager::{PackageManager, VerifiedModule, PackageManifest};
