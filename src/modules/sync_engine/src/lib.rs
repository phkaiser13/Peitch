/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/sync_engine/src/lib.rs
*
* This file provides the Foreign Function Interface (FFI) bridge for the
* sync_engine module. It exposes a C-compatible API that the core application
* can use to initialize, execute, and clean up the module.
*
* The core of this module's architecture is a globally shared Tokio runtime,
* managed by lazy_static. This allows asynchronous Rust code to be executed
* from a synchronous C context. The Mutex wrapper around the Runtime has been
* intentionally removed to prevent potential deadlocks. The tokio::runtime::Runtime
* is thread-safe (Sync) by design, making an external lock not only redundant
* but dangerous, as a task attempting a re-entrant call to the module would
* self-deadlock.
*
* SPDX-License-Identifier: Apache-2.0 */

use libc::{c_char, c_int};
use std::ffi::{CStr, CString};
use std::sync::Mutex;

use lazy_static::lazy_static;
use tokio::runtime::Runtime;

// The internal module containing the actual synchronization logic.
mod sync;

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
                let module_name = CString::new("SYNC_ENGINE").unwrap();
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
static MODULE_NAME: &[u8] = b"sync_engine\0";
static MODULE_VERSION: &[u8] = b"1.0.0\0";
static MODULE_DESC: &[u8] = b"Performs complex, bi-directional repository synchronization.\0";

// A null-terminated array of supported command strings.
const SUPPORTED_COMMANDS_PTRS: &[*const c_char] = &[
    b"sync-run\0".as_ptr() as *const c_char,
    b"sync-config\0".as_ptr() as *const c_char,
    b"sync-status\0".as_ptr() as *const c_char,
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
        // Cannot log here as context is not available.
        return phStatus::ErrorInitFailed;
    }

    // Safely store the context provided by the core.
    if let Ok(mut guard) = CORE_CONTEXT.lock() {
        *guard = Some(unsafe { std::ptr::read(context) });
    } else {
        // Mutex is poisoned, a panic occurred while it was locked.
        return phStatus::ErrorInitFailed;
    }

    // Eagerly initialize the runtime and enter its context.
    // This is the idiomatic way to ensure the runtime is ready without locking.
    // The returned guard must be kept in scope for the duration of the context.
    let _enter = RUNTIME.enter();

    log_to_core(phLogLevel::Info, "sync_engine module initialized successfully.");
    phStatus::Success
}

#[no_mangle]
pub extern "C" fn module_exec(argc: c_int, argv: *const *const c_char) -> phStatus {
    if argv.is_null() || argc < 1 {
        log_to_core(phLogLevel::Error, "Execution failed: No command provided.");
        return phStatus::ErrorInvalidArgs;
    }

    // Safely parse arguments from C into a Vec<String>.
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
            "sync-run" => sync::handle_run_sync(user_args).await,
            // Add handlers for other commands here.
            _ => Err(format!("Unknown command '{}' for sync_engine", command)),
        }
    });

    match result {
        Ok(success_message) => {
            log_to_core(phLogLevel::Info, &success_message);
            // For CLI tools, printing to stdout is often expected on success.
            println!("{}", success_message);
            phStatus::Success
        }
        Err(e) => {
            let error_message = format!("Command failed: {}", e);
            log_to_core(phLogLevel::Error, &error_message);
            // Printing to stderr is standard for errors.
            eprintln!("[ph SYNC ERROR] {}", e);
            phStatus::ErrorExecFailed
        }
    }
}

#[no_mangle]
pub extern "C" fn module_cleanup() {
    log_to_core(phLogLevel::Info, "sync_engine module cleaned up.");
    // The RUNTIME will be dropped automatically when the program exits.
    // No explicit cleanup is required for lazy_static variables.
}