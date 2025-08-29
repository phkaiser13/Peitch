/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/issue_tracker/src/lib.rs
*
* This file provides the Foreign Function Interface (FFI) bridge for the
* issue_tracker module. It exposes a C-compatible API for the core application,
* allowing it to execute asynchronous operations related to issue tracking
* services (e.g., GitHub Issues, Jira).
*
* Like other async modules, it relies on a globally shared Tokio runtime to
* bridge the synchronous C world with asynchronous Rust. The critical design
* choice here is the direct use of the `tokio::runtime::Runtime` without a
* `Mutex` wrapper. This is essential for preventing deadlocks that could occur
* if an async task were to make a re-entrant call into the module, as the
* Runtime itself is already thread-safe.
*
* SPDX-License-Identifier: Apache-2.0 */

use libc::{c_char, c_int};
use std::ffi::{CStr, CString};
use std::sync::Mutex;

use lazy_static::lazy_static;
use tokio::runtime::Runtime;

// The internal module containing the actual issue service logic.
mod issue_service;

// --- FFI Type Definitions (matching the core C API) ---

#[repr(C)]
pub enum phStatus {
    Success = 0,
    ErrorInvalidArgs = -2,
    ErrorInitFailed = -4,
    ErrorExecFailed = -5,
}

#[repr(C)]
pub enum phLogLevel {
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

// Function pointer type for the core logger callback.
type LogFn = extern "C" fn(phLogLevel, *const c_char, *const c_char);

#[repr(C)]
pub struct phCoreContext {
    log: Option<LogFn>,
}

// --- Global State Management ---

lazy_static! {
    // The global Tokio runtime for executing all async tasks within this module.
    // CRITICAL: It is NOT wrapped in a Mutex. `tokio::runtime::Runtime` is thread-safe.
    // Wrapping it in a Mutex creates a risk of deadlocks if an async task ever
    // needs to call back into a function that also tries to lock the runtime.
    static ref RUNTIME: Runtime = Runtime::new().expect("Failed to create Tokio runtime");

    // The core context is stored globally to allow logging from anywhere.
    // A Mutex is necessary here because it's written to once during initialization.
    static ref CORE_CONTEXT: Mutex<Option<phCoreContext>> = Mutex::new(None);
}

/// Logs a message back to the core application using the provided callback.
fn log_to_core(level: phLogLevel, message: &str) {
    if let Ok(guard) = CORE_CONTEXT.lock() {
        if let Some(context) = &*guard {
            if let Some(log_fn) = context.log {
                let module_name = CString::new("ISSUE_TRACKER").unwrap();
                let msg = CString::new(message).unwrap_or_else(|_| {
                    CString::new("Error: Log message contained null bytes.").unwrap()
                });
                log_fn(level, module_name.as_ptr(), msg.as_ptr());
            }
        }
    }
}

// --- Module Metadata ---

#[repr(C)]
pub struct phModuleInfo {
    name: *const c_char,
    version: *const c_char,
    description: *const c_char,
    commands: *const *const c_char,
}

// A wrapper to safely mark phModuleInfo as Sync.
struct SafeModuleInfo(phModuleInfo);
unsafe impl Sync for SafeModuleInfo {}

// Static C-strings for module information. The null terminator is included.
static MODULE_NAME: &[u8] = b"issue_tracker\0";
static MODULE_VERSION: &[u8] = b"1.0.0\0";
static MODULE_DESC: &[u8] = b"Interacts with issue tracking services like GitHub Issues.\0";

// A null-terminated array of supported command strings.
const SUPPORTED_COMMANDS_PTRS: &[*const c_char] = &[
    b"issue-get\0".as_ptr() as *const c_char,
    std::ptr::null(),
];

static MODULE_INFO: SafeModuleInfo = SafeModuleInfo(phModuleInfo {
    name: MODULE_NAME.as_ptr() as *const c_char,
    version: MODULE_VERSION.as_ptr() as *const c_char,
    description: MODULE_DESC.as_ptr() as *const c_char,
    commands: SUPPORTED_COMMANDS_PTRS.as_ptr(),
});

// --- FFI Function Implementations ---

#[no_mangle]
pub extern "C" fn module_get_info() -> *const phModuleInfo {
    &MODULE_INFO.0
}

#[no_mangle]
pub extern "C" fn module_init(context: *const phCoreContext) -> phStatus {
    if context.is_null() {
        return phStatus::ErrorInitFailed;
    }

    if let Ok(mut guard) = CORE_CONTEXT.lock() {
        *guard = Some(unsafe { std::ptr::read(context) });
    } else {
        // Mutex is poisoned.
        return phStatus::ErrorInitFailed;
    }

    // Eagerly initialize the runtime and enter its context.
    // This is the idiomatic way to ensure the runtime is ready without locking.
    let _enter = RUNTIME.enter();

    log_to_core(phLogLevel::Info, "issue_tracker module initialized successfully.");
    phStatus::Success
}

#[no_mangle]
pub extern "C" fn module_exec(argc: c_int, argv: *const *const c_char) -> phStatus {
    if argv.is_null() || argc < 1 {
        log_to_core(phLogLevel::Error, "Execution called with no arguments.");
        return phStatus::ErrorInvalidArgs;
    }

    let args: Vec<String> = (0..argc as isize)
        .map(|i| unsafe {
            let arg_ptr = *argv.offset(i);
            CStr::from_ptr(arg_ptr).to_string_lossy().into_owned()
        })
        .collect();

    let command = args[0].as_str();
    let user_args = &args[1..];

    // Get a reference to the global runtime and block on the async execution.
    // No .lock() is needed, which eliminates the deadlock risk.
    let result = RUNTIME.block_on(async {
        match command {
            "issue-get" => issue_service::handle_get_issue(user_args).await,
            _ => {
                let err_msg = format!("Unknown command '{}' for issue_tracker module", command);
                Err(err_msg)
            }
        }
    });

    match result {
        Ok(_) => {
            log_to_core(phLogLevel::Info, "Command executed successfully.");
            phStatus::Success
        }
        Err(e) => {
            log_to_core(phLogLevel::Error, &format!("Command execution failed: {}", e));
            phStatus::ErrorExecFailed
        }
    }
}

#[no_mangle]
pub extern "C" fn module_cleanup() {
    log_to_core(phLogLevel::Info, "issue_tracker module cleaned up.");
}