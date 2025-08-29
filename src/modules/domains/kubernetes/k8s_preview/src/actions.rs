/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/k8s_preview/src/actions.rs
*
* This file has been refactored to implement the Operator pattern. Instead of
* imperatively creating Kubernetes resources (like namespaces) directly, the CLI
* now creates a `phPreview` custom resource. An in-cluster operator is
* responsible for watching these resources and enacting the desired state.
*
* The `handle_*_action` functions are now responsible for:
* 1. CRUD operations on `phPreview` custom resources.
* 2. Reporting status back to the user based on the `status` subresource
*    of the `phPreview` object, which is managed by the operator.
*
* SPDX-License-Identifier: Apache-2.0 */

use crate::config::PreviewConfig;
use crate::crd::{phPreview, phPreviewSpec, phPreviewStatus};
use anyhow::{anyhow, Context, Result};
use kube::{
    api::{Api, DeleteParams, ObjectMeta, PostParams},
    Client,
};
use prettytable::{row, Table};

// The namespace where phPreview resources will be created.
// The ph-operator should be configured to watch this namespace.
const PREVIEW_RESOURCE_NAMESPACE: &str = "ph-previews";

/// Handles the logic for creating a new phPreview custom resource.
///
/// This function constructs a `phPreview` object and applies it to the cluster.
/// The operator will then pick it up and create the actual environment.
pub async fn handle_create_action(config: &PreviewConfig) -> Result<()> {
    let pr_number = config.pr_number.context("Missing --pr argument")?;
    let repo_url = config.git_repo_url.as_ref().context("Missing --repo argument")?;
    let commit_sha = config.commit_sha.as_ref().context("Missing --commit-sha argument")?;
    
    println!("🚀 Submitting phPreview resource for PR #{}...", pr_number);

    let client = Client::try_default().await.context("Failed to initialize Kubernetes client")?;
    let previews: Api<phPreview> = Api::namespaced(client, PREVIEW_RESOURCE_NAMESPACE);

    let resource_name = format!("pr-{}", pr_number);

    let preview = phPreview {
        metadata: ObjectMeta {
            name: Some(resource_name.clone()),
            ..ObjectMeta::default()
        },
        spec: phPreviewSpec {
            pr_number: pr_number as i32,
            repo_url: repo_url.clone(),
            commit_sha: commit_sha.clone(),
            // These will use the defaults defined in the CRD struct if not specified.
            ttl_hours: config.new_ttl.unwrap_or(24) as i32, 
            manifest_path: "./k8s".to_string(),
        },
        status: Some(phPreviewStatus::default()),
    };

    let pp = PostParams::default();
    previews.create(&pp, &preview).await.with_context(|| {
        format!("Failed to create phPreview resource '{}'", resource_name)
    })?;

    println!("✅ Successfully submitted phPreview resource '{}'.", resource_name);
    println!("👉 The ph-operator will now create the environment. Use 'ph preview status --pr {}' to monitor progress.", pr_number);

    Ok(())
}

/// Handles the logic for deleting a phPreview custom resource.
/// The operator will see the deletion and tear down the environment.
pub async fn handle_destroy_action(config: &PreviewConfig) -> Result<()> {
    let pr_number = config.pr_number.context("Missing --pr argument")?;
    let resource_name = format!("pr-{}", pr_number);
    
    println!("🔥 Submitting deletion request for phPreview resource '{}'...", resource_name);
    
    let client = Client::try_default().await.context("Failed to initialize Kubernetes client")?;
    let previews: Api<phPreview> = Api::namespaced(client, PREVIEW_RESOURCE_NAMESPACE);

    previews.delete(&resource_name, &DeleteParams::default()).await.with_context(|| {
        format!("Failed to delete phPreview resource '{}'", resource_name)
    })?;

    println!("✅ Successfully deleted phPreview resource '{}'. The operator will now tear down the environment.", resource_name);
    Ok(())
}

/// Fetches the `phPreview` resource and displays its status subresource.
pub async fn handle_status_action(config: &PreviewConfig) -> Result<()> {
    let pr_number = config.pr_number.context("Missing --pr argument")?;
    let resource_name = format!("pr-{}", pr_number);

    println!("🔎 Getting status for phPreview resource '{}'...", resource_name);

    let client = Client::try_default().await.context("Failed to initialize Kubernetes client")?;
    let previews: Api<phPreview> = Api::namespaced(client, PREVIEW_RESOURCE_NAMESPACE);
    
    let preview = previews.get(&resource_name).await.with_context(|| {
        format!("Could not find phPreview resource '{}'. Has it been created?", resource_name)
    })?;

    if let Some(status) = preview.status {
        let mut table = Table::new();
        table.add_row(row!["PROPERTY", "VALUE"]);
        table.add_row(row!["Name", resource_name]);
        table.add_row(row!["Phase", status.phase]);
        table.add_row(row!["URL", status.url]);
        table.add_row(row!["Expires At", status.expires_at]);
        table.add_row(row!["Message", status.message]);
        table.printstd();
    } else {
        println!("The phPreview resource '{}' does not have a status field yet. The operator may not have processed it.", resource_name);
    }

    Ok(())
}

use crate::kube_client;
use chrono::{Duration, Utc};
use futures::stream::TryStreamExt;
use kube::api::{ListParams, Patch, PatchParams};
use serde_json::json;

/// Finds the target namespace and pod, then streams logs back to the user.
pub async fn handle_logs_action(config: &PreviewConfig) -> Result<()> {
    let pr_number = config.pr_number.context("Missing --pr argument")?;
    let component = config.component_name.as_ref().context("Missing --component argument")?;
    let namespace = format!("pr-{}", pr_number);

    println!("🔎 Finding pod for component '{}' in namespace '{}'...", component, namespace);
    let client = Client::try_default().await?;
    let pod_name = kube_client::get_pod_by_component_label(&client, &namespace, component).await?;
    println!("✅ Found pod '{}'. Streaming logs...", pod_name);

    let mut log_stream = kube_client::stream_pod_logs(&client, &namespace, &pod_name).await?;

    while let Some(line) = log_stream.try_next().await? {
        print!("{}", String::from_utf8_lossy(&line));
    }

    Ok(())
}

/// Finds the target pod and executes an interactive command inside it.
pub async fn handle_exec_action(config: &PreviewConfig) -> Result<()> {
    let pr_number = config.pr_number.context("Missing --pr argument")?;
    let component = config.component_name.as_ref().context("Missing --component argument")?;
    let command = config.command_to_exec.as_ref().context("Missing command to execute")?;
    let namespace = format!("pr-{}", pr_number);

    println!("🔎 Finding pod for component '{}' in namespace '{}'...", component, namespace);
    let client = Client::try_default().await?;
    let pod_name = kube_client::get_pod_by_component_label(&client, &namespace, component).await?;
    println!("✅ Found pod '{}'. Executing command...", pod_name);

    kube_client::exec_in_pod(&client, &namespace, &pod_name, command).await
}

/// Patches the `phPreview` custom resource to extend its TTL.
pub async fn handle_extend_action(config: &PreviewConfig) -> Result<()> {
    let pr_number = config.pr_number.context("Missing --pr argument")?;
    let new_ttl = config.new_ttl.context("Missing --ttl argument")?;
    let resource_name = format!("pr-{}", pr_number);

    println!("🚀 Extending TTL for phPreview resource '{}' to {} hours...", resource_name, new_ttl);

    let client = Client::try_default().await?;
    let previews: Api<phPreview> = Api::namespaced(client, PREVIEW_RESOURCE_NAMESPACE);

    let patch = json!({
        "spec": {
            "ttl_hours": new_ttl
        }
    });

    let patch_params = PatchParams::default();
    previews.patch(&resource_name, &patch_params, &Patch::Merge(&patch)).await.with_context(|| {
        format!("Failed to patch phPreview resource '{}'", resource_name)
    })?;

    println!("✅ Successfully patched phPreview resource. The operator will update the environment's expiration.");
    Ok(())
}

/// Implements client-side garbage collection by deleting expired `phPreview` resources.
pub async fn handle_gc_action(config: &PreviewConfig) -> Result<()> {
    let max_age_hours = config.max_age_hours.context("Missing --max-age-hours argument")?;
    println!("🗑️  Running garbage collection for previews older than {} hours...", max_age_hours);

    let client = Client::try_default().await?;
    let previews: Api<phPreview> = Api::namespaced(client, PREVIEW_RESOURCE_NAMESPACE);
    let preview_list = previews.list(&ListParams::default()).await?;

    let now = Utc::now();
    let max_age = Duration::hours(max_age_hours as i64);
    let mut deleted_count = 0;

    for preview in preview_list {
        let resource_name = preview.metadata.name.as_ref().unwrap();
        if let Some(creation_timestamp) = &preview.metadata.creation_timestamp {
            let age = now.signed_duration_since(creation_timestamp.0);
            if age > max_age {
                println!("  -> Found expired preview: '{}' (age: {} hours). Deleting...", resource_name, age.num_hours());
                match previews.delete(resource_name, &DeleteParams::default()).await {
                    Ok(_) => {
                        println!("     ✅ Deleted successfully.");
                        deleted_count += 1;
                    }
                    Err(e) => eprintln!("     ❌ Failed to delete '{}': {}", resource_name, e),
                }
            }
        }
    }

    println!("✅ Garbage collection complete. Deleted {} expired previews.", deleted_count);
    Ok(())
}