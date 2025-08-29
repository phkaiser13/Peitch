/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* src/process_wrapper.rs - Safe wrappers for executing external processes.
*
* This file provides a toolbox of helper functions for running external
* command-line tools like `terraform` and `vault`. It abstracts the details
* of `std::process::Command` to offer two distinct execution strategies:
*
* 1. Streaming: For long-running or interactive commands where real-time
*    output is desired. The child process inherits the parent's stdio.
* 2. Capturing: For commands that produce structured data (like JSON) on
*    stdout, which needs to be captured and parsed by the application.
*
* Error handling is designed to be robust, returning detailed messages if a
* command is not found or if it exits with a non-zero status code.
*
* SPDX-License-Identifier: Apache-2.0 */

use std::process::{Command, Stdio};

/// A flexible error type for command execution failures.
type CommandResult<T> = Result<T, Box<dyn std::error::Error>>;

/// Executes a command and streams its stdout/stderr directly to the parent process.
/// This is ideal for long-running, interactive commands like `terraform plan`.
pub fn run_command_with_streaming(command_name: &str, args: &[String]) -> CommandResult<()> {
    println!("> Executing: {} {}", command_name, args.join(" "));

    let mut command = Command::new(command_name);
    command.args(args);

    // The child process will inherit the stdin, stdout, and stderr of the parent.
    // This is the most efficient way to stream output.
    let status = command.status()
        .map_err(|e| format!("Failed to execute '{}'. Is it in your PATH? Error: {}", command_name, e))?;

    if !status.success() {
        // The command's own error messages will have already been printed to stderr.
        return Err(format!("Command '{}' failed with exit code: {}", command_name, status).into());
    }

    Ok(())
}

/// Executes a command and captures its stdout for parsing. Stderr is also captured
/// to be included in error messages. This is ideal for commands that return data.
pub fn run_command_and_capture(command_name: &str, args: &[String]) -> CommandResult<String> {
    println!("> Capturing: {} {}", command_name, args.join(" "));

    let mut command = Command::new(command_name);
    command.args(args);

    // We want to capture the output, so we configure pipes for stdout and stderr.
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let output = command.output()
        .map_err(|e| format!("Failed to execute '{}'. Is it in your PATH? Error: {}", command_name, e))?;

    if !output.status.success() {
        // If the command fails, we return its stderr as the error message for context.
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Command '{}' failed: {}", command_name, stderr).into());
    }

    // If successful, we return its stdout as a String.
    let stdout = String::from_utf8(output.stdout)?;
    Ok(stdout)
}