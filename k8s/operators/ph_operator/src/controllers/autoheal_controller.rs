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
use crate::crds::{
    phAutoHealRule, phAutoHealRuleStatus, HealState, NotifyAction, SnapshotAction, StatusCondition,
};
use chrono::{DateTime, Utc};
use futures::stream::StreamExt;
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::batch::v1::{Job, JobSpec};
use k8s_openapi::api::core::v1::{
    ConfigMapVolumeSource, Container, EnvVar, PodSpec, PodTemplateSpec, Secret, Volume,
    VolumeMount,
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
use notification_manager::{send_notification, IssueNotification, SlackNotification};
use serde::Deserialize;
use serde_json::json;
use snapshot_manager::{self, SnapshotConfig};
use std::{collections::HashMap, sync::Arc};
use thiserror::Error;
use tokio::sync::RwLock;
use tokio::time::Duration;
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

    #[error("Notification/Failover error: {0}")]
    FailoverError(String),
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

/// Processes a single rule: checks cooldown and executes the defined actions if applicable.
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

    // 2. Execute all defined actions sequentially.
    info!(rule = %rule.name_any(), "Executing {} action(s) for rule", rule.spec.actions.len());
    for (i, action) in rule.spec.actions.iter().enumerate() {
        info!(action_index = i + 1, "Executing action");
        if let Some(redeploy) = &action.redeploy {
            if let Err(e) = execute_redeploy_action(&rule, &alert, &client, redeploy).await {
                error!(error = %e, "Redeploy action failed");
                // Optionally, decide if we should stop processing further actions on failure
            }
        } else if let Some(scale_up) = &action.scale_up {
            if let Err(e) = execute_scale_up_action(&rule, &alert, &client, scale_up).await {
                error!(error = %e, "Scale-up action failed");
            }
        } else if let Some(runbook) = &action.runbook {
            if let Err(e) = execute_runbook_action(&rule, &alert, &client, runbook).await {
                error!(error = %e, "Runbook action failed");
            }
        } else if let Some(notify) = &action.notify {
            if let Err(e) = execute_notify_action(&rule, &alert, &client, notify).await {
                error!(error = %e, "Notify action failed");
            }
        } else if let Some(snapshot) = &action.snapshot {
            if let Err(e) = execute_snapshot_action(&rule, &alert, &client, snapshot).await {
                error!(error = %e, "Snapshot action failed");
            }
        } else {
            warn!("Action at index {} is empty or has an unknown type.", i);
        }
    }

    // 3. Update Status after all actions are attempted.
    update_status(&rule, &client).await?;

    Ok(())
}

// --- Action Execution Helpers ---

/// Executes a notification action, sending messages to Slack and/or creating an issue.
async fn execute_notify_action(
    rule: &phAutoHealRule,
    alert: &Alert,
    client: &Client,
    action: &NotifyAction,
) -> Result<(), Error> {
    info!(rule = %rule.name_any(), "Executing notify action");

    let mut slack_payload = None;
    if let Some(slack_config) = &action.slack {
        // Fetch the webhook URL from the specified secret
        let secrets: Api<Secret> = Api::namespaced(client.clone(), rule.namespace().unwrap().as_str());
        let secret = secrets.get(&slack_config.webhook_url_secret_ref).await?;
        if let Some(data) = secret.data {
            if let Some(url_bytes) = data.get("webhookUrl") {
                let webhook_url = String::from_utf8(url_bytes.0.clone()).unwrap_or_default();
                if !webhook_url.is_empty() {
                    // TODO: Implement simple templating for the message
                    slack_payload = Some(SlackNotification {
                        webhook_url: &webhook_url,
                        message: &slack_config.message,
                    });
                } else {
                    warn!("'webhookUrl' key in secret '{}' is empty.", slack_config.webhook_url_secret_ref);
                }
            } else {
                warn!("'webhookUrl' key not found in secret '{}'.", slack_config.webhook_url_secret_ref);
            }
        }
    }

    let mut issue_payload = None;
    if let Some(issue_config) = &action.issue {
        // TODO: Implement simple templating for title and body
        // TODO: The repo needs to be configured somewhere, for now, hardcoding
        let repo = "phkaiser13/peitch";
        issue_payload = Some(IssueNotification {
            repo,
            title: &issue_config.title,
            body: &issue_config.body,
        });
    }

    send_notification(slack_payload, issue_payload)
        .await
        .map_err(|e| Error::FailoverError(e.to_string()))?;

    Ok(())
}

/// Executes a diagnostic snapshot action.
async fn execute_snapshot_action(
    rule: &phAutoHealRule,
    alert: &Alert,
    client: &Client,
    action: &SnapshotAction,
) -> Result<(), Error> {
    let ns = rule.namespace().ok_or(Error::MissingObjectKey("namespace"))?;
    info!(rule = %rule.name_any(), "Executing snapshot action");

    // Try to get the application name from the alert labels. Fallback to a default if not present.
    let app_name = alert
        .labels
        .get("app")
        .or_else(|| alert.labels.get("app_kubernetes_io_name"))
        .map(|s| s.as_str())
        .unwrap_or("unknown-app");

    let config = SnapshotConfig {
        app_name,
        namespace: &ns,
        snapshot_name: &action.name,
        include_logs: action.include_logs,
        include_traces: action.include_traces,
        include_db_dump: action.include_db_dump,
    };

    match snapshot_manager::take_snapshot(client.clone(), config).await {
        Ok(filepath) => {
            info!(
                rule = %rule.name_any(),
                filepath = %filepath,
                "Successfully created diagnostic snapshot"
            );
            // Optionally, link the snapshot in a notification
        }
        Err(e) => {
            error!(
                rule = %rule.name_any(),
                error = %e,
                "Failed to create diagnostic snapshot"
            );
            // Don't return an error to the controller, just log it.
        }
    }

    Ok(())
}

/// Triggers a rolling restart of a deployment by setting an annotation.
async fn execute_redeploy_action(
    rule: &phAutoHealRule,
    _alert: &Alert,
    client: &Client,
    action: &crate::crds::RedeployAction,
) -> Result<(), Error> {
    let ns = rule.namespace().ok_or(Error::MissingObjectKey("namespace"))?;
    let dep_api: Api<Deployment> = Api::namespaced(client.clone(), &ns);
    let target_name = &action.target;

    info!(deployment = %target_name, "Executing redeploy action");

    let patch = json!({
        "spec": {
            "template": {
                "metadata": {
                    "annotations": {
                        "ph.io/restartedAt": Utc::now().to_rfc3339()
                    }
                }
            }
        }
    });

    dep_api.patch(target_name, &PatchParams::apply("ph-autoheal-controller"), &Patch::Merge(&patch)).await?;
    info!(deployment = %target_name, "Successfully triggered redeploy");
    Ok(())
}

/// Scales up a deployment to a specified number of replicas.
async fn execute_scale_up_action(
    rule: &phAutoHealRule,
    _alert: &Alert,
    client: &Client,
    action: &crate::crds::ScaleUpAction,
) -> Result<(), Error> {
    let ns = rule.namespace().ok_or(Error::MissingObjectKey("namespace"))?;
    let dep_api: Api<Deployment> = Api::namespaced(client.clone(), &ns);
    let target_name = &action.target;
    let replicas = action.replicas;

    info!(deployment = %target_name, replicas = replicas, "Executing scale-up action");

    let patch = json!({ "spec": { "replicas": replicas } });
    dep_api.patch(target_name, &PatchParams::merge(), &Patch::Merge(&patch)).await?;
    info!(deployment = %target_name, "Successfully scaled up");
    Ok(())
}


/// Creates a Kubernetes Job to execute the specified runbook script.
async fn execute_runbook_action(
    rule: &phAutoHealRule,
    alert: &Alert,
    client: &Client,
    runbook: &crate::crds::RunbookSpec,
) -> Result<(), Error> {
    let script_name = &runbook.script_name;
    let job_name = format!("autoheal-{}-{}", rule.name_any(), Utc::now().format("%y%m%d-%H%M%S"));
    let namespace = rule.namespace().ok_or(Error::MissingObjectKey("namespace"))?;
    let jobs_api: Api<Job> = Api::namespaced(client.clone(), &namespace);

    info!(job = %job_name, script = %script_name, "Executing runbook action");

    // Pass alert labels as environment variables, sanitizing names for shell compatibility.
    let mut env_vars: Vec<EnvVar> = alert.labels.iter()
        .map(|(k, v)| EnvVar {
            name: format!("ALERT_{}", k.to_uppercase().replace(|c: char| !c.is_ascii_alphanumeric(), "_")),
            value: Some(v.clone()),
            ..Default::default()
        })
        .collect();
    
    // Also pass annotations
    for (k, v) in &alert.annotations {
        env_vars.push(EnvVar {
            name: format!("ANNOTATION_{}", k.to_uppercase().replace(|c: char| !c.is_ascii_alphanumeric(), "_")),
            value: Some(v.clone()),
            ..Default::default()
        });
    }

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
                        image: Some("alpine:latest".to_string()),
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
                            name: Some("autoheal-runbooks".to_string()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }]),
                    ..Default::default()
                }),
                ..Default::default()
            },
            backoff_limit: Some(1),
            ttl_seconds_after_finished: Some(3600),
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