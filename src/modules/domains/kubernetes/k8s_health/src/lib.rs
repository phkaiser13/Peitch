/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: lib.rs
*
* This file serves as the FFI (Foreign Function Interface) boundary for the
* `k8s_health` module. Its primary responsibility is to provide a safe and
* stable C-compatible entry point (`run_health_manager`) that the core C
* application can call.
*
* The logic within this file handles:
* 1. Safely reading and interpreting the C string pointer (`config_json`).
* 2. Validating the input for null pointers and UTF-8 encoding.
* 3. Deserializing the JSON payload into strongly-typed Rust structs.
* 4. Setting up and managing the Tokio asynchronous runtime.
* 5. Dispatching the request to the appropriate business logic function in
*    `health_logic.rs` based on the "action" field in the JSON.
* 6. Catching any potential Rust panics to prevent them from unwinding
*    across the C boundary, which would be undefined behavior.
* 7. Translating the `Result` from the business logic into a C-compatible
*    integer status code.
*
* SPDX-License-Identifier: Apache-2.0 */

mod health_logic;

use anyhow::{Context, Result};
use libc::c_char;
use serde::Deserialize;
use std::ffi::CStr;
use std::panic;

// --- FFI Configuration Structures ---

/// Defines the parameters for a 'health check' action.
#[derive(Deserialize, Debug)]
struct HealthCheckParams {
    app: String,
    cluster: String, // Note: 'cluster' is used for context, logging, etc.
}

/// Defines the parameters for an 'autoheal enable' action.
#[derive(Deserialize, Debug)]
struct AutoHealEnableParams {
    trigger: String,
    actions: String,
    cooldown: String,
}

/// An enum representing all possible actions this module can perform.
/// `serde(tag = "action")` makes deserialization robust, routing JSON
/// based on the value of the "action" field.
#[derive(Deserialize, Debug)]
#[serde(tag = "action")]
enum Action {
    #[serde(rename = "check")]
    Check { parameters: HealthCheckParams },
    #[serde(rename = "autoheal_enable")]
    AutoHealEnable {
        parameters: AutoHealEnableParams,
    },
}

// --- FFI Entry Point ---

/// The main FFI entry point for the C core to run health and auto-heal operations.
///
/// # Safety
/// The `config_json` pointer must be a valid, null-terminated C string,
/// or a null pointer. Passing an invalid pointer is undefined behavior.
///
/// # Returns
/// - `0` on success.
/// - `-1` on a null pointer input.
/// - `-2` on a UTF-8 conversion error.
/// - `-3` on a JSON parsing error.
/// - `-4` on a runtime execution error.
/// - `-5` on a panic.
#[no_mangle]
pub extern "C" fn run_health_manager(config_json: *const c_char) -> i32 {
    // Catch panics to prevent unwinding across the FFI boundary.
    let result = panic::catch_unwind(|| {
        if config_json.is_null() {
            eprintln!("[k8s_health] Error: Received a null pointer from C.");
            return -1;
        }

        // Safely convert C string to Rust string slice.
        let c_str = unsafe { CStr::from_ptr(config_json) };
        let rust_str = match c_str.to_str() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[k8s_health] Error: Invalid UTF-8 in config string: {}", e);
                return -2;
            }
        };

        // Deserialize the JSON into our Action enum.
        let action: Action = match serde_json::from_str(rust_str) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[k8s_health] Error: Failed to parse JSON: {}", e);
                return -3;
            }
        };

        // Build and run the Tokio runtime.
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                eprintln!("[k8s_health] Error: Failed to build Tokio runtime: {}", e);
                return -4;
            }
        };

        // Execute the business logic based on the parsed action.
        match runtime.block_on(run_action_internal(action)) {
            Ok(_) => 0, // Success
            Err(e) => {
                // The business logic is expected to print detailed errors.
                // We print the top-level error context here for debugging.
                eprintln!("[k8s_health] Error during execution: {:?}", e);
                -4
            }
        }
    });

    result.unwrap_or(-5) // Return -5 if a panic occurred
}

// --- Core Orchestration Logic ---

/// Internal async function that dispatches to the correct business logic.
async fn run_action_internal(action: Action) -> Result<()> {
    match action {
        Action::Check { parameters } => {
            println!(
                "Performing health check for app '{}' on cluster '{}'...",
                parameters.app, parameters.cluster
            );
            health_logic::perform_checks(parameters)
                .await
                .context("Health check failed")?;
            println!("Health check completed successfully.");
        }
        Action::AutoHealEnable { parameters } => {
            // Placeholder for the auto-heal logic.
            println!(
                "Received 'autoheal enable' request for trigger: {}",
                parameters.trigger
            );
            // In the future, this would call a function in another module:
            // autoheal_logic::configure_rules(parameters).await?;
            println!("'autoheal enable' is not yet fully implemented in Rust.");
        }
    }
    Ok(())
}