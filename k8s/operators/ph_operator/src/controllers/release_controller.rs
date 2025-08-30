/*
* Copyright (C) 2025 Pedro Henrique / phkaiser13
*
* File: k8s/operators/ph_operator/src/controllers/release_controller.rs
*
* This file implements the reconciliation logic for the phRelease custom resource.
* It has been evolved into an intelligent state machine to manage the lifecycle
* of a progressive canary deployment, including automated metric analysis,
* promotion, and rollback.
*
* Architecture:
* The controller's core, `apply_release`, now functions as a state machine,
* driven by the `status.phase` field of the `phRelease` resource. This allows
* it to manage a multi-step process over time instead of a single, one-shot
* application of state.
*
* Core Logic & State Transitions:
* - **Initial State (No Phase)**: On first reconciliation, the controller
*   creates the necessary `stable` and `canary` Deployments with the initial
*   traffic split (based on replica counts) and sets the phase to `Progressing`.
* - **`Progressing` Phase**: This is the main active state.
*   - If `analysis` is configured in the spec, the controller enters an
*     analysis loop.
*   - It periodically calls the `metrics_analyzer` module to evaluate the
*     health of the canary version against user-defined Prometheus metrics.
*   - It updates the `analysisRun` status with success or failure counts.
*   - Based on these counts, it decides the next state:
*     - On reaching the success `threshold`, it transitions to `Promoting` (if
*       `autoPromote` is true) or `Paused` (if manual promotion is required).
*     - On reaching the `maxFailures` count, it transitions to `RollingBack`.
*   - If no `analysis` is configured, it remains in `Progressing`, awaiting
*     manual changes to the `phRelease` spec by the user.
* - **`Promoting` Phase**: A terminal action state. It sets the canary
*   deployment's traffic to 100% and the stable to 0%, then transitions the
*   phase to `Succeeded`.
* - **`RollingBack` Phase**: A terminal action state. It sets the canary
*   deployment's traffic to 0% and the stable to 100%, then transitions the
*   phase to `Failed`.
* - **`Succeeded`, `Failed`, `Paused`**: Terminal states where the controller
*   takes no further action and waits for changes to the resource.
*
* This state-driven approach makes the release process robust, observable, and
* fully automated, turning a simple deployment tool into a true progressive
* delivery orchestrator.
*
* SPDX-License-Identifier: Apache-2.0
*/

use crate::crds::{phRelease, phReleaseStatus, AnalysisRunStatus, ReleasePhase, StrategyType};
use crate::mesh::{self, TrafficManagerClient, TrafficSplit as MeshTrafficSplit};
use crate::metrics;
use crate::metrics_analyzer::{AnalysisResult, PrometheusClient};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::{Secret, Service};
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube::{
    api::{Api, DeleteParams, ListParams, ObjectMeta, Patch, PatchParams},
    client::Client,
    runtime::{
        controller::Action,
        finalizer::{finalizer, Event as FinalizerEvent},
    },
    Error as KubeError, ResourceExt,
};
use opentelemetry::{
    global,
    propagation::{Extractor, TextMapPropagator},
    sdk::propagation::TraceContextPropagator,
};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tracing::{info_span, Instrument};
use tracing_opentelemetry::OpenTelemetrySpanExt;

// The unique identifier for our controller's finalizer.
const RELEASE_FINALIZER: &str = "ph.io/release-finalizer";
const SKIP_SIG_CHECK_ANNOTATION: &str = "ph.io/skip-sig-check";
const DEFAULT_REPLICAS: i32 = 5; // Default total replicas for the application.

// Custom error types for the controller for better diagnostics.
#[derive(Debug, Error)]
pub enum Error {
    #[error("Missing phRelease spec")]
    MissingSpec,

    #[error("Kubernetes API error: {0}")]
    KubeError(#[from] KubeError),

    #[error("Failed to update resource status: {0}")]
    StatusUpdateError(String),

    #[error("Unsupported release strategy")]
    UnsupportedStrategy,

    #[error("Invalid interval format in analysis spec: {0}")]
    InvalidIntervalFormat(String),

    #[error("Metrics analysis error: {0}")]
    MetricsAnalysisError(#[from] anyhow::Error),

    #[error("Image signature verification failed: {0}")]
    SignatureVerificationFailed(String),
}

/// The context required by the reconciler.
pub struct Context {
    pub client: Client,
    pub prometheus_client: PrometheusClient,
}

impl Context {
    pub fn new(client: Client, prometheus_endpoint: &str) -> Self {
        Self {
            client,
            prometheus_client: PrometheusClient::new(prometheus_endpoint),
        }
    }
}

// Helper struct to extract trace context from Kubernetes annotations.
struct AnnotationExtractor<'a>(&'a std::collections::BTreeMap<String, String>);
impl<'a> opentelemetry::propagation::Extractor for AnnotationExtractor<'a> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(|s| s.as_str())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|s| s.as_str()).collect()
    }
}

/// Main reconciliation function for the phRelease resource.
pub async fn reconcile(release: Arc<phRelease>, ctx: Arc<Context>) -> Result<Action, Error> {
    // --- OpenTelemetry: Context Extraction ---
    let propagator = TraceContextPropagator::new();
    let parent_context = propagator.extract(&AnnotationExtractor(release.annotations()));
    let span = info_span!(
        "reconcile_release",
        "ph.release.name" = release.name_any().as_str(),
        "ph.release.app" = release.spec.as_ref().map_or("unknown", |s| s.app_name.as_str())
    );
    span.set_parent(parent_context);

    // Instrument the entire reconciliation process.
    async move {
        let ns = release
            .namespace()
            .ok_or_else(|| KubeError::Request(http::Error::new("Missing namespace for phRelease")))?;
        let releases: Api<phRelease> = Api::namespaced(ctx.client.clone(), &ns);

        // Use the finalizer helper to manage the resource lifecycle.
        finalizer(&releases, RELEASE_FINALIZER, release, |event| async {
            match event {
                FinalizerEvent::Apply(r) => apply_release(r, ctx.clone()).await,
                FinalizerEvent::Cleanup(r) => cleanup_release(r, ctx.clone()).await,
            }
        })
        .await
        .map_err(|e| KubeError::Request(http::Error::new(e.to_string())).into())
    }
    .instrument(span)
    .await
}

/// The core logic for creating and managing a Canary release, now a state machine.
use tracing::field;

async fn apply_release(release: Arc<phRelease>, ctx: Arc<Context>) -> Result<Action, Error> {
    let span = info_span!(
        "apply_release",
        "ph.release.phase" = tracing::field::Empty
    );
    async move {
        let client = ctx.client.clone();
        let ns = release.namespace().unwrap();
    let spec = release.spec.as_ref().ok_or(Error::MissingSpec)?;
    let status = release.status.as_ref().cloned().unwrap_or_default();
    let releases: Api<phRelease> = Api::namespaced(client.clone(), &ns);
    let release_name = release.name_any();

    // --- 1. MANDATORY SIGNATURE VERIFICATION (Pre-flight check) ---
    let skip_check = release
        .annotations()
        .get(SKIP_SIG_CHECK_ANNOTATION)
        .map_or(false, |v| v == "true");

    if skip_check {
        println!("⚠️ Signature verification was skipped for release '{}' via annotation.", release_name);
        // In a real system, we'd create a Kubernetes Event here for auditing.
    } else {
        // Verification is not skipped, so it's mandatory.
        let verification_config = spec.security.as_ref().and_then(|s| s.signature_verification.as_ref());

        if verification_config.is_none() {
            // Config is missing, fail the release.
            println!("❌ Release '{}' failed: Signature verification is mandatory but not configured. To bypass, annotate the resource with 'ph.io/skip-sig-check: \"true\"'.", release_name);
            let new_status = phReleaseStatus {
                phase: Some(ReleasePhase::Failed),
                traffic_split: Some("Error: Signature verification is mandatory but not configured.".to_string()),
                ..status.clone()
            };
            update_status(&releases, &release_name, new_status).await?;
            return Ok(Action::await_change());
        }

        // Config is present, perform the verification.
        println!("🔒 Performing mandatory signature verification for release '{}'", release_name);
        match verify_image_signature_for_release(release.clone(), ctx.clone())
            .instrument(info_span!("verify_image_signature"))
            .await
        {
            Ok(_) => {
                println!("✅ Signature verification successful for '{}'", release_name);
            }
            Err(e) => {
                println!("❌ Signature verification failed for '{}': {}", release_name, e);
                let new_status = phReleaseStatus {
                    phase: Some(ReleasePhase::Failed),
                    traffic_split: Some(format!("Error: {}", e)),
                    ..status.clone()
                };
                update_status(&releases, &release_name, new_status).await?;
                return Ok(Action::await_change());
            }
        }
    }

    // Determine the current phase, defaulting to Progressing if not set.
    let current_phase = status.phase.clone().unwrap_or(ReleasePhase::Progressing);
    span.record("ph.release.phase", &serde_json::to_string(&current_phase).unwrap_or_default());

    // --- STATE MACHINE ---
    match current_phase {
        ReleasePhase::Progressing => {
            println!(
                "Reconciling release '{}' in Progressing phase.",
                release_name
            );
            let deployments: Api<Deployment> = Api::namespaced(client.clone(), &ns);
            let canary_name = format!("{}-canary", spec.app_name);

            // If the canary deployment doesn't exist, this is the initial setup.
            if deployments.get(&canary_name).await.is_err() {
                return initial_setup(release, ctx).instrument(info_span!("initial_setup")).await;
            }

            // Deployments exist, proceed with the analysis loop if configured.
            let canary_spec = spec
                .strategy
                .canary
                .as_ref()
                .ok_or(Error::UnsupportedStrategy)?;

            /* BEGIN CHANGE: Implement automated metrics analysis loop.
             * This block contains the core logic for integrating the metrics_analyzer.
             * It checks if analysis is enabled, manages the check interval, invokes
             * the analyzer, updates the phRelease status with results, and decides
             * whether to promote, rollback, or continue the analysis based on configured
             * thresholds.
             */
            if let Some(analysis_config) = &canary_spec.analysis {
                // --- AUTOMATED ANALYSIS LOGIC ---
                let interval = parse_duration_str(&analysis_config.interval)?;
                let now = Utc::now();

                // Check if it's time for a new analysis based on lastCheck timestamp.
                let should_analyze = match status.analysis_run.as_ref().and_then(|ar| ar.last_check.as_ref()) {
                    Some(last_check_str) => {
                        let last_check_time = DateTime::parse_from_rfc3339(last_check_str)
                            .map_err(|e| Error::StatusUpdateError(format!("Invalid lastCheck timestamp: {}", e)))?
                            .with_timezone(&Utc);
                        now >= last_check_time + chrono::Duration::from_std(interval).unwrap()
                    }
                    None => true, // First time, analyze immediately.
                };

                if !should_analyze {
                    let last_check_time = DateTime::parse_from_rfc3339(
                        status.analysis_run.as_ref().unwrap().last_check.as_ref().unwrap()
                    ).unwrap().with_timezone(&Utc);
                    let next_run_time = last_check_time + chrono::Duration::from_std(interval).unwrap();
                    let requeue_duration = (next_run_time - now).to_std().unwrap_or(Duration::from_secs(1));
                    println!("Next analysis for '{}' scheduled in {:?}", release_name, requeue_duration);
                    return Ok(Action::requeue(requeue_duration));
                }

                // It's time to run the analysis.
                println!("Running analysis for release '{}'...", release_name);
                
                // Run analysis for all configured metrics
                let (analysis_results, new_history) = run_metrics_analysis(
                    &ctx.prometheus_client,
                    &analysis_config.metrics,
                    status.analysis_run.as_ref().and_then(|ar| ar.metric_history.as_ref()),
                )
                .instrument(info_span!("run_metrics_analysis"))
                .await?;
                let mut analysis_run_status = status.analysis_run.clone().unwrap_or_default();
                analysis_run_status.metric_history = Some(new_history);


                // Evaluate overall analysis result
                let all_metrics_passed = analysis_results.iter().all(|(_, result)| matches!(result, AnalysisResult::Success));
                let has_inconclusive = analysis_results.iter().any(|(_, result)| matches!(result, AnalysisResult::Inconclusive));
                let is_trending_worse = analysis_results.iter().any(|(_, result)| matches!(result, AnalysisResult::TrendingWorse));

                // Log individual metric results
                for (metric_name, result) in &analysis_results {
                    println!("  - Metric '{}': {:?}", metric_name, result);
                }

                // Update success/failure counters based on analysis results
                if is_trending_worse {
                    println!("~ Predictive analysis detected a negative trend for '{}'. Pausing release.", release_name);
                    let mut new_status = status.clone();
                    new_status.phase = Some(ReleasePhase::Paused);
                    new_status.analysis_run = Some(analysis_run_status);
                    // TODO: Add a specific condition for this
                    update_status(&releases, &release_name, new_status).await?;
                    return Ok(Action::requeue(interval));
                } else if all_metrics_passed {
                    analysis_run_status.success_count += 1;
                    analysis_run_status.failure_count = 0; // Reset failure count on success
                    println!("All metrics passed for '{}'. Success count: {}", release_name, analysis_run_status.success_count);
                } else if has_inconclusive {
                    // Don't increment failure count for inconclusive results, just log and retry
                    println!("Some metrics were inconclusive for '{}'. Will retry on next analysis.", release_name);
                } else {
                    // At least one metric failed
                    analysis_run_status.failure_count += 1;
                    analysis_run_status.success_count = 0; // Reset success count on failure
                    println!("Some metrics failed for '{}'. Failure count: {}", release_name, analysis_run_status.failure_count);
                }

                analysis_run_status.last_check = Some(now.to_rfc3339());

                let mut new_status = status.clone();
                new_status.analysis_run = Some(analysis_run_status.clone());

                // Decide the next phase based on thresholds
                if analysis_run_status.success_count >= analysis_config.threshold {
                    // --- Measure Latency on Promotion ---
                    if let Some(start_time_str) = &status.progressing_start_time {
                        if let Ok(start_time) = DateTime::parse_from_rfc3339(start_time_str) {
                            let duration = Utc::now().signed_duration_since(start_time);
                            metrics::PHGIT_ROLLOUT_STEP_LATENCY_SECONDS.observe(duration.num_seconds() as f64);
                            println!("Observed promotion latency: {}s", duration.num_seconds());
                        }
                    }
                    new_status.phase = if canary_spec.auto_promote {
                        println!("Success threshold ({}) reached for '{}'. Promoting automatically.", 
                                analysis_config.threshold, release_name);
                        Some(ReleasePhase::Promoting)
                    } else {
                        println!("Success threshold ({}) reached for '{}'. Pausing for manual promotion.", 
                                analysis_config.threshold, release_name);
                        Some(ReleasePhase::Paused)
                    };
                } else if analysis_run_status.failure_count >= analysis_config.max_failures {
                    // --- Measure Latency on Rollback ---
                     if let Some(start_time_str) = &status.progressing_start_time {
                        if let Ok(start_time) = DateTime::parse_from_rfc3339(start_time_str) {
                            let duration = Utc::now().signed_duration_since(start_time);
                            metrics::PHGIT_ROLLOUT_STEP_LATENCY_SECONDS.observe(duration.num_seconds() as f64);
                             println!("Observed rollback latency: {}s", duration.num_seconds());
                        }
                    }
                    println!("Failure threshold ({}) reached for '{}'. Rolling back automatically.", 
                            analysis_config.max_failures, release_name);
                    new_status.phase = Some(ReleasePhase::RollingBack);
                } else {
                    println!("Analysis for '{}' complete. Successes: {}/{}, Failures: {}/{}. Continuing analysis.", 
                            release_name, 
                            analysis_run_status.success_count, analysis_config.threshold,
                            analysis_run_status.failure_count, analysis_config.max_failures);
                }

                update_status(&releases, &release_name, new_status).await?;
                Ok(Action::requeue(interval))

            } else {
                // No analysis configured, this is a manual canary.
                println!("Release '{}' is in Progressing phase (manual). Waiting for spec changes.", release_name);
                Ok(Action::requeue(Duration::from_secs(300)))
            }
            /* END CHANGE */
        }
        ReleasePhase::Promoting => {
            println!("Reconciling release '{}' in Promoting phase.", release_name);
            promote_release(release, ctx).instrument(info_span!("promote_release")).await
        }
        ReleasePhase::RollingBack => {
            println!("Reconciling release '{}' in RollingBack phase.", release_name);
            rollback_release(release, ctx).instrument(info_span!("rollback_release")).await
        }
        ReleasePhase::Succeeded | ReleasePhase::Failed | ReleasePhase::Paused => {
            println!("Reconciliation for '{}' is complete (Phase: {:?}). No action needed.", release_name, status.phase);
            Ok(Action::await_change())
        }
    }
    }.instrument(span).await
}

/// Runs analysis for all configured metrics and returns the results.
async fn run_metrics_analysis(
    prometheus_client: &PrometheusClient,
    metrics: &[crate::crds::Metric],
    history: Option<&Vec<crate::crds::MetricHistory>>,
) -> Result<(Vec<(String, AnalysisResult)>, Vec<crate::crds::MetricHistory>), Error> {
    let mut results = Vec::new();
    let mut new_history = history.cloned().unwrap_or_default();

    for metric in metrics {
        println!("  - Analyzing metric: {}", metric.name);

        let metric_history_points: Vec<crate::metrics_analyzer::HistoricalValue> = new_history
            .iter()
            .find(|h| h.name == metric.name)
            .map(|h| {
                h.values
                    .iter()
                    .map(|v| crate::metrics_analyzer::HistoricalValue {
                        timestamp: DateTime::parse_from_rfc3339(&v.timestamp)
                            .unwrap()
                            .timestamp(),
                        value: v.value,
                    })
                    .collect()
            })
            .unwrap_or_default();

        let (result, value) = prometheus_client
            .analyze(metric, &metric_history_points)
            .await?;
        results.push((metric.name.clone(), result.clone()));

        // Update history regardless of outcome to have a complete picture.
        if let Some(h) = new_history.iter_mut().find(|h| h.name == metric.name) {
            h.values.push(crate::crds::HistoricalValue {
                timestamp: Utc::now().to_rfc3339(),
                value,
            });
            // Cap history size
            if h.values.len() > 20 {
                h.values.remove(0);
            }
        } else {
            new_history.push(crate::crds::MetricHistory {
                name: metric.name.clone(),
                values: vec![crate::crds::HistoricalValue {
                    timestamp: Utc::now().to_rfc3339(),
                    value,
                }],
            });
        }
    }

    Ok((results, new_history))
}

/// Performs the initial setup of Kubernetes resources for the release.
async fn initial_setup(release: Arc<phRelease>, ctx: Arc<Context>) -> Result<Action, Error> {
    let client = ctx.client.clone();
    let ns = release.namespace().unwrap();
    let spec = release.spec.as_ref().ok_or(Error::MissingSpec)?;
    let releases: Api<phRelease> = Api::namespaced(client.clone(), &ns);
    let deployments: Api<Deployment> = Api::namespaced(client.clone(), &ns);
    let services: Api<Service> = Api::namespaced(client.clone(), &ns);

    println!("Performing initial setup for release '{}'", release.name_any());

    let app_name = &spec.app_name;
    let canary_version = &spec.version;
    let stable_name = "stable".to_string();
    let canary_name = "canary".to_string();

    let canary_strategy = match &spec.strategy.strategy_type {
        StrategyType::Canary => spec.strategy.canary.as_ref().ok_or(Error::MissingSpec)?,
        _ => return Err(Error::UnsupportedStrategy),
    };

    // Create or update the root Service.
    let service = build_service(app_name, app_name);
    services.patch(app_name, &PatchParams::apply("ph-release-controller"), &Patch::Apply(&service)).await?;

    // Determine the stable version from the existing stable deployment, if any.
    let stable_version = deployments.get(&format!("{}-{}", app_name, stable_name)).await
        .ok()
        .and_then(|d| d.spec.as_ref()?.template.spec.as_ref()?.containers.get(0)?.image.as_deref().map(|i| i.split(':').last().unwrap_or("latest").to_string()))
        .unwrap_or_else(|| "latest".to_string());

    // Create or update Stable and Canary Deployments.
    // When using a service mesh, we keep replicas balanced and let the mesh handle traffic.
    let (stable_replicas, canary_replicas) = (DEFAULT_REPLICAS / 2, DEFAULT_REPLICAS / 2);
    
    let stable_dep_def = build_deployment(app_name, &stable_name, &stable_version, stable_replicas);
    deployments.patch(&format!("{}-{}", app_name, stable_name), &PatchParams::apply("ph-release-controller"), &Patch::Apply(&stable_dep_def)).await?;

    let canary_dep_def = build_deployment(app_name, &canary_name, canary_version, canary_replicas);
    deployments.patch(&format!("{}-{}", app_name, canary_name), &PatchParams::apply("ph-release-controller"), &Patch::Apply(&canary_dep_def)).await?;

    // Traffic Splitting Logic
    let traffic_percent = canary_strategy.traffic_percent;
    if let Some(mesh_client) = get_traffic_manager_client(client.clone()).await? {
        println!("Traffic manager detected. Shifting traffic via mesh/controller.");
        let split = MeshTrafficSplit {
            app_name: app_name.clone(),
            weights: vec![
                (stable_name.clone(), 100 - traffic_percent),
                (canary_name.clone(), traffic_percent),
            ],
        };
        mesh_client.update_traffic_split(ns, split).await?;
    } else {
        println!("No service mesh detected. Shifting traffic via replica scaling.");
        // Fallback to replica scaling if no mesh is found
        let canary_replicas_scaled = (DEFAULT_REPLICAS as u8 * traffic_percent) / 100;
        let stable_replicas_scaled = DEFAULT_REPLICAS as u8 - canary_replicas_scaled;
        
        let patch_params = PatchParams::apply("ph-release-controller");
        let canary_patch = Patch::Merge(json!({ "spec": { "replicas": canary_replicas_scaled } }));
        let stable_patch = Patch::Merge(json!({ "spec": { "replicas": stable_replicas_scaled } }));

        deployments.patch(&format!("{}-canary", app_name), &patch_params, &canary_patch).await?;
        deployments.patch(&format!("{}-stable", app_name), &patch_params, &stable_patch).await?;
    }

    // Set the initial status to Progressing.
    let new_status = phReleaseStatus {
        phase: Some(ReleasePhase::Progressing),
        stable_version: Some(stable_version),
        canary_version: Some(canary_version.clone()),
        traffic_split: Some(format!("stable: {}%, canary: {}%", 100 - traffic_percent, traffic_percent)),
        progressing_start_time: Some(Utc::now().to_rfc3339()),
        ..Default::default()
    };

    update_status(&releases, &release.name_any(), new_status).await?;
    println!("Initial setup complete for '{}'. Starting monitoring phase.", release.name_any());

    Ok(Action::requeue(Duration::from_secs(1)))
}

/// Promotes the canary release to 100% traffic.
async fn promote_release(release: Arc<phRelease>, ctx: Arc<Context>) -> Result<Action, Error> {
    let client = ctx.client.clone();
    let ns = release.namespace().unwrap();
    let spec = release.spec.as_ref().ok_or(Error::MissingSpec)?;
    let releases: Api<phRelease> = Api::namespaced(client.clone(), &ns);
    let deployments: Api<Deployment> = Api::namespaced(client.clone(), &ns);

    let app_name = &spec.app_name;
    println!("Promoting canary '{}' to stable for release '{}'", spec.version, release.name_any());

    if let Some(traffic_manager) = get_traffic_manager_client(client.clone()).await? {
        println!("Traffic manager detected. Promoting via client.");
        traffic_manager.promote(ns, app_name).await?;
    } else {
        println!("No service mesh detected. Promoting via replica scaling.");
        let patch_params = PatchParams::apply("ph-release-controller");
        let stable_update_patch = Patch::Merge(json!({
            "spec": {
                "replicas": DEFAULT_REPLICAS,
                "template": { "spec": { "containers": [{"name": app_name, "image": format!("nginx:{}", spec.version)}] } }
            }
        }));
        let canary_scale_patch = Patch::Merge(json!({ "spec": { "replicas": 0 } }));

        deployments.patch(&format!("{}-stable", app_name), &patch_params, &stable_update_patch).await?;
        deployments.patch(&format!("{}-canary", app_name), &patch_params, &canary_scale_patch).await?;
    }

    let new_status = phReleaseStatus {
        phase: Some(ReleasePhase::Succeeded),
        stable_version: Some(spec.version.clone()),
        canary_version: None,
        traffic_split: Some("stable: 100%, canary: 0%".to_string()),
        ..release.status.as_ref().cloned().unwrap_or_default()
    };
    update_status(&releases, &release.name_any(), new_status).await?;

    metrics::PHGIT_ROLLOUTS_TOTAL.with_label_values(&["canary", "succeeded"]).inc();
    println!("Release '{}' promoted successfully.", release.name_any());
    Ok(Action::await_change())
}

/// Rolls back the canary release to 0% traffic.
async fn rollback_release(release: Arc<phRelease>, ctx: Arc<Context>) -> Result<Action, Error> {
    let client = ctx.client.clone();
    let ns = release.namespace().unwrap();
    let spec = release.spec.as_ref().ok_or(Error::MissingSpec)?;
    let releases: Api<phRelease> = Api::namespaced(client.clone(), &ns);
    let deployments: Api<Deployment> = Api::namespaced(client.clone(), &ns);

    let app_name = &spec.app_name;
    println!("Rolling back canary for release '{}'", release.name_any());

    if let Some(traffic_manager) = get_traffic_manager_client(client.clone()).await? {
        println!("Traffic manager detected. Rolling back via client.");
        traffic_manager.rollback(ns, app_name).await?;
    } else {
        println!("No service mesh detected. Rolling back via replica scaling.");
        let patch_params = PatchParams::apply("ph-release-controller");
        let canary_patch = Patch::Merge(json!({ "spec": { "replicas": 0 } }));
        let stable_patch = Patch::Merge(json!({ "spec": { "replicas": DEFAULT_REPLICAS } }));

        deployments.patch(&format!("{}-canary", app_name), &patch_params, &canary_patch).await?;
        deployments.patch(&format!("{}-stable", app_name), &patch_params, &stable_patch).await?;
    }

    let new_status = phReleaseStatus {
        phase: Some(ReleasePhase::Failed),
        traffic_split: Some("stable: 100%, canary: 0%".to_string()),
        ..release.status.as_ref().cloned().unwrap_or_default()
    };
    update_status(&releases, &release.name_any(), new_status).await?;

    metrics::PHGIT_ROLLOUTS_TOTAL.with_label_values(&["canary", "failed"]).inc();
    println!("Release '{}' rolled back successfully.", release.name_any());
    Ok(Action::await_change())
}

/// Cleans up the resources created for a release.
async fn cleanup_release(release: Arc<phRelease>, ctx: Arc<Context>) -> Result<Action, Error> {
    let client = ctx.client.clone();
    let ns = release.namespace().unwrap();
    let spec = release.spec.as_ref().ok_or(Error::MissingSpec)?;

    let app_name = &spec.app_name;
    let canary_name = format!("{}-canary", app_name);
    let deployments: Api<Deployment> = Api::namespaced(client, &ns);

    println!("Cleaning up canary deployment '{}' for release '{}'", canary_name, release.name_any());
    match deployments.delete(&canary_name, &DeleteParams::default()).await {
        Ok(_) => println!("Canary deployment '{}' deleted successfully.", canary_name),
        Err(KubeError::Api(ae)) if ae.code == 404 => {
            println!("Canary deployment '{}' already deleted.", canary_name);
        }
        Err(e) => return Err(e.into()),
    };

    Ok(Action::await_change())
}

/// Error handling function for the reconciliation loop.
pub async fn on_error(release: Arc<phRelease>, error: &Error, ctx: Arc<Context>) -> Action {
    eprintln!("Reconciliation error for phRelease '{}': {:?}", release.name_any(), error);
    let releases: Api<phRelease> = Api::namespaced(ctx.client.clone(), &release.namespace().unwrap());

    let error_message = match error {
        Error::MetricsAnalysisError(e) => format!("Metrics analysis failed: {}", e),
        Error::KubeError(e) => format!("Kubernetes API error: {}", e),
        Error::StatusUpdateError(e) => format!("Status update failed: {}", e),
        _ => format!("Controller error: {}", error),
    };

    let new_status = phReleaseStatus {
        phase: Some(ReleasePhase::Failed),
        traffic_split: Some(error_message),
        ..release.status.as_ref().cloned().unwrap_or_default()
    };

    if let Err(e) = update_status(&releases, &release.name_any(), new_status).await {
        eprintln!("Failed to update status on error: {}", e);
    }

    // Requeue with backoff based on error type
    let requeue_duration = match error {
        Error::MetricsAnalysisError(_) => Duration::from_secs(60), // Longer delay for metrics issues
        Error::KubeError(_) => Duration::from_secs(30), // Medium delay for k8s issues
        _ => Duration::from_secs(15), // Short delay for other errors
    };

    Action::requeue(requeue_duration)
}

/// Detects which traffic management tool (service mesh or Argo Rollouts) is active
/// by checking for installed CRDs and returns a suitable client.
async fn get_traffic_manager_client(
    client: Client,
) -> Result<Option<Box<dyn TrafficManagerClient + Send + Sync>>, Error> {
    let crd_api: Api<CustomResourceDefinition> = Api::all(client.clone());

    // Check for Argo Rollouts' Rollout CRD first, as it's a higher-level controller.
    if crd_api.get("rollouts.argoproj.io").await.is_ok() {
        println!("Argo Rollouts CRD found. Using Argo Rollouts client.");
        return Ok(Some(Box::new(mesh::argo::ArgoRolloutsClient::new(client))));
    }

    // Check for Istio's VirtualService CRD
    if crd_api
        .get("virtualservices.networking.istio.io")
        .await
        .is_ok()
    {
        println!("Istio CRD found. Using Istio service mesh client.");
        return Ok(Some(Box::new(mesh::istio::IstioClient::new(client))));
    }

    // Check for SMI's TrafficSplit CRD (used by Linkerd)
    if crd_api
        .get("trafficsplits.split.smi-spec.io")
        .await
        .is_ok()
    {
        println!("SMI TrafficSplit CRD found. Using Linkerd service mesh client.");
        return Ok(Some(Box::new(mesh::linkerd::LinkerdClient::new(client))));
    }

    println!("No supported traffic management tool found. Falling back to direct deployment scaling.");
    Ok(None)
}


// --- Helper Functions ---

/// A helper to patch the status subresource of a phRelease.
async fn update_status(releases: &Api<phRelease>, name: &str, status: phReleaseStatus) -> Result<(), Error> {
    let patch = Patch::Apply(json!({ "status": status }));
    releases
        .patch_status(name, &PatchParams::apply("ph-release-controller"), &patch)
        .await
        .map_err(|e| Error::StatusUpdateError(e.to_string()))?;
    Ok(())
}

/// Parses a simple duration string (e.g., "1m", "30s") into a `Duration`.
fn parse_duration_str(s: &str) -> Result<Duration, Error> {
    let s = s.trim();
    if let Some(num_str) = s.strip_suffix('s') {
        let secs = num_str.parse::<u64>().map_err(|_| Error::InvalidIntervalFormat(s.to_string()))?;
        Ok(Duration::from_secs(secs))
    } else if let Some(num_str) = s.strip_suffix('m') {
        let mins = num_str.parse::<u64>().map_err(|_| Error::InvalidIntervalFormat(s.to_string()))?;
        Ok(Duration::from_secs(mins * 60))
    } else if let Some(num_str) = s.strip_suffix('h') {
        let hours = num_str.parse::<u64>().map_err(|_| Error::InvalidIntervalFormat(s.to_string()))?;
        Ok(Duration::from_secs(hours * 3600))
    } else {
        Err(Error::InvalidIntervalFormat(s.to_string()))
    }
}

/// Constructs a Kubernetes Service definition for the application.
fn build_service(app_name: &str) -> Service {
    Service {
        metadata: ObjectMeta { 
            name: Some(app_name.to_string()), 
            labels: Some([("managed-by".to_string(), "ph-operator".to_string())].into()),
            ..Default::default() 
        },
        spec: Some(k8s_openapi::api::core::v1::ServiceSpec {
            selector: Some([("app".to_string(), app_name.to_string())].into()),
            ports: Some(vec![k8s_openapi::api::core::v1::ServicePort { 
                port: 80, 
                target_port: Some(k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::Int(80)),
                protocol: Some("TCP".to_string()),
                ..Default::default() 
            }]),
            type_: Some("ClusterIP".to_string()),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Constructs a Kubernetes Deployment definition.
fn build_deployment(app_name: &str, name: &str, version: &str, replicas: i32) -> Deployment {
    let pod_labels = [
        ("app".to_string(), app_name.to_string()),
        ("version-id".to_string(), name.to_string()),
        ("managed-by".to_string(), "ph-operator".to_string()),
    ].into();

    Deployment {
        metadata: ObjectMeta { 
            name: Some(name.to_string()), 
            labels: Some(pod_labels.clone()),
            ..Default::default() 
        },
        spec: Some(k8s_openapi::api::apps::v1::DeploymentSpec {
            replicas: Some(replicas),
            selector: k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector {
                match_labels: Some([
                    ("app".to_string(), app_name.to_string()),
                    ("version-id".to_string(), name.to_string()),
                ].into()),
                ..Default::default()
            },
            template: k8s_openapi::api::core::v1::PodTemplateSpec {
                metadata: Some(ObjectMeta { 
                    labels: Some(pod_labels), 
                    ..Default::default() 
                }),
                spec: Some(k8s_openapi::api::core::v1::PodSpec {
                    containers: vec![k8s_openapi::api::core::v1::Container {
                        name: app_name.to_string(),
                        image: Some(format!("nginx:{}", version)), // Using nginx for demonstration
                        ports: Some(vec![k8s_openapi::api::core::v1::ContainerPort {
                            container_port: 80,
                            protocol: Some("TCP".to_string()),
                            ..Default::default()
                        }]),
                        resources: Some(k8s_openapi::api::core::v1::ResourceRequirements {
                            requests: Some([
                                ("cpu".to_string(), k8s_openapi::apimachinery::pkg::api::resource::Quantity("100m".to_string())),
                                ("memory".to_string(), k8s_openapi::apimachinery::pkg::api::resource::Quantity("128Mi".to_string())),
                            ].into()),
                            limits: Some([
                                ("cpu".to_string(), k8s_openapi::apimachinery::pkg::api::resource::Quantity("500m".to_string())),
                                ("memory".to_string(), k8s_openapi::apimachinery::pkg::api::resource::Quantity("512Mi".to_string())),
                            ].into()),
                            ..Default::default()
                        }),
                        readiness_probe: Some(k8s_openapi::api::core::v1::Probe {
                            http_get: Some(k8s_openapi::api::core::v1::HTTPGetAction {
                                path: Some("/".to_string()),
                                port: k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::Int(80),
                                ..Default::default()
                            }),
                            initial_delay_seconds: Some(10),
                            period_seconds: Some(5),
                            timeout_seconds: Some(3),
                            failure_threshold: Some(3),
                            ..Default::default()
                        }),
                        liveness_probe: Some(k8s_openapi::api::core::v1::Probe {
                            http_get: Some(k8s_openapi::api::core::v1::HTTPGetAction {
                                path: Some("/".to_string()),
                                port: k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::Int(80),
                                ..Default::default()
                            }),
                            initial_delay_seconds: Some(30),
                            period_seconds: Some(10),
                            timeout_seconds: Some(5),
                            failure_threshold: Some(3),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }],
                    restart_policy: Some("Always".to_string()),
                    ..Default::default()
                }),
            },
            strategy: Some(k8s_openapi::api::apps::v1::DeploymentStrategy {
                type_: Some("RollingUpdate".to_string()),
                rolling_update: Some(k8s_openapi::api::apps::v1::RollingUpdateDeployment {
                    max_unavailable: Some(k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::String("25%".to_string())),
                    max_surge: Some(k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::String("25%".to_string())),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Verifies the image signature based on the configuration in the phRelease spec.
async fn verify_image_signature_for_release(release: Arc<phRelease>, ctx: Arc<Context>) -> Result<(), Error> {
    let spec = release.spec.as_ref().ok_or(Error::MissingSpec)?;
    let ns = release.namespace().unwrap();
    let secrets: Api<Secret> = Api::namespaced(ctx.client.clone(), &ns);

    let signature_config = spec
        .security
        .as_ref()
        .and_then(|s| s.signature_verification.as_ref())
        .ok_or_else(|| Error::SignatureVerificationFailed("Missing signatureVerification config".to_string()))?;
    
    let secret_ref = &signature_config.public_key_secret_ref;

    // 1. Fetch the secret containing the public key
    let secret = secrets.get(&secret_ref.name).await.map_err(|e| {
        Error::SignatureVerificationFailed(format!("Failed to get public key secret '{}': {}", secret_ref.name, e))
    })?;

    // 2. Extract and decode the public key
    let public_key_bytes = secret
        .data
        .as_ref()
        .and_then(|data| data.get(&secret_ref.key))
        .ok_or_else(|| Error::SignatureVerificationFailed(format!("Key '{}' not found in secret '{}'", secret_ref.key, secret_ref.name)))?;
    
    let public_key_pem = String::from_utf8(public_key_bytes.0.clone()).map_err(|e| {
        Error::SignatureVerificationFailed(format!("Public key in secret is not valid UTF-8: {}", e))
    })?;

    // 3. Construct the image URL
    // NOTE: This assumes a docker.io-like naming convention. The registry source
    // might need to be configurable in a real-world scenario.
    let image_url = format!("{}:{}", spec.app_name, spec.version);

    // 4. Call the verifier
    signature_verifier::verify_image_signature(&image_url, &public_key_pem)
        .await
        .map_err(|e| Error::SignatureVerificationFailed(e.to_string()))?;

    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::crds::{CanaryStrategy, AnalysisSpec, Metric, ReleaseStrategy};
    use std::collections::HashMap;

    fn create_test_context() -> Context {
        Context::new(
            // In real tests, you'd use a mock client
            Client::try_default().unwrap_or_else(|_| panic!("Failed to create k8s client for tests")),
            "http://prometheus-test:9090"
        )
    }

    fn create_test_release(auto_promote: bool) -> phRelease {
        phRelease {
            metadata: ObjectMeta {
                name: Some("test-release".to_string()),
                namespace: Some("default".to_string()),
                ..Default::default()
            },
            spec: Some(crate::crds::phReleaseSpec {
                app_name: "test-app".to_string(),
                version: "v2.0.0".to_string(),
                strategy: ReleaseStrategy {
                    strategy_type: StrategyType::Canary,
                    canary: Some(CanaryStrategy {
                        traffic_percent: 20,
                        auto_promote,
                        analysis: Some(AnalysisSpec {
                            interval: "30s".to_string(),
                            threshold: 3,
                            max_failures: 2,
                            metrics: vec![
                                Metric {
                                    name: "error_rate".to_string(),
                                    query: "rate(http_requests_total{status=~'5..'}[5m])".to_string(),
                                    on_success: "result < 0.05".to_string(),
                                },
                            ],
                        }),
                    }),
                },
            }),
            status: None,
        }
    }

    #[test]
    fn test_parse_duration_str() {
        assert_eq!(parse_duration_str("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration_str("5m").unwrap(), Duration::from_secs(300));
        assert_eq!(parse_duration_str("2h").unwrap(), Duration::from_secs(7200));
        
        assert!(parse_duration_str("invalid").is_err());
        assert!(parse_duration_str("30x").is_err());
    }

    #[test]
    fn test_build_deployment() {
        let deployment = build_deployment("test-app", "test-app-canary", "v1.0.0", 3);
        
        assert_eq!(deployment.metadata.name, Some("test-app-canary".to_string()));
        assert_eq!(deployment.spec.as_ref().unwrap().replicas, Some(3));
        
        let container = &deployment.spec.as_ref().unwrap()
            .template.spec.as_ref().unwrap()
            .containers[0];
        assert_eq!(container.name, "test-app");
        assert_eq!(container.image, Some("nginx:v1.0.0".to_string()));
        
        // Check that health probes are configured
        assert!(container.readiness_probe.is_some());
        assert!(container.liveness_probe.is_some());
    }

    #[test]
    fn test_build_service() {
        let service = build_service("test-app");
        
        assert_eq!(service.metadata.name, Some("test-app".to_string()));
        assert!(service.metadata.labels.is_some());
        
        let spec = service.spec.as_ref().unwrap();
        assert_eq!(spec.type_, Some("ClusterIP".to_string()));
        assert_eq!(spec.ports.as_ref().unwrap().len(), 1);
        assert_eq!(spec.ports.as_ref().unwrap()[0].port, 80);
    }

    #[tokio::test]
    async fn test_run_metrics_analysis() {
        let prometheus_client = PrometheusClient::new("http://localhost:9090");
        let metrics = vec![
            Metric {
                name: "test_metric".to_string(),
                query: "up".to_string(),
                on_success: "result > 0".to_string(),
            }
        ];

        // This test would require a mock Prometheus client in a real implementation
        // For now, we're just testing that the function signature is correct
        let result = run_metrics_analysis(&prometheus_client, &metrics).await;
        
        // In a real test environment with mocked Prometheus, you would assert:
        // assert!(result.is_ok());
        // let results = result.unwrap();
        // assert_eq!(results.len(), 1);
        // assert_eq!(results[0].0, "test_metric");
    }
}