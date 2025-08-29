/*
* Copyright (C) 2025 Pedro Henrique / phkaiser13
*
* File: dr_controller.rs
*
* This file implements the reconciliation logic for the PhgitDisasterRecovery
* custom resource. This controller provides an active DR strategy by monitoring
* an application's health in a primary cluster and orchestrating an automated
* failover to a DR cluster if necessary.
*
* Architecture:
* - The controller watches `PhgitDisasterRecovery` resources.
* - It operates as a state machine driven by the `status.state` field.
* - It requires kubeconfigs for both clusters to be stored in Secrets, and uses
*   a Prometheus client to monitor application health.
*
* State Transitions:
* - Monitoring: The default state. The controller periodically checks the health
*   of the target application in the primary cluster via a Prometheus query.
* - Degraded: If health checks fail consecutively beyond a threshold, the state
*   changes to Degraded. The controller now waits for a failover trigger.
* - FailingOver: Triggered automatically or manually. The controller performs
*   the failover sequence: scale down primary, replicate resources, scale up DR.
* - ActiveOnDR: The failover is complete, and the application is running on the
*   DR cluster. This is a terminal state until manual intervention.
* - Failed: If any step in the failover process fails, the state transitions
*   to Failed, requiring manual investigation.
*
* SPDX-License-Identifier: Apache-2.0
*/

use crate::crds::{
    ActiveCluster, DRState, PhgitDisasterRecovery, PhgitDisasterRecoveryStatus,
};
use crate::metrics_analyzer::{AnalysisResult, PrometheusClient};
use anyhow::Result;
use chrono::Utc;
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::{ConfigMap, Secret};
use kube::{
    api::{Api, ListParams, Patch, PatchParams},
    client::Client,
    runtime::controller::Action,
    Config, Resource, ResourceExt,
};
use reqwest;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

const APP_INSTANCE_LABEL: &str = "app.kubernetes.io/instance";
const FAILOVER_ANNOTATION: &str = "ph.io/failover";

#[derive(Debug, Error)]
pub enum Error {
    #[error("Kubernetes API error: {0}")]
    KubeError(#[from] kube::Error),
    #[error("Missing spec field in DR resource")]
    MissingSpec,
    #[error("Failed to build kubeconfig for cluster: {0}")]
    KubeconfigError(String),
    #[error("Failover failed during execution: {0}")]
    FailoverError(String),
    #[error("Metrics analysis error: {0}")]
    MetricsAnalysisError(#[from] anyhow::Error),
    #[error("Invalid interval format: {0}")]
    InvalidInterval(String),
}

/// The context required by the reconciler.
pub struct Context {
    pub client: Client, // This is the client for the operator's own cluster
    pub prometheus_client: PrometheusClient,
}

/// Creates a Kubernetes client for a remote cluster using a kubeconfig from a Secret.
async fn get_remote_client(
    op_client: &Client,
    op_namespace: &str,
    secret_name: &str,
) -> Result<Client, Error> {
    let secrets: Api<Secret> = Api::namespaced(op_client.clone(), op_namespace);
    let kubeconfig_secret = secrets.get(secret_name).await.map_err(|e| {
        Error::KubeconfigError(format!(
            "Failed to get kubeconfig secret '{}': {}",
            secret_name, e
        ))
    })?;

    let kubeconfig_bytes = kubeconfig_secret
        .data
        .and_then(|mut d| d.remove("kubeconfig"))
        .ok_or_else(|| {
            Error::KubeconfigError(format!(
                "Key 'kubeconfig' not found in secret '{}'",
                secret_name
            ))
        })?;

    let config = Config::from_kubeconfig(&kubeconfig_bytes.0)
        .await
        .map_err(|e| Error::KubeconfigError(e.to_string()))?;

    Client::try_from(config).map_err(|e| Error::KubeconfigError(e.to_string()))
}

/// The main reconciliation function for the DR controller.
pub async fn reconcile(
    dr_resource: Arc<PhgitDisasterRecovery>,
    ctx: Arc<Context>,
) -> Result<Action, Error> {
    let op_namespace = dr_resource
        .namespace()
        .ok_or_else(|| kube::Error::Request(http::Error::new("Missing namespace")))?;
    let spec = dr_resource.spec.as_ref().ok_or(Error::MissingSpec)?;
    let status = dr_resource
        .status
        .as_ref()
        .cloned()
        .unwrap_or_default();
    let dr_api: Api<PhgitDisasterRecovery> = Api::namespaced(ctx.client.clone(), &op_namespace);

    let state = status.state.clone().unwrap_or(DRState::Monitoring);

    match state {
        DRState::Monitoring => {
            println!("Monitoring health for DR resource '{}'", dr_resource.name_any());
            let health_policy = &spec.policy.health_check;
            let interval = parse_duration_str(&health_policy.interval)?;
            
            let mut new_status = status.clone();
            let mut check_is_successful = false;

            match ctx.prometheus_client.execute_prometheus_query(&health_policy.prometheus_query).await {
                Ok(metric_value) => {
                    let success_condition = health_policy
                        .success_condition
                        .as_deref()
                        .unwrap_or("value > 0"); // Default for backward compatibility
                    
                    let expression = success_condition.replace("value", &metric_value.to_string());

                    match evaluate_simple_expression(&expression) {
                        Ok(is_success) => {
                            check_is_successful = is_success;
                            println!(
                                "Health check for '{}': expression '{}' evaluated to {}",
                                dr_resource.name_any(),
                                expression,
                                is_success
                            );
                        }
                        Err(e) => {
                            eprintln!("Failed to evaluate health check expression '{}': {}", expression, e);
                            check_is_successful = false; // Treat evaluation error as failure
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Prometheus query failed for health check: {}", e);
                    check_is_successful = false; // Treat query error as failure
                }
            }

            if check_is_successful {
                new_status.consecutive_failures = 0;
                new_status.state = Some(DRState::Monitoring);
                new_status.last_health_check_time = Some(Utc::now().to_rfc3339());
            } else {
                new_status.consecutive_failures += 1;
                println!(
                    "Health check failed for '{}'. Consecutive failures: {}/{}",
                    dr_resource.name_any(),
                    new_status.consecutive_failures,
                    health_policy.failure_threshold
                );
                if new_status.consecutive_failures >= health_policy.failure_threshold {
                    println!("Failure threshold reached. Moving to Degraded state.");
                    new_status.state = Some(DRState::Degraded);
                }
            }
            
            update_status(&dr_api, &dr_resource.name_any(), new_status).await?;
            Ok(Action::requeue(interval))
        }
        DRState::Degraded => {
            println!("DR resource '{}' is Degraded. Awaiting failover trigger.", dr_resource.name_any());
            let mut should_failover = false;
            
            if spec.policy.failover_trigger == crate::crds::FailoverTrigger::Automatic {
                println!("Automatic failover policy detected. Triggering failover.");
                should_failover = true;
            } else {
                // Manual trigger check via annotation
                if let Some(annotations) = dr_resource.meta().annotations.as_ref() {
                    if let Some(val) = annotations.get(FAILOVER_ANNOTATION) {
                        if val == "true" {
                            println!("Manual failover annotation found. Triggering failover.");
                            should_failover = true;
                        }
                    }
                }
            }

            if should_failover {
                let new_status = PhgitDisasterRecoveryStatus {
                    state: Some(DRState::FailingOver),
                    ..status
                };
                update_status(&dr_api, &dr_resource.name_any(), new_status).await?;
                Ok(Action::requeue(Duration::from_secs(1))) // Requeue immediately
            } else {
                Ok(Action::requeue(Duration::from_secs(30))) // Wait for annotation
            }
        }
        DRState::FailingOver => {
            println!("Failing over application for DR resource '{}'", dr_resource.name_any());
            
            // Get clients for both clusters
            let primary_client = get_remote_client(&ctx.client, &op_namespace, &spec.primary_cluster.kubeconfig_secret_ref).await?;
            let dr_client = get_remote_client(&ctx.client, &op_namespace, &spec.dr_cluster.kubeconfig_secret_ref).await?;

            let app_ns = &spec.target_application.namespace;
            let app_name = &spec.target_application.deployment_name;

            // 1. Scale down primary deployment
            println!("Scaling down primary deployment '{}' in namespace '{}'", app_name, app_ns);
            let primary_dep_api: Api<Deployment> = Api::namespaced(primary_client.clone(), app_ns);
            let patch = json!({ "spec": { "replicas": 0 } });
            primary_dep_api.patch(app_name, &PatchParams::merge(), &Patch::Merge(&patch)).await?;

            // 2. Replicate resources (Secrets & ConfigMaps)
            println!("Replicating Secrets and ConfigMaps for '{}'", app_name);
            replicate_resources(&primary_client, &dr_client, app_ns, app_name).await?;
            
            // 3. Scale up DR deployment
            println!("Scaling up DR deployment '{}' in namespace '{}'", app_name, app_ns);
            let dr_dep_api: Api<Deployment> = Api::namespaced(dr_client.clone(), app_ns);
            
            // We need to fetch the deployment definition from primary first
            let primary_deployment = primary_dep_api.get(app_name).await?;
            let mut dr_deployment = primary_deployment.clone();
            dr_deployment.metadata.resource_version = None; // Clear resource version for apply
            
            // Apply the deployment to the DR cluster
            dr_dep_api.patch(app_name, &PatchParams::apply("ph-dr-controller"), &Patch::Apply(&dr_deployment)).await?;

            // Scale it up (the applied deployment might have replicas=0 from primary)
            let replicas = spec.dr_cluster.replicas.unwrap_or(3);
            let patch = json!({ "spec": { "replicas": replicas } });
            dr_dep_api.patch(app_name, &PatchParams::merge(), &Patch::Merge(&patch)).await?;
            println!("Scaled up DR deployment to {} replicas.", replicas);

            // 4. Update status to ActiveOnDR
            let new_status = PhgitDisasterRecoveryStatus {
                state: Some(DRState::ActiveOnDR),
                active_cluster: Some(ActiveCluster::DR),
                ..status
            };
            update_status(&dr_api, &dr_resource.name_any(), new_status).await?;
            println!("Failover complete for '{}'. Application is now active on DR cluster.", dr_resource.name_any());
            
            // 5. Send notification if configured
            if let Some(notification) = &spec.policy.notification {
                if let Some(webhook_url) = &notification.webhook_url {
                    send_notification(webhook_url, &dr_resource.name_any(), "success").await;
                }
            }

            Ok(Action::await_change())
        }
        DRState::ActiveOnDR | DRState::Failed => {
            println!("DR resource '{}' is in a terminal state ({:?}). No action needed.", dr_resource.name_any(), state);
            Ok(Action::await_change())
        }
    }
}

/// Replicates all Secrets and ConfigMaps associated with an application to a peer cluster.
async fn replicate_resources(primary_client: &Client, dr_client: &Client, ns: &str, app_name: &str) -> Result<(), Error> {
    let label_selector = format!("{}={}", APP_INSTANCE_LABEL, app_name);
    let lp = ListParams::default().labels(&label_selector);
    let ssapply = PatchParams::apply("ph-dr-controller");

    // Replicate Secrets
    let secrets_api_primary: Api<Secret> = Api::namespaced(primary_client.clone(), ns);
    let secrets_to_replicate = secrets_api_primary.list(&lp).await?;
    let secrets_api_peer: Api<Secret> = Api::namespaced(dr_client.clone(), ns);
    for mut secret in secrets_to_replicate {
        secret.metadata.resource_version = None;
        secrets_api_peer.patch(&secret.name_any(), &ssapply, &Patch::Apply(&secret)).await?;
    }

    // Replicate ConfigMaps
    let cm_api_primary: Api<ConfigMap> = Api::namespaced(primary_client.clone(), ns);
    let cms_to_replicate = cm_api_primary.list(&lp).await?;
    let cm_api_peer: Api<ConfigMap> = Api::namespaced(dr_client.clone(), ns);
    for mut cm in cms_to_replicate {
        cm.metadata.resource_version = None;
        cm_api_peer.patch(&cm.name_any(), &ssapply, &Patch::Apply(&cm)).await?;
    }
    Ok(())
}

/// Parses a simple duration string (e.g., "1m", "30s") into a `Duration`.
fn parse_duration_str(s: &str) -> Result<Duration, Error> {
    let s = s.trim();
    if let Some(num_str) = s.strip_suffix('s') {
        let secs = num_str.parse::<u64>().map_err(|_| Error::InvalidInterval(s.to_string()))?;
        Ok(Duration::from_secs(secs))
    } else if let Some(num_str) = s.strip_suffix('m') {
        let mins = num_str.parse::<u64>().map_err(|_| Error::InvalidInterval(s.to_string()))?;
        Ok(Duration::from_secs(mins * 60))
    } else if let Some(num_str) = s.strip_suffix('h') {
        let hours = num_str.parse::<u64>().map_err(|_| Error::InvalidInterval(s.to_string()))?;
        Ok(Duration::from_secs(hours * 3600))
    } else {
        Err(Error::InvalidInterval(s.to_string()))
    }
}

use anyhow::Context;

/// Evaluates simple comparison expressions.
fn evaluate_simple_expression(expression: &str) -> Result<bool, anyhow::Error> {
    let expression = expression.trim();
    if let Some(pos) = expression.find("&&") {
        let left_expr = expression[..pos].trim();
        let right_expr = expression[pos + 2..].trim();
        let left_result = evaluate_simple_expression(left_expr)?;
        let right_result = evaluate_simple_expression(right_expr)?;
        return Ok(left_result && right_result);
    }
    if let Some(pos) = expression.find("||") {
        let left_expr = expression[..pos].trim();
        let right_expr = expression[pos + 2..].trim();
        let left_result = evaluate_simple_expression(left_expr)?;
        let right_result = evaluate_simple_expression(right_expr)?;
        return Ok(left_result || right_result);
    }
    let operators = ["<=", ">=", "==", "!=", "<", ">"];
    for op in &operators {
        if let Some(pos) = expression.find(op) {
            let left_str = expression[..pos].trim();
            let right_str = expression[pos + op.len()..].trim();
            let left: f64 = left_str.parse().with_context(|| format!("Failed to parse left operand '{}' as number", left_str))?;
            let right: f64 = right_str.parse().with_context(|| format!("Failed to parse right operand '{}' as number", right_str))?;
            let result = match *op {
                "<" => left < right,
                "<=" => left <= right,
                ">" => left > right,
                ">=" => left >= right,
                "==" => (left - right).abs() < f64::EPSILON,
                "!=" => (left - right).abs() >= f64::EPSILON,
                _ => unreachable!(),
            };
            return Ok(result);
        }
    }
    Err(anyhow!("Unsupported expression format: '{}'", expression))
}

/// A helper to patch the status subresource of a PhgitDisasterRecovery.
async fn update_status(
    api: &Api<PhgitDisasterRecovery>,
    name: &str,
    status: PhgitDisasterRecoveryStatus,
) -> Result<(), Error> {
    let patch = Patch::Apply(json!({ "status": status }));
    api.patch_status(name, &PatchParams::apply("ph-dr-controller"), &patch)
        .await?;
    Ok(())
}

/// Error handling function for the reconciliation loop.
pub async fn on_error(
    dr_resource: Arc<PhgitDisasterRecovery>,
    error: &Error,
    ctx: Arc<Context>,
) -> Action {
    eprintln!(
        "DR reconciliation error for '{}': {:?}",
        dr_resource.name_any(),
        error
    );
    let ns = dr_resource.namespace().unwrap();
    let api: Api<PhgitDisasterRecovery> = Api::namespaced(ctx.client.clone(), &ns);

    let status = PhgitDisasterRecoveryStatus {
        state: Some(DRState::Failed),
        ..dr_resource.status.as_ref().cloned().unwrap_or_default()
    };

    if let Err(e) = update_status(&api, &dr_resource.name_any(), status).await {
        eprintln!("Failed to update status on error: {}", e);
    }

    // Send notification on failure if configured
    if let Some(spec) = dr_resource.spec.as_ref() {
        if let Some(notification) = &spec.policy.notification {
            if let Some(webhook_url) = &notification.webhook_url {
                let error_message = format!("Reconciliation failed: {}", error);
                send_notification(webhook_url, &dr_resource.name_any(), &error_message).await;
            }
        }
    }

    Action::requeue(Duration::from_secs(300))
}

/// Sends a POST request to a webhook URL with the failover status.
async fn send_notification(webhook_url: &str, resource_name: &str, status: &str) {
    let client = reqwest::Client::new();
    let payload = json!({
        "resource_name": resource_name,
        "status": status,
        "timestamp": Utc::now().to_rfc3339(),
    });

    println!("Sending notification to webhook: {}", webhook_url);
    match client.post(webhook_url).json(&payload).send().await {
        Ok(response) => {
            if response.status().is_success() {
                println!("Successfully sent notification for '{}'", resource_name);
            } else {
                eprintln!(
                    "Failed to send notification for '{}'. Status: {}. Body: {:?}",
                    resource_name,
                    response.status(),
                    response.text().await
                );
            }
        }
        Err(e) => {
            eprintln!(
                "Error sending webhook notification for '{}': {}",
                resource_name, e
            );
        }
    }
}
