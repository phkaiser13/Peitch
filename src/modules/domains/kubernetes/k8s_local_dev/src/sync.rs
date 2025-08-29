/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * File: src/modules/k8s_local_dev/src/sync.rs
 *
 * This file implements the logic for the `ph local sync` command, which provides
 * a fast inner-loop development cycle for Kubernetes. It orchestrates a sequence
 * of common development tasks: building a container image, pushing it to a local
 * registry, and deploying Kubernetes manifests.
 *
 * The core of this module is the `run_command` helper, which uses `tokio` to
 * execute external processes (`docker`, `kubectl`) and streams their output
 * directly to the user's terminal in real-time. This provides immediate
 * feedback, which is crucial for a smooth developer experience.
 *
 * SPDX-License-Identifier: Apache-2.0 */

use crate::cli::SyncArgs;
use anyhow::{anyhow, Context, Result};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// Executes a shell command, streams its output to stdout/stderr, and waits for completion.
async fn run_command(mut command: Command) -> Result<()> {
    // Configure the command to capture stdout and stderr
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    // Spawn the child process
    let mut child = command.spawn().context("Failed to spawn command")?;

    // Take ownership of stdout and stderr
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("Child process did not have a handle to stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("Child process did not have a handle to stderr"))?;

    // Create buffered readers for both streams
    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    // Asynchronously read from both streams and print lines as they come in
    loop {
        tokio::select! {
            line = stdout_reader.next_line() => {
                if let Some(line) = line? {
                    println!("{}", line);
                } else {
                    // Stream is closed
                }
            },
            line = stderr_reader.next_line() => {
                if let Some(line) = line? {
                    eprintln!("{}", line);
                } else {
                    // Stream is closed
                }
            },
            // The loop will break when both streams are closed and `wait` completes.
            else => break,
        }
    }

    // Wait for the command to finish and check its exit status
    let status = child.wait().await.context("Failed to wait for command to complete")?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("Command exited with non-zero status: {}", status))
    }
}

/// The main handler for the `sync` command.
pub async fn handle_sync(args: SyncArgs) -> Result<()> {
    // --- Step 1: Build the container image ---
    println!("\n--- Step 1: Building container image: {} ---", &args.image);
    let mut build_cmd = Command::new("docker");
    build_cmd.args(["build", ".", "-t", &args.image]);
    run_command(build_cmd).await.context("Failed to build container image")?;
    println!("✅ Image build successful.");

    // --- Step 2: Push the image to a local registry ---
    // This assumes the user has a local registry running (e.g., via `kind` or `k3d`).
    println!("\n--- Step 2: Pushing image to local registry: {} ---", &args.image);
    let mut push_cmd = Command::new("docker");
    push_cmd.args(["push", &args.image]);
    run_command(push_cmd).await.context("Failed to push image to local registry. Is a local registry running and accessible?")?;
    println!("✅ Image push successful.");

    // --- Step 3: Deploy Kubernetes manifests ---
    println!("\n--- Step 3: Applying manifests from: {} ---", args.path.display());
    let mut apply_cmd = Command::new("kubectl");
    // Using -k for Kustomize is generally preferred for directory-based manifests.
    apply_cmd.args(["apply", "-k", &args.path.to_string_lossy()]);
    run_command(apply_cmd).await.context("Failed to apply Kubernetes manifests")?;
    println!("✅ Manifests applied successfully.");

    Ok(())
}
