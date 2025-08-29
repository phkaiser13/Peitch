/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* src/lib.rs - YAML workflow parser for the ci_cd_manager module.
*
* This Rust library is a drop-in replacement for the original Go parser.
* It exposes a single C-compatible function, `ParseWorkflowToJSON`, which
* reads a workflow file (e.g., from GitHub Actions), parses its YAML content,
* and serializes it into a JSON string.
*
* The returned JSON string is allocated by Rust and its ownership is
* transferred to the C/C++ caller. A corresponding `FreeJSONString` function
* is provided to allow the caller to safely release the memory, which is a
* critical pattern for robust FFI design.
*
* SPDX-License-Identifier: Apache-2.0 */

use libc::c_char;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::fs;
use std::ptr;

// --- Data Structures for YAML Parsing and JSON Serialization ---

/// Represents a single step within a CI/CD job.
/// Derives `Serialize` for JSON output and `Deserialize` for YAML input.
#[derive(Serialize, Deserialize, Debug)]
struct WorkflowStep {
    name: String,
    run: String,
}

/// Represents the structure of a job as defined in the YAML file.
/// Note that this struct does not contain the job's name, as the name
/// is the key in the `jobs` map in the source YAML.
#[derive(Deserialize, Debug)]
struct YamlWorkflowJob {
    #[serde(rename = "runs-on")]
    runs_on: String,
    steps: Vec<WorkflowStep>,
}

/// Represents the top-level structure of the YAML workflow file.
#[derive(Deserialize, Debug)]
struct YamlWorkflow {
    name: String,
    jobs: HashMap<String, YamlWorkflowJob>,
}

/// Represents a job for the final JSON output.
/// This struct *includes* the job's name, which we will populate
/// after parsing the YAML. This makes the final JSON more convenient to consume.
#[derive(Serialize, Debug)]
struct JsonWorkflowJob {
    name: String,
    #[serde(rename = "runs_on")]
    runs_on: String,
    steps: Vec<WorkflowStep>,
}

/// Represents the final, top-level structure to be serialized to JSON.
#[derive(Serialize, Debug)]
struct JsonWorkflow {
    name: String,
    // We convert the map of jobs into a simple array for easier iteration
    // on the consumer side (e.g., in C++ or JavaScript).
    jobs: Vec<JsonWorkflowJob>,
}

/// Internal function to handle the core logic, separating it from unsafe FFI code.
/// This promotes testability and clarity.
fn parse_and_serialize(filepath: &str) -> Result<String, Box<dyn std::error::Error>> {
    // 1. Read the YAML file content into a string.
    let yaml_content = fs::read_to_string(filepath)?;

    // 2. Deserialize the YAML string into our intermediate `YamlWorkflow` struct.
    // `serde_yaml` handles the parsing efficiently.
    let yaml_workflow: YamlWorkflow = serde_yaml::from_str(&yaml_content)?;

    // 3. Transform the parsed YAML structure into the target JSON structure.
    // This is where we handle the business logic of moving the job name from
    // the map key into the job object itself.
    let json_jobs: Vec<JsonWorkflowJob> = yaml_workflow
        .jobs
        .into_iter()
        .map(|(job_name, yaml_job)| JsonWorkflowJob {
            name: job_name,
            runs_on: yaml_job.runs_on,
            steps: yaml_job.steps,
        })
        .collect();

    let json_workflow = JsonWorkflow {
        name: yaml_workflow.name,
        jobs: json_jobs,
    };

    // 4. Serialize the final `JsonWorkflow` struct into a pretty-printed JSON string.
    let json_string = serde_json::to_string_pretty(&json_workflow)?;

    Ok(json_string)
}

/// Parses a YAML workflow file and returns it as a JSON string.
///
/// # Contract
/// The returned `*mut c_char` points to memory allocated by Rust.
/// It is the caller's responsibility to free this memory by calling
/// `FreeJSONString`. A NULL pointer is returned on error.
///
/// # Safety
/// This function is `unsafe` because it dereferences a raw C pointer `filepath`.
/// The caller must ensure that `filepath` is a valid, null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn ParseWorkflowToJSON(filepath: *const c_char) -> *mut c_char {
    // Ensure the input pointer is not null.
    if filepath.is_null() {
        eprintln!("Error: ParseWorkflowToJSON received a NULL filepath.");
        return ptr::null_mut();
    }

    // Safely convert the C string to a Rust string slice.
    let c_str = CStr::from_ptr(filepath);
    let rust_filepath = match c_str.to_str() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: Filepath is not valid UTF-8: {}", e);
            return ptr::null_mut();
        }
    };

    // Call the safe, internal logic function.
    match parse_and_serialize(rust_filepath) {
        Ok(json_string) => {
            // Convert the Rust String into a C-compatible string.
            // `into_raw` transfers ownership of the memory to the C caller.
            match CString::new(json_string) {
                Ok(c_string) => c_string.into_raw(),
                Err(_) => ptr::null_mut(), // Should not happen if json_string is valid
            }
        }
        Err(e) => {
            // As in the Go version, print the error and return null.
            eprintln!("Error processing workflow file '{}': {}", rust_filepath, e);
            ptr::null_mut()
        }
    }
}

/// Frees the memory for a C string that was allocated by Rust.
///
/// # Contract
/// This function should ONLY be called with a pointer that was returned
/// by `ParseWorkflowToJSON`.
///
/// # Safety
/// This function is `unsafe` because it takes ownership of a raw pointer
/// and will deallocate it. Calling it with an invalid pointer (or calling
/// it more than once on the same pointer) will lead to undefined behavior.
#[no_mangle]
pub unsafe extern "C" fn FreeJSONString(s: *mut c_char) {
    if !s.is_null() {
        // `CString::from_raw` takes ownership of the pointer and will
        // correctly deallocate the memory when it goes out of scope at the
        // end of this line. This is the safe and correct way to free memory
        // across the FFI boundary.
        let _ = CString::from_raw(s);
    }
}