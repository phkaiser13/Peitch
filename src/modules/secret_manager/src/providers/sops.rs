/*
 * Copyright (C) 2025 Pedro Henrique / phkaiser13
 *
 * File: src/modules/secret_manager/src/providers/sops.rs
 *
 * This file provides a client for interacting with Mozilla SOPS (Secrets OPerationS).
 * The primary function of this module is to decrypt files that have been encrypted with
 * SOPS, enabling the application to access secrets stored securely in a Git repository.
 * The implementation operates as a robust wrapper around the `sops` command-line tool.
 *
 * Architecture:
 * The design philosophy is to leverage the battle-tested `sops` binary directly rather
 * than reimplementing its cryptographic and cloud KMS integrations. This approach
 * significantly reduces complexity and enhances security by relying on the canonical
 * implementation.
 *
 * Key components of the architecture:
 * 1. Asynchronous Process Execution: The module uses `tokio::process::Command` to
 * execute the `sops` binary as a child process. This is done asynchronously to prevent
 * blocking the application's main event loop, which is critical for a responsive system
 * that may perform multiple I/O operations concurrently.
 * 2. Robust I/O and Error Handling: The standard output and standard error of the `sops`
 * process are captured. In case of a decryption failure, the error message from `sops`
 * is captured and propagated as a rich, contextual error via `anyhow::Result`. This
 * provides clear, actionable feedback for debugging.
 * 3. Data Deserialization: After successful decryption, the output (which is typically
 * YAML or JSON) is deserialized into a structured Rust type. For maximum flexibility,
 * this module decodes the decrypted content into `serde_yaml::Value`, which can
 * represent any valid YAML structure. The caller can then extract the specific values it
 * needs from this generic structure.
 * 4. Clear and Focused API: A single public function, `decrypt_and_parse`, serves as
 * the entry point. It takes the path to the SOPS-encrypted file and returns the parsed
 * content, abstracting away all the details of process management and data parsing.
 *
 * Process Flow for decrypting a file:
 * - The `decrypt_and_parse` function is called with a path to an encrypted file.
 * - It first checks if the `sops` executable is available in the system's PATH.
 * - A new child process is spawned to execute `sops --decrypt <file_path>`.
 * - The function waits for the process to complete, capturing its exit status and output.
 * - If the process exits with an error, the captured stderr is used to create a detailed
 * error message.
 * - If the process succeeds, the captured stdout (the decrypted file content) is parsed
 * as YAML into a `serde_yaml::Value`.
 * - The parsed value is returned to the caller for further processing.
 *
 * SPDX-License-Identifier: Apache-2.0
 */

use anyhow::{bail, Context, Result};
use serde_yaml::Value;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

/// Decrypts a SOPS-encrypted file and parses its content as a YAML value.
///
/// This function executes the `sops --decrypt` command on the given file path.
/// It captures the decrypted output (stdout) and parses it into a generic
/// `serde_yaml::Value`, which allows the caller to handle arbitrarily structured
/// YAML data (maps, sequences, scalars).
///
/// # Arguments
/// * `file_path` - A reference to the path of the SOPS-encrypted file.
///
/// # Returns
/// A `Result` containing the parsed `serde_yaml::Value` on success.
/// Returns an error if the `sops` command is not found, if the decryption fails,
/// or if the output cannot be parsed as YAML.
///
/// # Example
/// ```rust,no_run
/// # use anyhow::Result;
/// # use std::path::Path;
/// # use crate::providers::sops::decrypt_and_parse;
/// #
/// #[tokio::main]
/// async fn main() -> Result<()> {
///     let secret_file = Path::new("secrets.sops.yaml");
///     match decrypt_and_parse(secret_file).await {
///         Ok(secrets) => {
///             if let Some(api_key) = secrets.get("api_key") {
///                 println!("Successfully decrypted API key.");
///             }
///         }
///         Err(e) => {
///             eprintln!("Failed to decrypt secrets: {:?}", e);
///         }
///     }
///     Ok(())
/// }
/// ```
pub async fn decrypt_and_parse(file_path: &Path) -> Result<Value> {
    println!("Attempting to decrypt SOPS file: {}", file_path.display());

    // 1. Construct the command to run `sops` for decryption.
    // We configure the command to pipe its stdout and stderr so we can capture them.
    let mut cmd = Command::new("sops");
    cmd.arg("--decrypt")
        .arg(file_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // 2. Spawn the child process to execute the command.
    let child = cmd.spawn().with_context(|| {
        format!("Failed to spawn 'sops' command. Is SOPS installed and in your PATH?")
    })?;

    // 3. Wait for the process to finish and capture its output.
    // `output()` waits for the child to exit and collects all stdout and stderr.
    let output = child.await?;

    // 4. Check the exit status of the process.
    if !output.status.success() {
        // If the command failed, we construct a detailed error message using the
        // captured stderr, which provides valuable context for why `sops` failed.
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "SOPS decryption failed with status {}:\n{}",
            output.status,
            stderr
        );
    }

    // 5. If successful, parse the decrypted stdout as YAML.
    // The decrypted content is expected to be valid YAML (or JSON, which is a subset).
    let decrypted_content = &output.stdout;
    let secrets: Value = serde_yaml::from_slice(decrypted_content)
        .with_context(|| {
            let content_preview = String::from_utf8_lossy(decrypted_content);
            format!("Failed to parse decrypted SOPS output as YAML. Output preview:\n{}", content_preview.chars().take(200).collect::<String>())
        })?;

    println!("Successfully decrypted and parsed SOPS file: {}", file_path.display());
    Ok(secrets)
}