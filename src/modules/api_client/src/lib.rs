/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* src/lib.rs - FFI bridge and entry point for the api_client Rust module.
*
* This file is the core of the C-Rust interaction. It implements the
* `ph_core_api.h` contract, allowing this Rust crate to be compiled as a
* C shared library (.so/.dll) and be loaded by the main C application.
*
* It handles data type translation between Rust and C, manages the module
* lifecycle, and dispatches commands to the appropriate Rust functions.
* It uses a global, lazily-initialized Tokio runtime to bridge the synchronous
* FFI world with the asynchronous, high-performance I/O world of modern Rust.
*
* SPDX-License-Identifier: Apache-2.0 */

// We declare the github_handler module, making its public items available.
mod github_handler;

use github_handler::{set_repository, ApiProvider, GitHubHandler};
use libc::{c_char, c_int};
use once_cell::sync::Lazy;
use std::ffi::{CStr, CString};
use std::ptr;
use std::sync::Mutex;
use tokio::runtime::Runtime;

// --- C ABI Definitions ---
// These definitions mirror the C header file `ph_core_api.h`.

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

// The function pointer type for the logger provided by the C core.
type LogFn = extern "C" fn(LogLevel, *const c_char, *const c_char);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CoreContext {
    log: Option<LogFn>,
}

#[repr(C)]
pub struct ModuleInfo {
    name: *const c_char,
    version: *const c_char,
    description: *const c_char,
    commands: *const *const c_char,
}

// SAFETY: This is safe because the pointers in ModuleInfo point to the data
// of other `static Lazy` variables. This data is guaranteed to be valid for
// the entire lifetime of the program and is immutable. Therefore, sharing
// ModuleInfo across threads (Sync) and sending it between threads (Send) is safe.
unsafe impl Send for ModuleInfo {}
unsafe impl Sync for ModuleInfo {}


// --- Global State Management ---

// A global, thread-safe, lazily-initialized Tokio runtime.
static RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime")
});

// The core context provided by the host application.
static CORE_CONTEXT: Mutex<Option<CoreContext>> = Mutex::new(None);

// --- Logging Helper ---

/// A safe Rust wrapper around the C logging function pointer.
fn log_to_core(level: LogLevel, message: &str) {
    let context_guard = CORE_CONTEXT.lock().unwrap();
    if let Some(context) = *context_guard {
        if let Some(log_fn) = context.log {
            let module_name = CString::new("API_CLIENT_RUST").unwrap();
            let msg = CString::new(message).unwrap();
            log_fn(level, module_name.as_ptr(), msg.as_ptr());
        }
    }
}

// --- Module Info Statics ---

// Using `Lazy` to create CStrings ensures they are allocated only once and live
// for the entire duration of the program, so we don't need to manually free them.
static MODULE_NAME: Lazy<CString> = Lazy::new(|| CString::new("api_client").unwrap());
static MODULE_VERSION: Lazy<CString> = Lazy::new(|| CString::new("1.1.0").unwrap());
static MODULE_DESCRIPTION: Lazy<CString> = Lazy::new(|| CString::new("Client for interacting with Git provider APIs (e.g., GitHub) [Rust]").unwrap());
static SRP_COMMAND: Lazy<CString> = Lazy::new(|| CString::new("srp").unwrap());

// --- FIX: WRAP THE POINTER ARRAY IN A THREAD-SAFE STRUCT ---

// A wrapper struct for the C-compatible command list.
struct CommandList {
    ptrs: [*const c_char; 2],
}

// SAFETY: This is safe because the pointers in the array point to static,
// immutable CString data that lives for the program's entire duration.
// This makes the wrapper struct safe to send and share across threads.
unsafe impl Send for CommandList {}
unsafe impl Sync for CommandList {}

// The null-terminated array of command strings for the C side, now wrapped.
static SUPPORTED_COMMANDS: Lazy<CommandList> = Lazy::new(|| CommandList {
    ptrs: [
        SRP_COMMAND.as_ptr(),
        ptr::null(), // Null terminator
    ],
});

static MODULE_INFO: Lazy<ModuleInfo> = Lazy::new(|| ModuleInfo {
    name: MODULE_NAME.as_ptr(),
    version: MODULE_VERSION.as_ptr(),
    description: MODULE_DESCRIPTION.as_ptr(),
    // We now get the pointer from our wrapper struct.
    commands: SUPPORTED_COMMANDS.ptrs.as_ptr(),
});

// --- C-compatible API Implementation ---

/// Exposes the module's metadata.
#[no_mangle]
pub extern "C" fn module_get_info() -> *const ModuleInfo {
    &*MODULE_INFO
}

/// Initializes the module, receiving the context from the core.
#[no_mangle]
pub extern "C" fn module_init(context: *const CoreContext) -> Status {
    if context.is_null() {
        return Status::ErrorInitFailed;
    }
    let mut context_guard = CORE_CONTEXT.lock().unwrap();
    *context_guard = Some(unsafe { *context });

    log_to_core(LogLevel::Info, "api_client (Rust) module initialized successfully.");
    Status::Success
}

/// Executes a command passed from the core.
#[no_mangle]
pub extern "C" fn module_exec(argc: c_int, argv: *const *const c_char) -> Status {
    let args = unsafe { c_args_to_vec(argc, argv) };
    if args.is_empty() {
        log_to_core(LogLevel::Error, "Execution called with no command.");
        return Status::ErrorInvalidArgs;
    }

    let command = &args[0];
    let command_args = &args[1..];
    log_to_core(LogLevel::Debug, &format!("Dispatching command: {}", command));

    let result = RUNTIME.block_on(async {
        match command.as_str() {
            "srp" => {
                let provider: Box<dyn ApiProvider> = Box::new(GitHubHandler::new());
                set_repository(provider, command_args).await
            }
            _ => Err(format!("Unknown command '{}' for api_client module", command).into()),
        }
    });

    match result {
        Ok(_) => Status::Success,
        Err(e) => {
            log_to_core(LogLevel::Error, &format!("Command failed: {}", e));
            Status::ErrorExecFailed
        }
    }
}

/// Cleans up module resources.
#[no_mangle]
pub extern "C" fn module_cleanup() {
    log_to_core(LogLevel::Info, "api_client (Rust) module cleaned up.");
}

// --- Helper Functions ---

/// Converts C-style argc/argv to a Rust `Vec<String>`.
/// This function is `unsafe` because it dereferences raw C pointers.
unsafe fn c_args_to_vec(argc: c_int, argv: *const *const c_char) -> Vec<String> {
    if argc <= 0 || argv.is_null() {
        return Vec::new();
    }
    let mut result = Vec::with_capacity(argc as usize);
    for i in 0..argc {
        let arg_ptr = *argv.add(i as usize);
        if !arg_ptr.is_null() {
            let c_str = CStr::from_ptr(arg_ptr);
            result.push(c_str.to_string_lossy().into_owned());
        }
    }
    result
}