/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/k8s_preview/src/lib.rs
*
* This file serves as the main entry point for the `k8s_preview` dynamic
* library. It defines the Foreign Function Interface (FFI) boundary that allows
* the C core to invoke the Rust logic. Its primary responsibility is to safely
* handle data from C, set up the asynchronous Tokio runtime, parse the incoming
* JSON configuration, dispatch to the correct business logic function based on
* the requested action, and report a status code back to the C caller.
*
* SPDX-License-Identifier: Apache-2.0 */

// Public modules that define the library's structure.
pub mod actions;
pub mod config;
pub mod crd;
pub mod kube_client;

use config::{Action, PreviewConfig};
use std::ffi::{c_char, CStr};
use std::panic;

/// The main FFI entry point to be called from the C core.
///
/// This function accepts a JSON configuration string from the C caller,
/// parses it, and executes the corresponding action (create, destroy, status,
/// logs, exec, extend, or gc). It orchestrates the entire lifecycle of a
/// command execution within this module.
///
/// # Safety
/// The `config_json` pointer must be a valid, null-terminated C string,
/// and it must remain valid for the duration of this function call. The C
/// core is responsible for managing the memory of this string.
///
/// # Returns
/// - `0` on success.
/// - `-1` on a null pointer input.
/// - `-2` on a UTF-8 conversion error.
/// - `-3` on a JSON parsing error.
/// - `-4` on a runtime execution error.
/// - `-5` on a panic within the Rust code (a critical bug).
#[no_mangle]
pub extern "C" fn run_preview_manager(config_json: *const c_char) -> i32 {
    // Set up a panic hook to catch any unexpected panics and prevent them
    // from unwinding across the FFI boundary, which is undefined behavior.
    let result = panic::catch_unwind(|| {
        // 1. Safely handle the C string pointer.
        if config_json.is_null() {
            eprintln!("[k8s_preview_module] Error: Received a null pointer for config_json.");
            return -1;
        }

        let c_str = unsafe { CStr::from_ptr(config_json) };
        let rust_str = match c_str.to_str() {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "[k8s_preview_module] Error: Failed to convert C string to Rust string: {}",
                    e
                );
                return -2;
            }
        };

        // 2. Parse the JSON configuration into our strongly-typed struct.
        let config: PreviewConfig = match serde_json::from_str(rust_str) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[k8s_preview_module] Error: Failed to parse JSON config: {}", e);
                // For debugging, print the invalid JSON.
                eprintln!("[k8s_preview_module] Received JSON: {}", rust_str);
                return -3;
            }
        };

        // 3. Create a Tokio runtime to execute our async logic.
        let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
            Ok(rt) => rt,
            Err(e) => {
                eprintln!("[k8s_preview_module] Error: Failed to build Tokio runtime: {}", e);
                return -4;
            }
        };

        // 4. Execute the business logic within the async runtime by dispatching
        //    to the appropriate handler based on the parsed action.
        let execution_result = runtime.block_on(async {
            match config.action {
                Action::Create => actions::handle_create_action(&config).await,
                Action::Destroy => actions::handle_destroy_action(&config).await,
                Action::Status => actions::handle_status_action(&config).await,
                Action::Logs => actions::handle_logs_action(&config).await,
                Action::Exec => actions::handle_exec_action(&config).await,
                Action::Extend => actions::handle_extend_action(&config).await,
                Action::Gc => actions::handle_gc_action(&config).await,
            }
        });

        // 5. Report the final status back to the C caller.
        match execution_result {
            Ok(_) => {
                // A success message is usually printed by the action handler itself.
                0 // Success
            }
            Err(e) => {
                // Using `anyhow`'s chain, we can print the full error context.
                eprintln!("[k8s_preview_module] Error during execution: {:?}", e);
                -4 // Runtime error
            }
        }
    });

    match result {
        Ok(status_code) => status_code,
        Err(_) => {
            eprintln!("[k8s_preview_module] Error: A panic occurred. This is a critical bug.");
            -5 // Panic
        }
    }
}