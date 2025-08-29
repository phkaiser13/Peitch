/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/policy_engine/src/lib.rs
*
* This file is the entry point for the 'policy_engine' Rust library. It exposes
* a single function, `run_policy_engine`, which is C-ABI compatible and serves as
* the Foreign Function Interface (FFI) for the C application core.
*
* The main logic here is:
* 1. Receive a C character pointer (`*const c_char`) containing a JSON payload.
* 2. Safely convert this C pointer into a Rust string.
* 3. Deserialize the JSON string into strongly-typed Rust structs using Serde.
*    This ensures the payload is well-formed before proceeding.
* 4. Start a Tokio asynchronous runtime, as interactions with Kubernetes
*    are asynchronous I/O operations.
* 5. Dispatch the action (scan, apply, test) to the appropriate asynchronous handler function,
*    which in turn invokes the logic in the submodules (`gatekeeper`, `preview_testing`).
* 6. Catch any errors that occur during the process, print them to stderr
*    for diagnostics, and translate the result (`Ok` or `Err`) into an
*    integer exit code that C can understand (0 for success, 1 for failure).
*
* SPDX-License-Identifier: Apache-2.0 */

use anyhow::{anyhow, Context, Result};
use libc::c_char;
use serde::Deserialize;
use std::ffi::CStr;
use std::fs;
use std::str;
use tempfile::Builder;
use tokio::process::Command;

// --- Submodule Declarations ---
mod gatekeeper;
mod preview_testing;

// --- Data Models for JSON Deserialization ---

/// Represents the complete structure of the JSON payload received from C.
#[derive(Deserialize, Debug)]
#[serde(tag = "action", content = "parameters", rename_all = "lowercase")]
enum EnginePayload {
    Scan(ScanParameters),
    Apply(ApplyParameters),
    Test(TestParameters),
}

/// Parameters for the 'scan' action.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ScanParameters {
    manifest_path: String,
    policy_repo_path: String,
    fail_on_violation: bool,
}

/// Parameters for the 'apply' action.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ApplyParameters {
    mode: String,
    cluster_name: Option<String>,
    policy_repo_path: Option<String>,
}

/// Parameters for the 'test' action.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct TestParameters {
    pr_number: u32,
    policy_repo_path: Option<String>,
}

// --- Main Logic and Action Handlers ---

/// Handler for the 'apply' action.
/// Dispatches execution to the `gatekeeper` module.
async fn handle_apply(params: ApplyParameters) -> Result<()> {
    println!("[RUST] 'apply' action received. Dispatching to Gatekeeper module.");
    gatekeeper::apply_policies(
        &params.policy_repo_path,
        params.cluster_name.as_deref(), // Converts Option<String> to Option<&str>
    )
    .await?;
    println!("[RUST] Gatekeeper module completed execution successfully.");
    Ok(())
}

/// Handler for the 'test' action.
/// Dispatches execution to the `preview_testing` module.
async fn handle_test(params: TestParameters) -> Result<()> {
    println!("[RUST] 'test' action received. Dispatching to Preview Testing module.");
    preview_testing::test_preview_policies(&params.policy_repo_path, params.pr_number).await?;
    println!("[RUST] Preview Testing module completed execution successfully.");
    Ok(())
}

/// Handler for the 'scan' action.
/// Invokes the `conftest` tool as a subprocess to execute
/// Rego policies against local manifest files.
async fn handle_scan(params: ScanParameters) -> Result<()> {
    println!("[RUST] 'scan' action received. Running 'conftest' locally.");
    println!("[RUST]   - Manifest Path: {}", params.manifest_path);
    println!("[RUST]   - Policy Repository: {}", params.policy_repo_path);

    let output = Command::new("conftest")
        .arg("test")
        .arg("--policy")
        .arg(&params.policy_repo_path)
        .arg(&params.manifest_path)
        .output()
        .await
        .context("Failed to execute 'conftest' process. Check if it is installed and in the system's PATH.")?;

    let stdout = str::from_utf8(&output.stdout)?;
    let stderr = str::from_utf8(&output.stderr)?;

    if output.status.success() {
        println!("[RUST] ✅ Local check with conftest passed successfully!");
        if !stdout.trim().is_empty() {
            println!("--- Conftest Output ---\n{}\n-----------------------", stdout);
        }
        Ok(())
    } else {
        let full_output = format!(
            "--- Standard Output (stdout) ---\n{}\n--- Standard Error (stderr) ---\n{}",
            stdout, stderr
        );
        Err(anyhow!(
            "❌ Policy violations detected by conftest in local check.\n\n{}",
            full_output
        ))
    }
}

/// Internal function that contains the main logic.
/// This separates the unsafe FFI code from the safe Rust code.
fn run_internal(config_json_str: &str) -> Result<()> {
    let payload: EnginePayload = serde_json::from_str(config_json_str)
        .context("Failed to deserialize the JSON payload from C.")?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("Failed to build the Tokio runtime.")?;

    rt.block_on(async {
        match payload {
            EnginePayload::Scan(params) => handle_scan(params).await,
            EnginePayload::Apply(params) => handle_apply(params).await,
            EnginePayload::Test(params) => handle_test(params).await,
        }
    })
}

// --- FFI Entry Point ---

/// This is the exported function that will be called by the C code.
#[no_mangle]
pub extern "C" fn run_policy_engine(config_json: *const c_char) -> i32 {
    if config_json.is_null() {
        eprintln!("[RUST FFI ERROR] Received a null pointer from C.");
        return -1;
    }

    let c_str = unsafe { CStr::from_ptr(config_json) };

    match c_str.to_str() {
        Ok(json_str) => match run_internal(json_str) {
            Ok(_) => 0, // Success
            Err(e) => {
                // Prints the full error chain provided by `anyhow`.
                eprintln!("[RUST ENGINE ERROR] The operation failed:\n{:#}", e);
                1 // Generic failure
            }
        },
        Err(e) => {
            eprintln!("[RUST FFI ERROR] Failed to convert C string (invalid UTF-8?): {:?}", e);
            2
        }
    }
}

/// Validates a string of Kubernetes manifests against a string of Rego policies.
///
/// This function creates temporary files for the manifests and policies, invokes
/// `conftest` to run the validation, and returns the result.
///
/// # Arguments
/// * `manifests` - A string containing one or more YAML documents of Kubernetes resources.
/// * `policies` - A string containing one or more Rego policy documents.
///
/// # Returns
/// A `Result` which is `Ok(())` on success or an `Err` containing the `conftest`
/// output if any policy violations are found.
pub async fn validate_manifests(manifests: &str, policies: &str) -> Result<()> {
    // 1. Create a temporary directory for the policies.
    let policy_dir = Builder::new().prefix("ph-policies").tempdir()?;
    let policy_file_path = policy_dir.path().join("policy.rego");
    fs::write(&policy_file_path, policies)?;

    // 2. Create a temporary file for the manifests.
    let manifest_file = Builder::new().prefix("ph-manifest").suffix(".yaml").tempfile()?;
    fs::write(manifest_file.path(), manifests)?;

    println!("[policy_engine] Running 'conftest' validation...");
    println!("[policy_engine]   - Manifests at: {}", manifest_file.path().display());
    println!("[policy_engine]   - Policies at: {}", policy_dir.path().display());

    // 3. Execute conftest.
    let output = Command::new("conftest")
        .arg("test")
        .arg("--policy")
        .arg(policy_dir.path())
        .arg(manifest_file.path())
        .output()
        .await
        .context("Failed to execute 'conftest' process. Is it installed and in the system's PATH?")?;

    let stdout = str::from_utf8(&output.stdout)?;
    let stderr = str::from_utf8(&output.stderr)?;

    if output.status.success() {
        println!("[policy_engine] ✅ Manifests passed policy validation.");
        Ok(())
    } else {
        let full_output = format!(
            "Policy validation failed.\n--- stdout ---\n{}\n--- stderr ---\n{}",
            stdout, stderr
        );
        Err(anyhow!(full_output))
    }
}