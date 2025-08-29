/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/git_sync/src/sync.rs
*
* This file contains the business logic for the `sync` command. It takes the
* parsed configuration, initializes a Kubernetes client, and orchestrates the
* application of manifests from a local path to the target cluster.
*
* SPDX-License-Identifier: Apache-2.0 */

use crate::config::SyncConfig;
use crate::error::{Error, Result};
use signature_verifier::verify_commit_signature;

pub async fn run(config: SyncConfig) -> Result<()> {
    println!("🚀 Starting 'sync' operation...");
    println!("   - Manifests Path: {}", config.path);

    // Signature Verification Step
    if !config.skip_signature_verification {
        println!("🔒 Verifying commit signature for repository at '{}'...", config.path);
        verify_commit_signature(&config.path)
            .map_err(|e| Error::SignatureVerificationError(e.to_string()))?;
        println!("✅ Commit signature verified.");
    } else {
        println!("⚠️ WARNING: Skipping commit signature verification due to --skip-signature-verification flag.");
    }

    println!("\n✅ Git-related tasks complete. Handing off to orchestrator for platform-specific apply.");

    Ok(())
}
