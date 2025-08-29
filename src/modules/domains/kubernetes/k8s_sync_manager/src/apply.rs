/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/git_sync/src/kube_apply.rs
* This file contains the core logic for interacting with a Kubernetes cluster
* to apply manifests. It provides a function that takes a path to a directory
* of manifests and applies them idempotently using the Server-Side Apply
* strategy. This module abstracts away the complexities of the kube-rs library,
* such as resource discovery and API interaction, providing a clean interface
* for other parts of the `git_sync` module.
* SPDX-License-Identifier: Apache-2.0 */

use kube::{
    api::{Api, DynamicObject, Patch, PatchParams},
    discovery, Client,
};
use log::{info, warn};
use tokio::fs::read_to_string;
use walkdir::WalkDir;

/// Applies all Kubernetes manifests found in a given directory path to the cluster.
pub async fn apply_manifests_from_path(
    client: Client,
    manifests_path: &str,
    dry_run: bool,
    force: bool,
) -> Result<String, anyhow::Error> {
    info!(
        "Starting manifest application from path: {} (Dry Run: {}, Force: {})",
        manifests_path, dry_run, force
    );

    let mut applied_count = 0;
    for entry in WalkDir::new(manifests_path)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file()
            && (path.extension().map_or(false, |s| s == "yaml" || s == "yml"))
        {
            info!("Processing manifest file: {}", path.display());

            let file_content = read_to_string(path).await?;

            for document in serde_yaml::Deserializer::from_str(&file_content) {
                match DynamicObject::deserialize(document) {
                    Ok(mut obj) => {
                        let mut ssapply = PatchParams::apply("ph-kube-sync-apply");
                        if dry_run {
                            ssapply.dry_run = true;
                        }
                        if force {
                            ssapply.force = true;
                        }

                        let gvk = obj.types.as_ref().ok_or_else(|| {
                            anyhow::anyhow!("DynamicObject is missing GroupVersionKind information")
                        })?;

                        let api_resource = discovery::group_version_kind(&client, gvk)
                            .await?
                            .1;

                        let api: Api<DynamicObject> =
                            if let Some(namespace) = obj.metadata.namespace.as_deref() {
                                Api::namespaced_with(client.clone(), namespace, &api_resource)
                            } else {
                                Api::all_with(client.clone(), &api_resource)
                            };

                        let name = obj
                            .metadata
                            .name
                            .as_ref()
                            .ok_or_else(|| anyhow::anyhow!("Manifest is missing metadata.name"))?;

                        info!(
                            "Applying resource '{}' of kind '{}'...",
                            name, gvk.kind
                        );

                        api.patch(name, &ssapply, &Patch::Apply(&obj)).await?;

                        applied_count += 1;
                        info!(
                            "Successfully applied resource '{}' of kind '{}'.",
                            name, gvk.kind
                        );
                    }
                    Err(e) => {
                        warn!(
                            "Skipping document in file {} due to deserialization error: {}",
                            path.display(),
                            e
                        );
                    }
                }
            }
        }
    }

    let success_message = format!(
        "Successfully applied {} manifest(s) from {}.",
        applied_count,
        manifests_path
    );
    info!("{}", success_message);
    Ok(success_message)
}
