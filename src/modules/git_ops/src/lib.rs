/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * lib.rs - FFI bridge and entry point for the git_ops Rust module.
 *
 * This file serves as the main library entry point and the Foreign Function
 * Interface (FFI) layer for the `git_ops` module. It is responsible for
 * exposing a C-compatible API that adheres to the contract defined in the
 * core `ph_core_api.h` header.
 *
 * Key responsibilities:
 * - Defining C-compatible data structures (`#[repr(C)]`).
 * - Implementing the required module functions (`module_get_info`, `module_init`,
 *   `module_exec`, `module_cleanup`) with C calling conventions.
 * - Safely handling data transfer between the C core and Rust (e.g.,
 *   converting C strings to Rust strings).
 * - Dispatching commands received from the core to the appropriate Rust
 *   functions within the `commands` submodule.
 * - Translating the specific Rust `CommandError` enum into the generic
 *   C-compatible `phStatus` enum for the core.
 *
 * SPDX-License-Identifier: Apache-2.0 */

// Import libc for C-compatible types
use libc::{c_char, c_int};
use std::ffi::{CStr, CString};
use std::sync::Mutex;

// Declare the other modules in our crate.
mod commands;
mod git_wrapper;

// --- C API Data Structure Definitions ---
// These must match the definitions in `ph_core_api.h` exactly.

#[repr(C)]
pub enum phStatus {
    Success = 0,
    ErrorGeneral = -1,
    ErrorInvalidArgs = -2,
    ErrorNotFound = -3,
    ErrorInitFailed = -4,
    ErrorExecFailed = -5,
}

#[repr(C)]
#[derive(Debug)] // Add Debug for easier logging
pub enum phLogLevel {
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

// A function pointer type for the logger callback from the C core.
type LogFn = extern "C" fn(phLogLevel, *const c_char, *const c_char);

#[repr(C)]
pub struct phCoreContext {
    log: Option<LogFn>,
    _get_config_value: Option<extern "C" fn()>,
    _print_ui: Option<extern "C" fn()>,
}

// Make the context globally and safely accessible within the Rust module.
lazy_static::lazy_static! {
    static ref CORE_CONTEXT: Mutex<Option<phCoreContext>> = Mutex::new(None);
}

// Helper function to log messages back to the C core.
fn log_to_core(level: phLogLevel, message: &str) {
    if let Ok(guard) = CORE_CONTEXT.lock() {
        if let Some(context) = &*guard {
            if let Some(log_fn) = context.log {
                let module_name = CString::new("GIT_OPS").unwrap();
                let msg = CString::new(message).unwrap();
                log_fn(level, module_name.as_ptr(), msg.as_ptr());
            }
        }
    }
}

// --- C-compatible API Implementation ---

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

// Define the static data for our module's information.
static MODULE_NAME: &[u8] = b"git_ops\0";
static MODULE_VERSION: &[u8] = b"0.2.0\0";
static MODULE_DESC: &[u8] =
    b"Provides intelligent, workflow-aware Git commands like add, commit, push.\0";

const SUPPORTED_COMMANDS_PTRS: &[*const c_char] = &[
    b"SND\0".as_ptr() as *const c_char,
    b"status\0".as_ptr() as *const c_char,
    // Add other future commands here
    std::ptr::null(), // Null terminator for the C array
];

static MODULE_INFO: SafeModuleInfo = SafeModuleInfo(phModuleInfo {
    name: MODULE_NAME.as_ptr() as *const c_char,
    version: MODULE_VERSION.as_ptr() as *const c_char,
    description: MODULE_DESC.as_ptr() as *const c_char,
    commands: SUPPORTED_COMMANDS_PTRS.as_ptr(),
});

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
        return phStatus::ErrorInitFailed;
    }
    log_to_core(
        phLogLevel::Info,
        "git_ops module initialized successfully.",
    );
    phStatus::Success
}

#[no_mangle]
pub extern "C" fn module_exec(argc: c_int, argv: *const *const c_char) -> phStatus {
    // Safely convert C-style arguments to a Rust `Vec<String>`.
    let args: Vec<String> = (0..argc as isize)
        .map(|i| unsafe {
            let c_str_ptr = *argv.offset(i);
            CStr::from_ptr(c_str_ptr).to_string_lossy().into_owned()
        })
        .collect();

    if args.is_empty() {
        log_to_core(
            phLogLevel::Error,
            "Execution called with no arguments.",
        );
        return phStatus::ErrorInvalidArgs;
    }

    // Dispatch to the correct command handler based on the first argument.
    let command = args[0].as_str();
    log_to_core(
        phLogLevel::Debug,
        &format!("Executing command: {}", command),
    );

    // The result is now a specific CommandResult from our commands module.
    // We pass `None` for the repo_path, so commands operate on the current working directory.
    let result = match command {
        // `false` means we want user confirmation for the push.
        "SND" => commands::handle_send(None, &args, false),
        "status" => commands::handle_status(None),
        _ => {
            let err_msg = format!("Unknown command '{}' for git_ops module.", command);
            log_to_core(phLogLevel::Error, &err_msg);
            // Use the GitError variant for unknown commands.
            Err(commands::CommandError::GitError(err_msg))
        }
    };

    // Translate the specific CommandError into a generic phStatus for the C core.
    match result {
        Ok(success_message) => {
            // Log the success message from the command handler.
            log_to_core(phLogLevel::Info, &success_message);
            println!("{}", success_message); // Also print to user stdout
            phStatus::Success
        }
        Err(e) => match e {
            commands::CommandError::GitError(msg) => {
                log_to_core(phLogLevel::Error, &format!("Execution failed: {}", msg));
                println!("Error: {}", msg); // Print error to user stderr
                phStatus::ErrorExecFailed
            }
            commands::CommandError::MissingCommitMessage => {
                let msg = "Missing commit message for 'SND' command. Usage: ph SND <message>";
                log_to_core(phLogLevel::Error, msg);
                println!("{}", msg);
                phStatus::ErrorInvalidArgs
            }
            commands::CommandError::NoUpstreamConfigured => {
                let msg = "Current branch has no upstream configured. Cannot push.\nHint: Use 'git push -u <remote> <branch>' to set the upstream.";
                log_to_core(phLogLevel::Error, msg);
                println!("{}", msg);
                phStatus::ErrorExecFailed
            }
            // These are not "errors" in the sense of failure, but successful "no-op" states.
            commands::CommandError::NoChanges => {
                let msg = "Working tree clean. No changes to send.";
                log_to_core(phLogLevel::Info, msg);
                println!("{}", msg);
                phStatus::Success
            }
            commands::CommandError::OperationAborted => {
                let msg = "Operation aborted by user.";
                log_to_core(phLogLevel::Info, msg);
                println!("{}", msg);
                phStatus::Success
            }
        },
    }
}

#[no_mangle]
pub extern "C" fn module_cleanup() {
    log_to_core(phLogLevel::Info, "git_ops module cleaned up.");
}