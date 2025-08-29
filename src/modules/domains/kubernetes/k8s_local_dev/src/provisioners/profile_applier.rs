/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * File: src/modules/k8s_local_dev/src/profile_applier.rs
 * Logic to apply a profile (a directory of YAML manifests) using kubectl.
 * SPDX-License-Identifier: Apache-2.0
 */

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::process::Command;

// Import the shared execute_command helper from the provisioners module.
// This uses `super::provisioners::common::execute_command` because this file
// is expected to be a sibling of the `provisioners` module in the same module
// tree (i.e. both are declared in main.rs with `mod provisioners; mod profile_applier;`).
use super::provisioners::common::execute_command;

/// Apply all `.yaml`/`.yml` files found (non-recursively) in `profile_path`.
///
/// The function will:
/// 1. Validate `profile_path` exists and is a directory.
/// 2. List entries and filter files ending with `.yml` or `.yaml` (case-insensitive).
/// 3. Sort files alphabetically for deterministic application order.
/// 4. For each file, run: `kubectl apply -f <absolute-path-to-file>` using `execute_command`.
///
/// Note: kubectl is expected to already point to the correct kubeconfig/context.
pub async fn apply_profile(profile_path: &Path) -> Result<()> {
    // Validate path exists and is a directory.
    let meta = fs::metadata(profile_path)
        .await
        .with_context(|| format!("Failed to stat profile path: {}", profile_path.display()))?;

    if !meta.is_dir() {
        anyhow::bail!("Provided profile path is not a directory: {}", profile_path.display());
    }

    // Read directory entries (async).
    let mut entries = fs::read_dir(profile_path)
        .await
        .with_context(|| format!("Failed to read profile directory: {}", profile_path.display()))?;

    let mut files: Vec<PathBuf> = Vec::new();

    while let Some(entry) = entries.next_entry().await.with_context(|| {
        format!("Failed to read an entry in directory: {}", profile_path.display())
    })? {
        let path = entry.path();
        // Only include regular files with extension yml/yaml (case-insensitive).
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext_lc = ext.to_ascii_lowercase();
            if (ext_lc == "yml" || ext_lc == "yaml") && fs::metadata(&path).await?.is_file() {
                files.push(path);
            }
        }
    }

    if files.is_empty() {
        println!(
            "No .yaml/.yml files found in profile path: {}",
            profile_path.display()
        );
        return Ok(());
    }

    // Sort deterministically.
    files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    // Apply each manifest with `kubectl apply -f <file>`.
    for file in files {
        let abs = if file.is_absolute() {
            file.clone()
        } else {
            // Convert to absolute path for clarity in kubectl output.
            std::env::current_dir()
                .with_context(|| "Failed to get current directory")?
                .join(&file)
        };

        println!("Applying manifest: {}", abs.display());

        let mut cmd = Command::new("kubectl");
        cmd.arg("apply").arg("-f").arg(abs);

        execute_command(&mut cmd)
            .await
            .with_context(|| format!("Failed to apply manifest: {}", file.display()))?;
    }

    println!("Profile applied successfully from: {}", profile_path.display());
    Ok(())
}
