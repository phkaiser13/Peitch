/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* src/lib.rs
* Unified FFI bridge and dispatcher for devops_automation.
*
* This file is the single entry point for the C core to interact with this
* module. It implements the `ph_core_api.h` contract and contains a
* central dispatcher (`module_exec`) that routes commands to the appropriate
* handler function in the `commands.rs` module.
*
* It reuses the same safe FFI patterns established in the other Rust modules,
* such as `once_cell` for static data and a clear separation between safe
* Rust code and the `unsafe` FFI boundary.
*
* SPDX-License-Identifier: Apache-2.0 */

mod commands;
mod process_wrapper;

use libc::{c_char, c_int};
use once_cell::sync::Lazy;
use std::ffi::{CStr, CString};
use std::ptr;

// --- C ABI Definitions (should be identical to other modules) ---

#[repr(C)]
pub enum Status {
    Success = 0,
    ErrorInitFailed = 1,
    ErrorInvalidArgs = 2,
    ErrorExecFailed = 3,
}

#[repr(C)]
pub enum LogLevel {
    Debug = 0,
    Info = 1,
    Warn = 2,
    Error = 3,
    Fatal = 4,
}

#[repr(C)]
pub struct ModuleInfo {
    name: *const c_char,
    version: *const c_char,
    description: *const c_char,
    commands: *const *const c_char,
}

// --- Module Info Statics ---

// These CStrings are managed by Lazy and have a 'static lifetime.
// They are the safe, owned source of truth for our module information.
static MODULE_NAME: Lazy<CString> = Lazy::new(|| CString::new("devops_automation").unwrap());
static MODULE_VERSION: Lazy<CString> = Lazy::new(|| CString::new("1.0.1").unwrap());
static MODULE_DESCRIPTION: Lazy<CString> = Lazy::new(|| CString::new("Wrapper for DevOps tools like Terraform and Vault [Rust]").unwrap());

static TF_PLAN_CMD: Lazy<CString> = Lazy::new(|| CString::new("tf-plan").unwrap());
static TF_APPLY_CMD: Lazy<CString> = Lazy::new(|| CString::new("tf-apply").unwrap());
static VAULT_READ_CMD: Lazy<CString> = Lazy::new(|| CString::new("vault-read").unwrap());

/// A container for all static C-API data.
/// This struct holds the C-compatible ModuleInfo and the array of command pointers it refers to.
/// By placing them together, we ensure the command array has the same 'static lifetime as the
/// ModuleInfo struct that points to it.
struct StaticApiData {
    info: ModuleInfo,
    // This array holds the pointers that `info.commands` will point to.
    commands: [*const c_char; 4],
}

/// # Safety
/// This implementation is safe because the data within `StaticApiData` is initialized only once
/// inside a `Lazy` static. The pointers it contains (`*const c_char`) point to the memory
/// managed by other `Lazy<CString>` statics, which are guaranteed to live for the entire
/// duration of the program ('static lifetime). Therefore, the pointers will always be valid,
/// and the structure is safe to be sent and shared across threads.
unsafe impl Send for StaticApiData {}
unsafe impl Sync for StaticApiData {}

/// This single static holds all the data required by `module_get_info`.
/// It's initialized once on the first access in a thread-safe manner.
/// It constructs the `ModuleInfo` and the list of command pointers, ensuring
/// that all pointers are valid and point to data with a 'static lifetime.
static STATIC_API_DATA: Lazy<StaticApiData> = Lazy::new(|| {
    // First, create the array of C-string pointers.
    let commands_array = [
        TF_PLAN_CMD.as_ptr(),
        TF_APPLY_CMD.as_ptr(),
        VAULT_READ_CMD.as_ptr(),
        ptr::null(), // Null terminator for the C side.
    ];

    // Now, create the struct that will hold both the ModuleInfo and the command array itself.
    // This is crucial: it ensures the array `commands_array` is not a temporary value
    // on the stack but is stored statically alongside the `info` struct.
    let mut data = StaticApiData {
        info: ModuleInfo {
            name: MODULE_NAME.as_ptr(),
            version: MODULE_VERSION.as_ptr(),
            description: MODULE_DESCRIPTION.as_ptr(),
            // This pointer will be correctly set in the next line.
            commands: ptr::null(),
        },
        commands: commands_array,
    };

    // Point `info.commands` to the `commands` field within the same struct.
    // This guarantees the pointer's validity for the lifetime of `STATIC_API_DATA`.
    data.info.commands = data.commands.as_ptr();

    data
});


// --- C-compatible API Implementation ---

#[no_mangle]
pub extern "C" fn module_get_info() -> *const ModuleInfo {
    // Return a stable pointer to the `info` field of our static data.
    &STATIC_API_DATA.info
}

#[no_mangle]
pub extern "C" fn module_init(_context: *const std::ffi::c_void) -> Status {
    // In this module, we don't need the context, but we keep the signature for consistency.
    println!("[DEVOPS_AUTOMATION] devops_automation (Rust) module initialized.");
    Status::Success
}

#[no_mangle]
pub extern "C" fn module_exec(argc: c_int, argv: *const *const c_char) -> Status {
    // This unsafe block is necessary to interact with C pointers, but the logic
    // is contained within the safe `c_args_to_vec` helper function.
    let args = unsafe { c_args_to_vec(argc, argv) };
    if args.is_empty() {
        eprintln!("[DEVOPS_AUTOMATION ERROR] Execution called with no command.");
        return Status::ErrorInvalidArgs;
    }

    let command = &args[0];
    let user_args = &args[1..];

    let result = match command.as_str() {
        "tf-plan" => commands::handle_terraform_plan(user_args),
        "tf-apply" => commands::handle_terraform_apply(user_args),
        "vault-read" => commands::handle_vault_read(user_args),
        _ => Err(format!("Unknown command '{}' for devops_automation module", command).into()),
    };

    match result {
        Ok(_) => Status::Success,
        Err(e) => {
            eprintln!("\n[ph ERROR] Command failed: {}", e);
            Status::ErrorExecFailed
        }
    }
}

#[no_mangle]
pub extern "C" fn module_cleanup() {
    println!("[DEVOPS_AUTOMATION] devops_automation (Rust) module cleaned up.");
    // All CString memory is managed by `Lazy` and cleaned up automatically at program exit.
}

/// # Safety
/// The caller must ensure that `argc` and `argv` are valid.
/// `argc` must be the correct count of pointers in `argv`.
/// Each pointer in `argv` must point to a valid, null-terminated C string.
/// The memory pointed to by `argv` and its elements must be valid for the duration of this call.
unsafe fn c_args_to_vec(argc: c_int, argv: *const *const c_char) -> Vec<String> {
    if argc <= 0 || argv.is_null() {
        return Vec::new();
    }
    std::slice::from_raw_parts(argv, argc as usize)
        .iter()
        .map(|&p| CStr::from_ptr(p).to_string_lossy().into_owned())
        .collect()
}