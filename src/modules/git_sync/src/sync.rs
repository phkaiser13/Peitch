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
use kube::{
    api::{Api, ObjectMeta, Patch, PatchParams},
    Client, CustomResource,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use signature_verifier::verify_commit_signature;

// --- CRD Struct Definition for PhgitSyncJob ---
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "ph.io",
    version = "v1alpha1",
    kind = "PhgitSyncJob",
    namespaced
)]
#[serde(rename_all = "camelCase")]
pub struct PhgitSyncJobSpec {
    pub path: String,
    pub cluster: String,
    #[serde(default)]
    pub apply: bool,
    #[serde(default)]
    pub force: bool,
    #[serde(default)]
    pub skip_signature_verification: bool,
}

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

    // Create PhgitSyncJob resource instead of calling an orchestrator
    let client = Client::try_default()
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("Failed to create Kubernetes client: {}", e)))?;

    let sync_jobs: Api<PhgitSyncJob> = Api::default_namespaced(client);

    let job_name = format!(
        "sync-job-{}-{}",
        config.path.replace("/", "-").replace("\\", "-"),
        chrono::Utc::now().format("%y%m%d-%H%M%S")
    );

    let job = PhgitSyncJob {
        metadata: ObjectMeta {
            name: Some(job_name.clone()),
            ..Default::default()
        },
        spec: PhgitSyncJobSpec {
            path: config.path,
            cluster: config.cluster,
            apply: config.apply,
            force: config.force,
            skip_signature_verification: config.skip_signature_verification,
        },
        status: None,
    };

    let ssapply = PatchParams::apply("ph.git-sync");
    sync_jobs
        .patch(&job_name, &ssapply, &Patch::Apply(&job))
        .await
        .map_err(|e| Error::Other(anyhow::anyhow!("Failed to create PhgitSyncJob: {}", e)))?;

    println!("\n✅ PhgitSyncJob '{}' created. The git-sync controller will now handle the synchronization.", job_name);

    Ok(())
}
