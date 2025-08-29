/*
* Copyright (C) 2025 Pedro Henrique / phkaiser13
*
* SPDX-License-Identifier: Apache-2.0
*/

// CHANGE SUMMARY:
// - Implemented the `apply_manifests` function, replacing the previous simulation.
// - The new implementation iterates through a specified directory (`repo_path`) to find
//   all `.yaml` and `.yml` files.
// - For each manifest found, it spawns a `kubectl apply -n <namespace> -f -` subprocess.
// - The content of the manifest file is piped to the `stdin` of the `kubectl` process,
//   which is a secure and efficient method for applying manifests without temporary files.
// - Error handling has been added to capture and report any failures from the `kubectl`
//   command, providing clear feedback to the user.
// - Added `use` statements for `tokio::process::Command`, `std::process::Stdio`, and
//   `walkdir::WalkDir` to support the new implementation.

// ---
//
// Module: src/modules/k8s_preview/src/kube_client.rs
//
// Purpose:
//   This file provides a high-level, abstracted interface for interacting with the
//   Kubernetes cluster. It encapsulates the logic of using the `kube-rs` crate and
//   shelling out to `kubectl` for certain operations, exposing simple, task-oriented
//   async functions. This separation of concerns keeps the main business logic in
//   `actions.rs` clean and independent of the specific Kubernetes client
//   implementation details.
//
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use futures::{Stream, StreamExt};
use k8s_openapi::api::{
    core::v1::{Namespace, Pod},
    networking::v1::Ingress,
};
use kube::{
    api::{
        Api, AttachParams, DeleteParams, ListParams, LogParams, ObjectMeta, Patch, PatchParams,
        PostParams,
    },
    Client, Config,
};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;
/* BEGIN CHANGE: Add necessary imports for subprocess and directory walking. */
use std::process::Stdio;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use walkdir::WalkDir; // NOTE: This implementation assumes the `walkdir` crate is a dependency.
/* END CHANGE */


/// A simplified struct to hold pod status for display purposes.
#[derive(Debug)]
pub struct PodStatus {
    pub name: String,
    pub status: String,
    pub restarts: i32,
    pub age: String,
}

/// Initializes a Kubernetes client from a configuration.
pub async fn initialize_client(kubeconfig_path: Option<&str>) -> Result<Client> {
    let config = match kubeconfig_path {
        Some(path) => Config::from_kubeconfig(&kube::config::Kubeconfig::read_from(path)?).await,
        None => Config::infer().await,
    }
    .context("Failed to load Kubernetes config")?;
    Client::try_from(config).context("Failed to create Kubernetes client from config")
}

/// Creates a new namespace in the Kubernetes cluster.
pub async fn create_namespace(client: &Client, name: &str, ttl_hours: u32) -> Result<()> {
    let ns_api: Api<Namespace> = Api::all(client.clone());
    let mut labels = BTreeMap::new();
    labels.insert("managed-by".to_string(), "peitch".to_string());
    let mut annotations = BTreeMap::new();
    annotations.insert("peitch.io/ttl-hours".to_string(), ttl_hours.to_string());

    let namespace = Namespace {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            labels: Some(labels),
            annotations: Some(annotations),
            ..ObjectMeta::default()
        },
        ..Namespace::default()
    };

    ns_api
        .create(&PostParams::default(), &namespace)
        .await
        .with_context(|| format!("Failed to create Kubernetes namespace '{}'", name))?;
    println!("Successfully created namespace: {}", name);
    Ok(())
}

/// Deletes a namespace from the Kubernetes cluster.
pub async fn delete_namespace(client: &Client, name: &str) -> Result<()> {
    let ns_api: Api<Namespace> = Api::all(client.clone());
    match ns_api.delete(name, &DeleteParams::default()).await {
        Ok(_) => {
            println!("Successfully deleted namespace: {}", name);
            Ok(())
        }
        Err(kube::Error::Api(e)) if e.code == 404 => {
            println!("Namespace '{}' not found, assuming already deleted.", name);
            Ok(())
        }
        Err(e) => Err(e).with_context(|| format!("Failed to delete Kubernetes namespace '{}'", name)),
    }
}

/* BEGIN CHANGE: Implement manifest application using kubectl. */
/// Applies all Kubernetes manifests (`.yaml`/`.yml`) found in a directory.
///
/// This function shells out to `kubectl`, which is a robust way to handle
/// complex multi-document YAML files and CRDs.
pub async fn apply_manifests(_client: &Client, namespace: &str, repo_path: &Path) -> Result<()> {
    println!(
        "Applying manifests from git repo at '{}' to namespace '{}'...",
        repo_path.display(),
        namespace
    );

    let walker = WalkDir::new(repo_path).into_iter();
    for entry in walker.filter_map(Result::ok).filter(|e| {
        e.file_type().is_file()
            && e.path()
                .extension()
                .map_or(false, |ext| ext == "yaml" || ext == "yml")
    }) {
        let manifest_path = entry.path();
        println!("--> Applying manifest: {}", manifest_path.display());

        let manifest_content = std::fs::read_to_string(manifest_path)
            .with_context(|| format!("Failed to read manifest file: {}", manifest_path.display()))?;

        // Skip empty files to avoid kubectl errors.
        if manifest_content.trim().is_empty() {
            println!("    Skipping empty manifest file.");
            continue;
        }

        let mut child = Command::new("kubectl")
            .args(["apply", "-n", namespace, "-f", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to spawn kubectl command. Is kubectl in your PATH?")?;

        // Take ownership of stdin and write the manifest content to it.
        // This must be done in a separate task or before waiting on the child
        // to avoid deadlocks if the content is large.
        let mut stdin = child.stdin.take().expect("Failed to open stdin for kubectl");
        stdin.write_all(manifest_content.as_bytes()).await?;
        drop(stdin); // Close stdin to signal the end of input to kubectl.

        let output = child
            .wait_with_output()
            .await
            .context("Failed to wait for kubectl command to complete")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!(
                "kubectl apply failed for {}: {}",
                manifest_path.display(),
                stderr
            ));
        } else {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Print kubectl's output for user feedback.
            print!("    {}", stdout);
        }
    }

    println!("\nSuccessfully applied all manifests.");
    Ok(())
}
/* END CHANGE */


/// Lists all pods in a given namespace and returns their status.
pub async fn list_pods_in_namespace(client: &Client, namespace: &str) -> Result<Vec<PodStatus>> {
    let pods_api: Api<Pod> = Api::namespaced(client.clone(), namespace);
    let pod_list = pods_api.list(&ListParams::default()).await?;
    let now = Utc::now();

    let statuses = pod_list
        .iter()
        .map(|pod| {
            let name = pod.metadata.name.clone().unwrap_or_default();
            let status = pod
                .status
                .as_ref()
                .and_then(|s| s.phase.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            let restarts = pod
                .status
                .as_ref()
                .and_then(|s| s.container_statuses.as_ref())
                .and_then(|cs| cs.first())
                .map_or(0, |s| s.restart_count);
            let age = pod
                .metadata
                .creation_timestamp
                .as_ref()
                .map(|ts| format_age(&ts.0, &now))
                .unwrap_or_else(|| "N/A".to_string());

            PodStatus { name, status, restarts, age }
        })
        .collect();

    Ok(statuses)
}

/// Retrieves the first URL found on an Ingress resource in the namespace.
pub async fn get_ingress_url(client: &Client, namespace: &str) -> Result<Option<String>> {
    let ingress_api: Api<Ingress> = Api::namespaced(client.clone(), namespace);
    let ingresses = ingress_api.list(&ListParams::default()).await?;

    Ok(ingresses
        .items
        .first()
        .and_then(|ingress| ingress.spec.as_ref())
        .and_then(|spec| spec.rules.as_ref())
        .and_then(|rules| rules.first())
        .and_then(|rule| rule.host.as_ref())
        .map(|host| format!("https://{}", host)))
}

/// Finds a pod name by a component label.
pub async fn get_pod_by_component_label(
    client: &Client,
    namespace: &str,
    component: &str,
) -> Result<String> {
    let pods_api: Api<Pod> = Api::namespaced(client.clone(), namespace);
    let lp = ListParams::default().labels(&format!("app.kubernetes.io/component={}", component));
    let pods = pods_api.list(&lp).await?;

    pods.items
        .first()
        .and_then(|pod| pod.metadata.name.clone())
        .ok_or_else(|| anyhow!("No pod found with component label '{}'", component))
}

/// Streams logs from a specific pod.
pub async fn stream_pod_logs(
    client: &Client,
    namespace: &str,
    pod_name: &str,
) -> Result<impl Stream<Item = Result<bytes::Bytes, kube::Error>>> {
    let pods_api: Api<Pod> = Api::namespaced(client.clone(), namespace);
    let log_params = LogParams {
        follow: true,
        ..Default::default()
    };
    pods_api
        .log_stream(pod_name, &log_params)
        .await
        .context("Failed to establish log stream")
}

/// Executes a command in a pod with an interactive TTY session.
pub async fn exec_in_pod(
    client: &Client,
    namespace: &str,
    pod_name: &str,
    command: &[String],
) -> Result<()> {
    let pods_api: Api<Pod> = Api::namespaced(client.clone(), namespace);
    let attach_params = AttachParams::interactive_tty();
    let mut attached = pods_api.exec(pod_name, command, &attach_params).await?;

    let mut stdin = attached
        .stdin()
        .ok_or_else(|| anyhow!("stdin is not available"))?;
    let mut stdout = attached
        .stdout()
        .ok_or_else(|| anyhow!("stdout is not available"))?;

    let mut term_stdin = tokio::io::stdin();
    let mut term_stdout = tokio::io::stdout();

    // Forward terminal stdin to the process stdin
    let stdin_task = tokio::spawn(async move {
        tokio::io::copy(&mut term_stdin, &mut stdin).await.ok();
    });

    // Forward process stdout to the terminal stdout
    let stdout_task = tokio::spawn(async move {
        tokio::io::copy(&mut stdout, &mut term_stdout).await.ok();
    });

    // Wait for the process to finish or for one of the tasks to complete
    tokio::select! {
        _ = stdin_task => {},
        _ = stdout_task => {},
        status = attached.await => {
            println!("\nProcess exited with status: {:?}", status);
        }
    }

    Ok(())
}

/// Updates the TTL annotation on a namespace.
pub async fn update_namespace_ttl(client: &Client, name: &str, new_ttl_hours: u32) -> Result<()> {
    let ns_api: Api<Namespace> = Api::all(client.clone());
    let patch = json!({
        "metadata": {
            "annotations": {
                "peitch.io/ttl-hours": new_ttl_hours.to_string()
            }
        }
    });

    ns_api
        .patch(name, &PatchParams::default(), &Patch::Merge(&patch))
        .await
        .with_context(|| format!("Failed to patch namespace '{}'", name))?;
    Ok(())
}

/// Lists all preview namespaces that have expired based on their creation time and TTL.
pub async fn list_expired_preview_namespaces(
    client: &Client,
    max_age_hours: u32,
) -> Result<Vec<String>> {
    let ns_api: Api<Namespace> = Api::all(client.clone());
    let lp = ListParams::default().labels("managed-by=peitch");
    let namespaces = ns_api.list(&lp).await?;
    let now = Utc::now();
    let max_age_duration = chrono::Duration::hours(max_age_hours as i64);

    let expired: Vec<String> = namespaces
        .into_iter()
        .filter_map(|ns| {
            let metadata = ns.metadata;
            let name = metadata.name?;
            let creation_time = metadata.creation_timestamp?.0;
            let age = now.signed_duration_since(creation_time);

            if age > max_age_duration {
                Some(name)
            } else {
                None
            }
        })
        .collect();

    Ok(expired)
}

/// Helper to format a duration into a human-readable string.
fn format_age(creation_timestamp: &DateTime<Utc>, now: &DateTime<Utc>) -> String {
    let duration = now.signed_duration_since(*creation_timestamp);
    if duration.num_days() > 0 {
        format!("{}d", duration.num_days())
    } else if duration.num_hours() > 0 {
        format!("{}h", duration.num_hours())
    } else if duration.num_minutes() > 0 {
        format!("{}m", duration.num_minutes())
    } else {
        format!("{}s", duration.num_seconds())
    }
}