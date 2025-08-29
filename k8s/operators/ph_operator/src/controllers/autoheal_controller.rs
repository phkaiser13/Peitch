/*
* Copyright (C) 2025 Pedro Henrique / phkaiser13
*
* SPDX-License-Identifier: Apache-2.0
*/

// CHANGE SUMMARY:
// - Reviewed the existing implementation of `execute_runbook` and confirmed its correctness
//   against all specified requirements. No functional changes were necessary.
// - Standardized file-level documentation to clarify the controller's architecture,
//   including the reconciler, webhook server, and remediation logic.
// - Ensured all comments are in English and provide clear, concise context for each
//   component, improving long-term maintainability.

// ---
//
// Module: k8s/operators/ph_operator/src/controllers/autoheal_controller.rs
//
// Purpose:
//   This file implements the controller for the `phAutoHealRule` Custom Resource.
//   It is the core of the auto-remediation logic within the ph-operator. The
//   controller operates with a dual-component architecture: a Kubernetes reconciler
//   and an embedded webhook server.
//
// Architecture:
//   1. Reconciler:
//      - Watches for `phAutoHealRule` resources in the cluster.
//      - Its primary responsibility is to maintain an in-memory cache of all active
//        rules, indexed by their `triggerName`.
//      - It uses a finalizer to ensure that when a rule is deleted from the cluster,
//        it is also cleanly removed from the in-memory cache.
//
//   2. Webhook Server (using `warp`):
//      - Exposes an HTTP endpoint (`/webhook`) to receive POST requests from an
//        external Alertmanager instance.
//      - When an alert is received, it extracts the `alertname` label and performs a
//        fast lookup in the in-memory cache to find a matching `phAutoHealRule`.
//
//   3. Remediation Logic:
//      - If a matching rule is found, it checks for a `cooldown` period to prevent
//        action storms.
//      - If not in cooldown, it dynamically creates a Kubernetes `Job` to execute a
//        runbook script. The Job mounts a script from a predefined `ConfigMap`
//        (`autoheal-runbooks`) and receives alert context as environment variables.
//      - After creating the Job, it updates the `phAutoHealRule` status with the
//        execution timestamp, enabling the cooldown logic for subsequent alerts.
//
use crate::crds::{phAutoHealRule, phAutoHealRuleStatus, HealState, StatusCondition};
use chrono::{DateTime, Utc};
use futures::stream::StreamExt;
use k8s_openapi::api::batch::v1::{Job, JobSpec};
use k8s_openapi::api::core::v1::{
    ConfigMapVolumeSource, Container, EnvVar, PodSpec, PodTemplateSpec, Volume, VolumeMount,
};
use kube::{
    api::{Api, ListParams, Patch, PatchParams, PostParams},
    client::Client,
    runtime::{
        controller::{Action, Controller},
        finalizer::{finalizer, Event as FinalizerEvent},
    },
    Resource, ResourceExt,
};
use serde::Deserialize;
use std::{collections::HashMap, sync::Arc};
use tokio::time::Duration;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, error, field, info, instrument, warn, Span};
use warp::{http::StatusCode, Filter};

// --- Custom Error Types ---

#[derive(Error, Debug)]
pub enum Error {
    #[error("Kubernetes API error: {0}")]
    KubeError(#[from] kube::Error),

    #[error("Finalizer error: {0}")]
    FinalizerError(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("Failed to parse duration string '{0}': {1}")]
    DurationParseError(String, String),

    #[error("Missing object key '{0}' in resource")]
    MissingObjectKey(&'static str),

    #[error("JSON serialization/deserialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

// --- Controller Context and State ---

/// Shared state for the controller and webhook.
struct Context {
    /// Kubernetes API client.
    client: Client,
    /// In-memory cache of auto-heal rules, indexed by trigger name for fast lookups.
    rules_cache: Arc<RwLock<HashMap<String, phAutoHealRule>>>,
}

// --- Alertmanager Webhook Structures ---

/// Represents the top-level payload received from Alertmanager.
#[derive(Deserialize, Debug)]
struct AlertmanagerPayload {
    alerts: Vec<Alert>,
}

/// Represents a single alert within the Alertmanager payload.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct Alert {
    labels: HashMap<String, String>,
    annotations: HashMap<String, String>,
}

// --- Controller Entrypoint ---

/// Runs the auto-heal controller and its associated webhook server.
pub async fn run(client: Client) {
    let rules_api: Api<phAutoHealRule> = Api::all(client.clone());

    // The shared cache is wrapped in Arc<RwLock<...>> to allow safe concurrent
    // access from both the reconciler loop and the webhook server threads.
    let rules_cache = Arc::new(RwLock::new(HashMap::new()));

    // Spawn the webhook server as a separate, long-running task.
    let webhook_task = tokio::spawn(run_webhook_server(
        rules_cache.clone(),
        client.clone(),
    ));

    // Configure and run the main controller loop.
    let controller = Controller::new(rules_api, ListParams::default())
        .run(
            reconcile,
            error_policy,
            Arc::new(Context {
                client,
                rules_cache,
            }),
        )
        .for_each(|res| async move {
            match res {
                Ok(o) => info!("Reconciliation successful for {:?}", o),
                Err(e) => warn!("Reconciliation failed: {}", e),
            }
        });

    // Run both the controller and the webhook server concurrently.
    tokio::select! {
        _ = webhook_task => warn!("Webhook server task has unexpectedly exited."),
        _ = controller => warn!("Controller reconciliation task has unexpectedly exited."),
    }
}

// --- Reconciler Implementation ---

/// The main reconciliation logic for `phAutoHealRule` resources.
/// Its primary job is to manage the in-memory cache using a finalizer.
#[instrument(skip(rule, ctx), fields(object_name = field::Empty, namespace = field::Empty))]
async fn reconcile(rule: Arc<phAutoHealRule>, ctx: Arc<Context>) -> Result<Action, Error> {
    let ns = rule.namespace().ok_or(Error::MissingObjectKey("namespace"))?;
    let name = rule.name_any();
    Span::current().record("object_name", &name).record("namespace", &ns);

    let api: Api<phAutoHealRule> = Api::namespaced(ctx.client.clone(), &ns);

    // Use the kube-rs finalizer utility to manage the lifecycle.
    finalizer(&api, "phautohealrules.ph.kaiser.io/cache-cleanup", rule, |event| async {
        match event {
            // On resource creation or update, add/update the rule in the cache.
            FinalizerEvent::Apply(rule) => {
                let mut cache = ctx.rules_cache.write().await;
                let trigger_name = rule.spec.trigger_name.clone();
                info!(trigger = %trigger_name, "Updating rule in cache");
                cache.insert(trigger_name, rule.as_ref().clone());
                Ok(Action::requeue(Duration::from_secs(3600)))
            }
            // On resource deletion, remove the rule from the cache.
            FinalizerEvent::Cleanup(rule) => {
                let mut cache = ctx.rules_cache.write().await;
                let trigger_name = rule.spec.trigger_name.clone();
                info!(trigger = %trigger_name, "Removing rule from cache");
                cache.remove(&trigger_name);
                Ok(Action::requeue(Duration::from_secs(3600)))
            }
        }
    })
    .await
    .map_err(|e| Error::FinalizerError(e.into()))
}

/// Defines the action to take when reconciliation fails.
fn error_policy(_rule: Arc<phAutoHealRule>, error: &Error, _ctx: Arc<Context>) -> Action {
    warn!("Reconciliation failed: {}", error);
    // Retry after a short delay.
    Action::requeue(Duration::from_secs(15))
}

// --- Webhook Server Implementation ---

/// A helper function to inject the shared context into warp filters.
fn with_context(
    ctx: Arc<(Arc<RwLock<HashMap<String, phAutoHealRule>>>, Client)>,
) -> impl Filter<Extract = (Arc<(Arc<RwLock<HashMap<String, phAutoHealRule>>>, Client)>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || ctx.clone())
}

/// Initializes and runs the warp-based HTTP server for receiving Alertmanager webhooks.
async fn run_webhook_server(
    rules_cache: Arc<RwLock<HashMap<String, phAutoHealRule>>>,
    client: Client,
) {
    let context = Arc::new((rules_cache, client));

    let webhook_route = warp::post()
        .and(warp::path("webhook"))
        .and(warp::body::json())
        .and(with_context(context))
        .and_then(handle_webhook);

    info!("Starting Alertmanager webhook server on 0.0.0.0:8080");
    warp::serve(webhook_route).run(([0, 0, 0, 0], 8080)).await;
}

/// The main handler for incoming webhook requests.
#[instrument(skip(payload, ctx))]
async fn handle_webhook(
    payload: AlertmanagerPayload,
    ctx: Arc<(Arc<RwLock<HashMap<String, phAutoHealRule>>>, Client)>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let (rules_cache, client) = &**ctx;
    info!("Received {} alert(s) from Alertmanager", payload.alerts.len());

    for alert in payload.alerts {
        let trigger_name = match alert.labels.get("alertname") {
            Some(name) => name,
            None => {
                warn!("Received alert without 'alertname' label. Skipping.");
                continue;
            }
        };

        let cache = rules_cache.read().await;
        if let Some(rule) = cache.get(trigger_name) {
            info!(trigger = %trigger_name, rule = %rule.name_any(), "Found matching rule for trigger");
            
            // To avoid holding the read lock for too long, we clone the necessary data.
            let rule_clone = rule.clone();
            let client_clone = client.clone();
            let alert_clone = alert.clone();

            // Spawn a new task to handle the rule execution asynchronously.
            // This allows the webhook to respond quickly to Alertmanager.
            tokio::spawn(async move {
                if let Err(e) = process_rule(rule_clone, alert_clone, client_clone).await {
                    error!(error = %e, "Failed to process auto-heal rule");
                }
            });
        } else {
            debug!(trigger = %trigger_name, "No matching rule found for trigger");
        }
    }

    // Acknowledge receipt of the webhook immediately.
    Ok(StatusCode::ACCEPTED)
}

// --- Rule Processing and Action Execution ---

/// Processes a single rule: checks cooldown and executes the runbook if applicable.
async fn process_rule(rule: phAutoHealRule, alert: Alert, client: Client) -> Result<(), Error> {
    // 1. Cooldown Check
    let now = Utc::now();
    if let Some(status) = &rule.status {
        if let Some(last_exec_str) = &status.last_execution_time {
            if let Ok(last_exec) = DateTime::parse_from_rfc3339(last_exec_str) {
                let cooldown_duration = parse_duration(&rule.spec.cooldown)?;
                if last_exec + cooldown_duration > now {
                    info!(rule = %rule.name_any(), "Rule is in cooldown. Skipping.");
                    return Ok(());
                }
            }
        }
    }

    // 2. Execute Runbook
    info!(rule = %rule.name_any(), "Executing runbook action");
    execute_runbook(&rule, &alert, &client).await?;

    // 3. Update Status
    update_status(&rule, &client).await?;

    Ok(())
}

/// Creates a Kubernetes Job to execute the specified runbook script.
async fn execute_runbook(rule: &phAutoHealRule, alert: &Alert, client: &Client) -> Result<(), Error> {
    let action = match rule.spec.actions.get(0) {
        Some(a) => a,
        None => {
            warn!(rule = %rule.name_any(), "Rule has no actions defined.");
            return Ok(());
        }
    };

    let runbook = match &action.runbook {
        Some(r) => r,
        None => {
            warn!(rule = %rule.name_any(), "Action is not a runbook.");
            return Ok(());
        }
    };

    let script_name = &runbook.script_name;
    let job_name = format!("autoheal-{}-{}", rule.name_any(), Utc::now().format("%y%m%d-%H%M%S"));
    let namespace = rule.namespace().ok_or(Error::MissingObjectKey("namespace"))?;
    let jobs_api: Api<Job> = Api::namespaced(client.clone(), &namespace);

    // Pass alert labels as environment variables, sanitizing names for shell compatibility.
    let env_vars: Vec<EnvVar> = alert.labels.iter()
        .map(|(k, v)| EnvVar {
            name: format!("ALERT_{}", k.to_uppercase().replace(|c: char| !c.is_ascii_alphanumeric(), "_")),
            value: Some(v.clone()),
            ..Default::default()
        })
        .collect();

    let job = Job {
        metadata: kube::api::ObjectMeta {
            name: Some(job_name.clone()),
            namespace: Some(namespace.clone()),
            owner_references: Some(vec![rule.controller_owner_ref(&()).unwrap()]),
            labels: Some([("app.kubernetes.io/managed-by".to_string(), "ph-operator".to_string())].into()),
            ..Default::default()
        },
        spec: Some(JobSpec {
            template: PodTemplateSpec {
                spec: Some(PodSpec {
                    containers: vec![Container {
                        name: "runbook-executor".to_string(),
                        image: Some("alpine:latest".to_string()), // Executor image
                        command: Some(vec!["/bin/sh".to_string()]),
                        args: Some(vec!["-c".to_string(), format!("/scripts/{}", script_name)]),
                        env: Some(env_vars),
                        volume_mounts: Some(vec![VolumeMount {
                            name: "runbook-scripts".to_string(),
                            mount_path: "/scripts".to_string(),
                            read_only: Some(true),
                        }]),
                        ..Default::default()
                    }],
                    restart_policy: Some("Never".to_string()),
                    volumes: Some(vec![Volume {
                        name: "runbook-scripts".to_string(),
                        config_map: Some(ConfigMapVolumeSource {
                            name: Some("autoheal-runbooks".to_string()), // Convention-based ConfigMap name
                            ..Default::default()
                        }),
                        ..Default::default()
                    }]),
                    ..Default::default()
                }),
                ..Default::default()
            },
            backoff_limit: Some(1),
            ttl_seconds_after_finished: Some(3600), // Clean up finished jobs after 1 hour
            ..Default::default()
        }),
        ..Default::default()
    };

    info!(job = %job_name, "Creating Kubernetes Job to execute runbook");
    jobs_api.create(&PostParams::default(), &job).await?;
    Ok(())
}

/// Updates the status of the `phAutoHealRule` resource after an action.
async fn update_status(rule: &phAutoHealRule, client: &Client) -> Result<(), Error> {
    let ns = rule.namespace().ok_or(Error::MissingObjectKey("namespace"))?;
    let api: Api<phAutoHealRule> = Api::namespaced(client.clone(), &ns);

    let new_status = phAutoHealRuleStatus {
        state: Some(HealState::Executing),
        last_execution_time: Some(Utc::now().to_rfc3339()),
        executions_count: Some(rule.status.as_ref().and_then(|s| s.executions_count).unwrap_or(0) + 1),
        conditions: vec![StatusCondition {
            type_: "Triggered".to_string(),
            message: "Auto-heal action Job has been created.".to_string(),
        }],
    };

    let patch = Patch::Apply(serde_json::json!({
        "apiVersion": "ph.kaiser.io/v1alpha1",
        "kind": "phAutoHealRule",
        "status": new_status
    }));

    let ps = PatchParams::apply("ph-operator-autoheal-controller").force();
    api.patch_status(&rule.name_any(), &ps, &patch).await?;
    info!(rule = %rule.name_any(), "Updated status successfully");
    Ok(())
}

// --- Utility Functions ---

/// Parses a simple duration string (e.g., "5m", "1h", "30s") into a `chrono::Duration`.
fn parse_duration(s: &str) -> Result<chrono::Duration, Error> {
    let s = s.trim();
    let numeric_part_end = s.find(|c: char| !c.is_digit(10)).unwrap_or_else(|| s.len());
    let (numeric_str, unit_str) = s.split_at(numeric_part_end);

    let value: i64 = numeric_str.parse().map_err(|_| {
        Error::DurationParseError(s.to_string(), "Invalid numeric part".to_string())
    })?;

    match unit_str {
        "s" => Ok(chrono::Duration::seconds(value)),
        "m" => Ok(chrono::Duration::minutes(value)),
        "h" => Ok(chrono::Duration::hours(value)),
        _ => Err(Error::DurationParseError(s.to_string(), format!("Unsupported unit '{}'", unit_str))),
    }
}