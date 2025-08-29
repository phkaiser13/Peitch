/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/git_sync/src/kube_diff.rs
*
* This file contains the core logic for the drift detection feature. It is
* responsible for fetching the state of managed resources from a Kubernetes
* cluster, comparing it against the desired state defined in local manifest
* files, and generating a detailed report of any discrepancies.
*
* The main function, `detect_drift`, orchestrates this process by:
* 1. Discovering all available, namespaced API resources in the cluster.
* 2. Listing all instances of these resources that are managed by `phgit` (identified by a label).
* 3. Reading all local manifest files from the specified path.
* 4. Comparing the two sets of resources to find added, deleted, and modified items.
* 5. Using the `similar` crate to generate a textual diff for modified resources.
*
* SPDX-License-Identifier: Apache-2.0 */

use kube::{
    api::{Api, DynamicObject, ListParams},
    discovery, Client,
};
use serde_yaml;
use similar::{ChangeTag, TextDiff};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

type ResourceMap = BTreeMap<String, DynamicObject>;

/// Loads all Kubernetes resource definitions from YAML files in a given local directory.
fn load_local_resources(path: &str) -> Result<ResourceMap, anyhow::Error> {
    let mut local_resources = ResourceMap::new();
    for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.is_file() && (p.extension().map_or(false, |s| s == "yaml" || s == "yml")) {
            let content = fs::read_to_string(p)?;
            for doc in serde_yaml::Deserializer::from_str(&content) {
                if let Ok(resource) = DynamicObject::deserialize(doc) {
                    let kind = &resource.types.as_ref().ok_or(anyhow::anyhow!("Resource missing kind"))?.kind;
                    let name = resource.metadata.name.as_ref().ok_or(anyhow::anyhow!("Resource missing name"))?;
                    let namespace = resource.metadata.namespace.as_deref().unwrap_or("default");
                    let key = format!("{}/{}/{}", kind, namespace, name);
                    local_resources.insert(key, resource);
                }
            }
        }
    }
    Ok(local_resources)
}

/// Lists all phgit-managed resources from the Kubernetes cluster.
async fn load_cluster_resources(client: &Client) -> Result<ResourceMap, anyhow::Error> {
    let mut cluster_resources = ResourceMap::new();
    let discovery = discovery::discover_resources(client, discovery::Scope::Namespaced).await?;
    
    let lp = ListParams::default().labels("app.kubernetes.io/managed-by=phgit");

    for group in discovery.groups() {
        for ar in &group.resources {
            let api: Api<DynamicObject> = Api::all_with(client.clone(), &ar.api_resource);
            match api.list(&lp).await {
                Ok(list) => {
                    for item in list {
                        let kind = &item.types.as_ref().ok_or(anyhow::anyhow!("Resource missing kind"))?.kind;
                        let name = item.metadata.name.as_ref().ok_or(anyhow::anyhow!("Resource missing name"))?;
                        let namespace = item.metadata.namespace.as_deref().unwrap_or("default");
                        let key = format!("{}/{}/{}", kind, namespace, name);
                        cluster_resources.insert(key, item);
                    }
                }
                Err(e) => eprintln!("  -> Error listing {}: {}", ar.api_resource.kind, e),
            }
        }
    }
    Ok(cluster_resources)
}

/// Compares local and cluster resources and generates a drift report.
fn compare_resources(local: &ResourceMap, cluster: &ResourceMap) -> Option<String> {
    let mut report = String::new();
    let mut has_drift = false;

    // Check for added and modified resources
    for (key, local_res) in local {
        match cluster.get(key) {
            Some(cluster_res) => {
                // Modified: Compare specs
                let local_spec = serde_json::to_string_pretty(&local_res.data.get("spec")).unwrap_or_default();
                let cluster_spec = serde_json::to_string_pretty(&cluster_res.data.get("spec")).unwrap_or_default();
                if local_spec != cluster_spec {
                    has_drift = true;
                    report.push_str(&format!("\n--- MODIFIED: {} ---\n", key));
                    let diff = TextDiff::from_lines(&cluster_spec, &local_spec);
                    for change in diff.iter_all_changes() {
                        let sign = match change.tag() {
                            ChangeTag::Delete => "-",
                            ChangeTag::Insert => "+",
                            ChangeTag::Equal => " ",
                        };
                        report.push_str(&format!("{}{}", sign, change));
                    }
                }
            }
            None => {
                // Added
                has_drift = true;
                report.push_str(&format!("\n--- ADDED: {} ---\n", key));
                report.push_str(&serde_yaml::to_string(local_res).unwrap_or_default());
            }
        }
    }

    // Check for deleted resources
    for (key, _) in cluster {
        if !local.contains_key(key) {
            has_drift = true;
            report.push_str(&format!("\n--- DELETED: {} (in cluster but not in local files) ---\n", key));
        }
    }

    if has_drift {
        Some(report)
    } else {
        None
    }
}


/// The main entry point for drift detection.
pub async fn detect_drift(client: &Client, local_path: &str) -> Result<Option<String>, anyhow::Error> {
    if !Path::new(local_path).exists() {
        return Err(anyhow::anyhow!("Local path for drift detection not found at '{}'", local_path));
    }

    let local_resources = load_local_resources(local_path)?;
    let cluster_resources = load_cluster_resources(client).await?;
    
    let report = compare_resources(&local_resources, &cluster_resources);

    Ok(report)
}
