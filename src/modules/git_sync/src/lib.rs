/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/git_sync/src/lib.rs
*
* This file is the main entry point for the `git_sync` dynamic library.
* It has been refactored to expose two distinct FFI functions, `run_sync` and
* `run_drift_detector`, to align with the C CLI handlers. It parses the specific
* JSON payload for each action and dispatches to the appropriate logic.
*
* SPDX-License-Identifier: Apache-2.0 */

pub mod config;
pub mod sync;
pub mod drift;
pub mod error;

use config::{DriftConfig, GitOpsPayload, SyncConfig};
use error::{Error, Result};
use std::ffi::{c_char, CStr, CString};
use std::os::raw::c_char;
use std::panic;
use std::ptr;

/// Safely writes a Rust string into a C buffer.
fn write_error_to_buffer(err_msg: String, buf: *mut c_char, buf_len: usize) {
    if buf.is_null() || buf_len == 0 {
        return;
    }
    let c_string = CString::new(err_msg).unwrap_or_else(|_| CString::new("Error message contained null bytes").unwrap());
    let c_bytes = c_string.as_bytes_with_nul();
    let len_to_copy = std::cmp::min(c_bytes.len(), buf_len - 1);
    unsafe {
        ptr::copy_nonoverlapping(c_bytes.as_ptr() as *const c_char, buf, len_to_copy);
        *buf.add(len_to_copy) = 0; // Null-terminate
    }
}

/// Generic internal runner that sets up the environment and calls the appropriate logic.
fn run_internal<F, Fut>(config_json: *const c_char, error_buf: *mut c_char, error_buf_len: usize, action_fn: F) -> i32
where
    F: FnOnce(GitOpsPayload) -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    let result = panic::catch_unwind(|| {
        if config_json.is_null() {
            write_error_to_buffer("Error: Received null pointer for config_json.".to_string(), error_buf, error_buf_len);
            return -1;
        }

        let c_str = unsafe { CStr::from_ptr(config_json) };
        let rust_str = match c_str.to_str() {
            Ok(s) => s,
            Err(e) => {
                write_error_to_buffer(format!("Error: Invalid UTF-8 in config string: {}", e), error_buf, error_buf_len);
                return -2;
            }
        };

        let config: GitOpsPayload = match serde_json::from_str(rust_str) {
            Ok(c) => c,
            Err(e) => {
                write_error_to_buffer(format!("Error: Failed to parse JSON: {}. Received: {}", e, rust_str), error_buf, error_buf_len);
                return -3;
            }
        };

        let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
            Ok(rt) => rt,
            Err(e) => {
                write_error_to_buffer(format!("Error: Failed to build Tokio runtime: {}", e), error_buf, error_buf_len);
                return -4;
            }
        };

        match runtime.block_on(action_fn(config)) {
            Ok(_) => 0, // Success
            Err(e) => {
                write_error_to_buffer(e.to_string(), error_buf, error_buf_len);
                -5 // Generic runtime error
            }
        }
    });

    match result {
        Ok(status_code) => status_code,
        Err(_) => {
            write_error_to_buffer("A panic occurred in the Rust module. This is a critical bug.".to_string(), error_buf, error_buf_len);
            -6 // Panic
        }
    }
}

/// FFI entry point for the `sync` command.
#[no_mangle]
pub extern "C" fn run_sync(config_json: *const c_char, error_buf: *mut c_char, error_buf_len: usize) -> i32 {
    run_internal(config_json, error_buf, error_buf_len, |payload| async {
        if let GitOpsPayload::Sync(config) = payload {
            sync::run(config).await
        } else {
            Err(Error::Other(anyhow::anyhow!("Invalid payload: Expected 'sync' action")))
        }
    })
}

/// FFI entry point for the `drift` command.
#[no_mangle]
pub extern "C" fn run_drift_detector(config_json: *const c_char, error_buf: *mut c_char, error_buf_len: usize) -> i32 {
    run_internal(config_json, error_buf, error_buf_len, |payload| async {
        if let GitOpsPayload::Drift(config) = payload {
            drift::run(config).await
        } else {
            Err(Error::Other(anyhow::anyhow!("Invalid payload: Expected 'drift' action")))
        }
    })
}