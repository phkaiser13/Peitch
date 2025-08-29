/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: health_logic.rs
*
* This file contains the core business logic for performing health checks against
* Kubernetes resources. It is called by the FFI bridge in `lib.rs` and is
* responsible for all interactions with the Kubernetes API server.
*
* The `perform_checks` function implements a multi-step verification process:
* 1. It connects to the Kubernetes cluster.
* 2. It fetches the target Deployment by name.
* 3. It validates the Deployment's replica status to ensure the desired number
*    of pods are running and available.
* 4. It uses the Deployment's label selector to find all associated Pods.
* 5. It iterates through each Pod, checking its phase, container readiness,
*    and restart counts.
*
* The function provides real-time, formatted feedback to the user's console
* and returns a `Result` indicating whether the application is holistically
* healthy or if any component has failed a check.
*
* SPDX-License-Identifier: Apache-2.0 */

use super::HealthCheckParams;
use anyhow::{bail, Context, Result};
use k8s_openapi::api::{apps::v1::Deployment, core::v1::Pod};
use kube::{
    api::{Api, ListParams},
    Client,
};

/// Performs a series of health checks on a given application within a Kubernetes cluster.
///
/// It checks the status of the Deployment and all its associated Pods.
pub async fn perform_checks(params: HealthCheckParams) -> Result<()> {
    // NOTE: In a real application, the namespace would likely be part of the
    // HealthCheckParams or retrieved from a central configuration. For this
    // example, we assume the 'default' namespace.
    let namespace = "default";

    // 1. Initialize the Kubernetes client.
    let client = Client::try_default()
        .await
        .context("Failed to create Kubernetes client. Is your kubeconfig set up correctly?")?;

    // 2. Check the Deployment's health.
    println!("--- Deployment: {} ---", params.app);
    let (deployment_healthy, label_selector_str) =
        check_deployment_health(&client, &params.app, namespace).await?;

    // 3. Check the health of all associated Pods.
    println!("\n--- Pods ---");
    let pods_healthy =
        check_pod_health(&client, &label_selector_str, namespace).await?;

    // 4. Determine the final outcome.
    if deployment_healthy && pods_healthy {
        Ok(())
    } else {
        // The specific error messages have already been printed.
        // We return a general failure to the FFI layer.
        bail!(
            "Application '{}' is not healthy. Please review the checks above.",
            params.app
        );
    }
}

/// Checks the status of a specific Deployment.
/// Returns a tuple: (is_healthy, label_selector_string).
async fn check_deployment_health(
    client: &Client,
    app_name: &str,
    namespace: &str,
) -> Result<(bool, String)> {
    let deployments: Api<Deployment> = Api::namespaced(client.clone(), namespace);
    let deployment = deployments
        .get(app_name)
        .await
        .with_context(|| format!("Could not find Deployment '{}'", app_name))?;

    let spec = deployment
        .spec
        .as_ref()
        .context("Deployment is missing a .spec section")?;
    let status = deployment
        .status
        .as_ref()
        .context("Deployment is missing a .status section")?;

    let desired_replicas = spec.replicas.unwrap_or(1);
    let ready_replicas = status.ready_replicas.unwrap_or(0);

    let is_healthy = ready_replicas >= desired_replicas;

    print_status(
        "Replica Status",
        is_healthy,
        &format!("{}/{} ready", ready_replicas, desired_replicas),
    );

    // Extract the label selector to find the pods managed by this deployment.
    let selector = spec
        .selector
        .as_ref()
        .context("Deployment spec is missing a label selector")?;
    let label_selector_str = selector
        .match_labels
        .as_ref()
        .map(|labels| {
            labels
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join(",")
        })
        .context("Could not form a label selector from the Deployment")?;

    Ok((is_healthy, label_selector_str))
}

/// Checks the status of all Pods matching a label selector.
async fn check_pod_health(
    client: &Client,
    label_selector: &str,
    namespace: &str,
) -> Result<bool> {
    let pods: Api<Pod> = Api::namespaced(client.clone(), namespace);
    let lp = ListParams::default().labels(label_selector);
    let pod_list = pods
        .list(&lp)
        .await
        .context("Failed to list pods using the deployment's label selector")?;

    if pod_list.items.is_empty() {
        print_status("Pod Status", false, "No pods found for this deployment.");
        return Ok(false);
    }

    let mut all_pods_healthy = true;

    for pod in pod_list.items {
        let pod_name = pod.metadata.name.as_deref().unwrap_or("unknown-pod");
        let pod_status = pod.status.as_ref();

        if let Some(status) = pod_status {
            let phase = status.phase.as_deref().unwrap_or("Unknown");
            let is_running = phase == "Running";
            print_status(&format!("Pod '{}' Phase", pod_name), is_running, phase);

            let all_containers_ready = status
                .container_statuses
                .as_ref()
                .map_or(false, |statuses| statuses.iter().all(|s| s.ready));

            print_status(
                "  └─ Containers Ready",
                all_containers_ready,
                if all_containers_ready {
                    "All ready"
                } else {
                    "Not all ready"
                },
            );

            let total_restarts = status
                .container_statuses
                .as_ref()
                .map_or(0, |statuses| statuses.iter().map(|s| s.restart_count).sum());

            // A high restart count is a warning, but we don't fail the check for it.
            // We just report it. A failing pod will be caught by phase/readiness.
            println!("    └─ Total Restarts: {}", total_restarts);

            if !is_running || !all_containers_ready {
                all_pods_healthy = false;
            }
        } else {
            print_status(&format!("Pod '{}' Status", pod_name), false, "Missing status");
            all_pods_healthy = false;
        }
    }

    Ok(all_pods_healthy)
}

/// A helper function to print a formatted status line to the console.
fn print_status(label: &str, success: bool, message: &str) {
    let status_icon = if success { "✅" } else { "❌" };
    // Pad the label to align the status icons for better readability.
    println!("{:<25} {} {}", label, status_icon, message);
}