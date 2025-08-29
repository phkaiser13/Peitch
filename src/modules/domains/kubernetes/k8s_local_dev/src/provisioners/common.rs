/*
 * Copyright (C) 2025 Pedro Henrique / phkaiser13
 *
 * File: src/modules/k8s_local_dev/src/provisioners/common.rs
 *
 * This file provides a shared, robust utility for executing external command-line
 * tools asynchronously. It is the cornerstone of all provisioner modules (`kind`, `k3s`,
 * etc.), ensuring that interactions with underlying CLIs are handled consistently,
 * efficiently, and with excellent user feedback.
 *
 * Architecture:
 * The core of this module is the `execute_command` function. Its design is centered
 * on providing a seamless user experience, making it appear as though the user is
 * running the external command directly in their terminal.
 *
 * Key architectural principles are:
 * 1. Asynchronous Process Management: It utilizes `tokio::process::Command` to spawn
 * child processes without blocking the main application thread. This is essential for
 * a responsive CLI, especially for long-running tasks like creating a Kubernetes cluster.
 * 2. Real-time I/O Streaming: Instead of waiting for the child process to complete
 * before showing any output, this function captures the stdout and stderr streams of the
 * child process and pipes them to the parent's stdout and stderr in real-time. This is
 * achieved by spawning two dedicated asynchronous tasks that handle the I/O copying
 * concurrently with the process execution.
 * 3. Centralized Error Handling: The function meticulously checks for errors at every
 * stage: failure to spawn the process, failure to capture I/O streams, and, most
 * importantly, a non-zero exit status from the child process. All errors are wrapped
 * in `anyhow::Result` to provide clear, contextual messages, which are then propagated
 * to the caller.
 * 4. Reusability: By encapsulating this complex logic in a single, reusable function,
 * we avoid code duplication across different provisioner modules and ensure that any
 * improvements to process handling benefit all provisioners simultaneously.
 *
 * This utility is fundamental to the modularity and robustness of the `k8s_local_dev`
 * tool, abstracting away the intricate details of process management from the
 * high-level logic of each specific provisioner.
 *
 * SPDX-License-Identifier: Apache-2.0
 */

use anyhow::{bail, Context, Result};
use std::process::Stdio;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};

/// Executes a given command asynchronously, streaming its stdout and stderr to the parent.
///
/// This function is the workhorse for all provisioner modules. It takes a configured
/// `tokio::process::Command`, executes it, and ensures that all output from the command
'/// is immediately visible to the user. It waits for the command to complete and returns
/// an error if the command exits with a non-successful status code.
///
/// # Arguments
/// * `command` - A mutable reference to a `tokio::process::Command` ready to be spawned.
///
/// # Returns
/// A `Result<()>` which is `Ok(())` on success or an `anyhow::Error` if the command
/// fails to spawn or exits with an error.
pub async fn execute_command(command: &mut Command) -> Result<()> {
    // 1. Configure the command to capture stdout and stderr.
    // `Stdio::piped()` creates a pipe that we can read from in our parent process.
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    // 2. Spawn the child process.
    // `spawn()` is non-blocking and returns immediately with a handle to the child process.
    let mut child = command
        .spawn()
        .context("Failed to spawn command. Is the required tool (e.g., 'kind', 'k3d') installed and in your PATH?")?;

    // 3. Concurrently stream stdout and stderr from the child to the parent.
    // We must handle both streams simultaneously to avoid deadlocks, which can occur if
    // the child process fills its stdout or stderr buffer and waits for the parent to
    // read it, while the parent is waiting for the other stream or for the process to exit.

    let stdout = child.stdout.take().context("Failed to capture stdout from child process.")?;
    let stderr = child.stderr.take().context("Failed to capture stderr from child process.")?;

    // Create buffered readers for efficient line-by-line processing.
    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    // Use a non-blocking loop with `tokio::select!` to process lines from either
    // stdout or stderr as they become available.
    loop {
        tokio::select! {
            // Biased select ensures we check for completion first.
            biased;

            // Check if the child process has exited.
            status = child.wait() => {
                let status = status.context("Failed to wait for child process.")?;
                // After the process exits, we might still have buffered lines to read.
                // Drain the remaining lines from both stdout and stderr.
                while let Some(line) = stdout_reader.next_line().await? {
                    println!("{}", line);
                }
                while let Some(line) = stderr_reader.next_line().await? {
                    eprintln!("{}", line);
                }

                // Finally, check the exit status.
                if !status.success() {
                    bail!("Command executed with a non-zero exit status: {}", status);
                }
                return Ok(());
            },

            // Process the next line from stdout.
            result = stdout_reader.next_line() => {
                match result {
                    Ok(Some(line)) => println!("{}", line),
                    // `Ok(None)` means the stream has closed. We do nothing and let the `child.wait()` branch handle termination.
                    Ok(None) => {},
                    Err(e) => eprintln!("Error reading stdout: {}", e),
                }
            },

            // Process the next line from stderr.
            result = stderr_reader.next_line() => {
                 match result {
                    Ok(Some(line)) => eprintln!("{}", line),
                    Ok(None) => {},
                    Err(e) => eprintln!("Error reading stderr: {}", e),
                }
            },
        }
    }
}