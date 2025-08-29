/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* src/commands.rs - Implementation of the specific DevOps commands.
*
* This file contains the business logic for each command supported by the
* module, such as `tf-plan` and `vault-read`. It uses the helper functions
* from `process_wrapper.rs` to execute external tools.
*
* For commands that return structured data (like `vault-read`), this is also
* where the data parsing (e.g., JSON deserialization) occurs. This separation
* of concerns makes the code easier to maintain and test.
*
* SPDX-License-Identifier: Apache-2.0 */

use crate::process_wrapper::{run_command_and_capture, run_command_with_streaming};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

/// A flexible error type for command logic failures.
type CommandResult<T> = Result<T, Box<dyn std::error::Error>>;

// --- Terraform Handlers ---

pub fn handle_terraform_plan(args: &[String]) -> CommandResult<()> {
    println!("--- Running Terraform Plan ---");
    let mut full_args = vec!["plan".to_string()];
    full_args.extend_from_slice(args);
    run_command_with_streaming("terraform", &full_args)
}

pub fn handle_terraform_apply(args: &[String]) -> CommandResult<()> {
    println!("--- Running Terraform Apply ---");
    let mut full_args = vec!["apply".to_string()];
    full_args.extend_from_slice(args);
    run_command_with_streaming("terraform", &full_args)
}

// --- Vault Handlers ---

/// Defines the structure of the JSON output from `vault read`.
/// We use `serde_json::Value` to handle arbitrary data types in the secret.
#[derive(Deserialize)]
struct VaultSecretData {
    data: HashMap<String, Value>,
}

#[derive(Deserialize)]
struct VaultSecretResponse {
    data: VaultSecretData,
}

pub fn handle_vault_read(args: &[String]) -> CommandResult<()> {
    if args.len() != 1 {
        return Err("Expected exactly one argument: <secret_path>".into());
    }
    let secret_path = &args[0];
    println!("--- Reading secret from Vault: {} ---", secret_path);

    // Construct the arguments for the `vault` command.
    let vault_args = &["read".to_string(), "-format=json".to_string(), secret_path.clone()];

    // Capture the JSON output from the vault CLI.
    let json_output = run_command_and_capture("vault", vault_args)?;

    // Parse the JSON string into our Rust structs.
    let secret_response: VaultSecretResponse = serde_json::from_str(&json_output)?;

    let secrets = secret_response.data.data;
    if secrets.is_empty() {
        println!("No data found at the specified secret path.");
    } else {
        println!("Secrets found:");
        for (key, value) in secrets {
            // `serde_json::Value` has a useful `to_string` implementation.
            println!("  - {}: {}", key, value);
        }
    }
    println!("---------------------------------");
    Ok(())
}