/*
 * Copyright (C) 2025 Pedro Henrique / phkaiser13
 *
 * File: pipeline_controller.rs
 *
 * This file implements the reconciliation logic for the phPipeline custom resource.
 * It functions as a resilient, state-driven orchestrator for CI/CD pipelines
 * defined declaratively within the Kubernetes cluster. The controller's primary
 * responsibility is to execute a sequence of stages and steps, represented as
 * Kubernetes Jobs, ensuring sequential execution and accurate status reporting.
 *
 * Architecture:
 * The controller's architecture is a refined state machine, where the state of the
 * pipeline is authoritatively stored within the `.status` subresource of the
 * phPipeline object itself. This makes the operator robust against restarts,
 * as it can resume a pipeline from where it left off. This version introduces a
 * finalizer to ensure that any external cleanup logic can be reliably executed
 * before the pipeline resource is deleted.
 *
 * Core Logic:
 * - `reconcile`: The primary entry point for the reconciliation loop. It uses the
 * `kube-rs` finalizer helper to delegate to either the `apply_pipeline` or
 * `cleanup_pipeline` functions.
 * - `apply_pipeline`: This function acts as the main dispatcher for the state machine.
 * It inspects the `status.phase` field and takes action accordingly:
 * - `Pending` (or None): Initializes the pipeline. It sets the status to `Running`,
 * records the start time, and sets the current stage/step indices to 0. This
 * is the initial state for any new pipeline.
 * - `Running`: The core processing state. It identifies the current step, creates
 * a Kubernetes Job for it if one doesn't exist, and then monitors the Job's
 * status. Upon Job success, it advances the step/stage indices. On failure,
 * it transitions the pipeline to the `Failed` phase.
 * - `Succeeded` / `Failed`: These are terminal states. No further action is taken,
 * and the reconciliation loop effectively stops for this resource until it is
 * updated or deleted.
 * - `handle_running_pipeline`: A dedicated function to manage the logic when a pipeline
 * is in the `Running` phase. It encapsulates Job creation, monitoring, and state
 * advancement logic, keeping `apply_pipeline` clean.
 * - `cleanup_pipeline`: This function is called by the finalizer logic when the
 * phPipeline resource is marked for deletion. While `ownerReferences` on Jobs
 * handle the primary garbage collection, this hook is preserved for any future,
 * more complex cleanup needs (e.g., sending notifications, cleaning external
 * resources).
 * - `update_status`: A robust, centralized function for patching the status
 * subresource using a server-side `Patch::Apply`. This is the modern, preferred
 * way to update status, preventing race conditions.
 * - Error Handling (`on_error`): If any step in the reconciliation fails, this
 * function is invoked. It patches the pipeline's status to `Failed` and provides
 * a clear error message, ensuring high observability for the user.
 *
 * SPDX-License-Identifier: Apache-2.0
 */

use chrono::Utc;
use k8s_openapi::api::batch::v1::Job;
use kube::{
    api::{Api, Patch, PatchParams, PostParams, ResourceExt},
    client::Client,
    runtime::{
        controller::Action,
        finalizer::{finalizer, Event as FinalizerEvent},
    },
    Error as KubeError,
};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

use crate::crds::{phPipeline, phPipelineStatus, PipelinePhase};

// The unique identifier for our controller's finalizer.
const PIPELINE_FINALIZER: &str = "ph.io/pipeline-finalizer";

// Custom error types for the pipeline controller.
#[derive(Debug, Error)]
pub enum Error {
    #[error("Kubernetes API error: {0}")]
    KubeError(#[from] KubeError),

    #[error("Missing phPipeline spec")]
    MissingSpec,

    #[error("Failed to update resource status: {0}")]
    StatusUpdateError(String),
}

/// The context required by the reconciler.
pub struct Context {
    pub client: Client,
}

/// Main reconciliation function for the phPipeline resource.
pub async fn reconcile(pipeline: Arc<phPipeline>, ctx: Arc<Context>) -> Result<Action, Error> {
    let ns = pipeline.namespace().ok_or_else(|| {
        KubeError::Request(http::Error::new("Missing namespace for phPipeline"))
    })?;
    let pipelines: Api<phPipeline> = Api::namespaced(ctx.client.clone(), &ns);

    finalizer(
        &pipelines,
        PIPELINE_FINALIZER,
        pipeline,
        |event| async {
            match event {
                FinalizerEvent::Apply(p) => apply_pipeline(p, ctx.clone()).await,
                FinalizerEvent::Cleanup(p) => cleanup_pipeline(p, ctx.clone()).await,
            }
        },
    )
        .await
        .map_err(|e| KubeError::Request(http::Error::new(e.to_string())).into())
}

/// The "apply" branch of the reconciliation loop, acting as a state machine dispatcher.
async fn apply_pipeline(pipeline: Arc<phPipeline>, ctx: Arc<Context>) -> Result<Action, Error> {
    let phase = pipeline.status.as_ref().and_then(|s| s.phase.clone());

    match phase {
        // State: Pending/None. This is a new pipeline. Let's initialize it.
        None | Some(PipelinePhase::Pending) => {
            println!("Pipeline '{}' is new. Initializing status.", pipeline.name_any());
            let initial_status = phPipelineStatus {
                phase: Some(PipelinePhase::Running),
                start_time: Some(Utc::now().to_rfc3339()),
                current_stage_index: Some(0),
                current_step_index: Some(0),
                ..Default::default()
            };
            update_status(pipeline, ctx.client.clone(), initial_status).await?;
            // Requeue immediately to begin the first step.
            Ok(Action::requeue(Duration::from_millis(100)))
        }
        // State: Running. This is where the core logic happens.
        Some(PipelinePhase::Running) => handle_running_pipeline(pipeline, ctx).await,
        // Terminal states: Do nothing further.
        Some(PipelinePhase::Succeeded) | Some(PipelinePhase::Failed) => {
            Ok(Action::await_change())
        }
    }
}

/// Handles the core logic when a pipeline is in the "Running" state.
async fn handle_running_pipeline(pipeline: Arc<phPipeline>, ctx: Arc<Context>) -> Result<Action, Error> {
    let client = &ctx.client;
    let ns = pipeline.namespace().unwrap();
    let spec = pipeline.spec.as_ref().ok_or(Error::MissingSpec)?;
    let mut status = pipeline.status.as_ref().cloned().unwrap_or_default(); // Should always exist here.

    let stage_index = status.current_stage_index.unwrap_or(0);
    let step_index = status.current_step_index.unwrap_or(0);

    // --- 1. Check for pipeline completion ---
    if stage_index >= spec.stages.len() {
        println!("Pipeline '{}' completed successfully.", pipeline.name_any());
        status.phase = Some(PipelinePhase::Succeeded);
        status.completion_time = Some(Utc::now().to_rfc3339());
        update_status(pipeline, client.clone(), status).await?;
        return Ok(Action::await_change());
    }

    // --- 2. Identify current step and construct Job name ---
    let current_stage = &spec.stages[stage_index];
    let current_step = &current_stage.steps[step_index];
    let job_name = format!(
        "{}-s{}-{}",
        pipeline.name_any(),
        stage_index,
        current_step.name.replace('_', "-")
    );

    // --- 3. Find or create the Job for the current step ---
    let jobs: Api<Job> = Api::namespaced(client.clone(), &ns);
    match jobs.get(&job_name).await {
        // Job exists, let's check its status.
        Ok(job) => {
            if let Some(job_status) = job.status {
                if job_status.succeeded.unwrap_or(0) > 0 {
                    // --- Job Succeeded: Advance to next step/stage ---
                    println!("Job '{}' succeeded for pipeline '{}'.", job_name, pipeline.name_any());
                    if step_index + 1 >= current_stage.steps.len() {
                        status.current_stage_index = Some(stage_index + 1);
                        status.current_step_index = Some(0);
                    } else {
                        status.current_step_index = Some(step_index + 1);
                    }
                    update_status(pipeline, client.clone(), status).await?;
                    Ok(Action::requeue(Duration::from_millis(100))) // Requeue immediately for next step.

                } else if job_status.failed.unwrap_or(0) > 0 {
                    // --- Job Failed: Terminate the pipeline ---
                    eprintln!("Job '{}' failed for pipeline '{}'.", job_name, pipeline.name_any());
                    status.phase = Some(PipelinePhase::Failed);
                    status.completion_time = Some(Utc::now().to_rfc3339());
                    update_status(pipeline, client.clone(), status).await?;
                    Ok(Action::await_change())

                } else {
                    // --- Job still running: Check again later ---
                    println!("Job '{}' is still running for pipeline '{}'.", job_name, pipeline.name_any());
                    Ok(Action::requeue(Duration::from_secs(15)))
                }
            } else {
                // Job status not yet available.
                Ok(Action::requeue(Duration::from_secs(5)))
            }
        }
        // Job does not exist, create it.
        Err(KubeError::Api(e)) if e.code == 404 => {
            println!("Creating Job '{}' for pipeline '{}'", job_name, pipeline.name_any());
            let job_def = create_job_for_step(&pipeline, &job_name, current_step)?;
            jobs.create(&PostParams::default(), &job_def).await?;
            Ok(Action::requeue(Duration::from_secs(10)))
        }
        // Another Kubernetes API error occurred.
        Err(e) => Err(e.into()),
    }
}

/// Constructs a Kubernetes Job object for a specific pipeline step.
fn create_job_for_step(pipeline: &phPipeline, job_name: &str, step: &crate::crds::PipelineStep) -> Result<Job, Error> {
    // Check for specialized step types
    if let Some(step_type) = &step.step_type {
        if step_type == "generate-sbom" {
            println!("Step '{}' is of type 'generate-sbom'. Creating specialized Job.", step.name);
            // This is a placeholder for a more complex logic that would find the image
            // from a previous step. For now, we use a placeholder image to scan.
            let image_to_scan = "nginx:latest";
            let output_path = "/artifacts/sbom.json";

            let job_json = json!({
                "apiVersion": "batch/v1",
                "kind": "Job",
                "metadata": {
                    "name": job_name,
                    "ownerReferences": [{
                        "apiVersion": "ph.io/v1alpha1",
                        "kind": "phPipeline",
                        "name": pipeline.name_any(),
                        "uid": pipeline.uid().ok_or_else(|| KubeError::Request(http::Error::new("Missing UID")))?,
                        "controller": true,
                    }]
                },
                "spec": {
                    "template": {
                        "spec": {
                            "containers": [{
                                "name": "sbom-generator",
                                "image": "anchore/syft:latest",
                                "command": ["syft", "packages", image_to_scan, "-o", "spdx-json", "--file", output_path],
                                "volumeMounts": [{
                                    "name": "artifacts",
                                    "mountPath": "/artifacts"
                                }]
                            }],
                            "volumes": [{
                                "name": "artifacts",
                                "emptyDir": {}
                            }],
                            "restartPolicy": "Never"
                        }
                    },
                    "backoffLimit": 1
                }
            });
            return serde_json::from_value(job_json).map_err(|e| Error::KubeError(KubeError::SerdeError(e)));
        }
    }

    // Default behavior for generic steps
    let job_json = json!({
        "apiVersion": "batch/v1",
        "kind": "Job",
        "metadata": {
            "name": job_name,
            "ownerReferences": [{
                "apiVersion": "ph.io/v1alpha1",
                "kind": "phPipeline",
                "name": pipeline.name_any(),
                "uid": pipeline.uid().ok_or_else(|| KubeError::Request(http::Error::new("Missing UID")))?,
                "controller": true,
            }]
        },
        "spec": {
            "template": {
                "spec": {
                    "containers": [{
                        "name": step.name.clone(),
                        "image": step.image.clone(),
                        "command": step.command,
                        "args": step.args,
                    }],
                    "restartPolicy": "Never"
                }
            },
            "backoffLimit": 1 // Do not retry failed jobs within the pipeline.
        }
    });

    serde_json::from_value(job_json).map_err(|e| Error::KubeError(KubeError::SerdeError(e)))
}


/// The "cleanup" branch of the reconciliation loop.
async fn cleanup_pipeline(pipeline: Arc<phPipeline>, _ctx: Arc<Context>) -> Result<Action, Error> {
    println!("Pipeline '{}' deleted. Associated Jobs will be garbage-collected.", pipeline.name_any());
    // With `ownerReferences` on the Jobs, Kubernetes handles garbage collection automatically.
    // This function serves as a hook for any additional, non-Kubernetes cleanup logic
    // that might be needed in the future (e.g., notifying an external system).
    Ok(Action::await_change())
}

/// Patches the status subresource of the phPipeline using Server-Side Apply.
async fn update_status(pipeline: Arc<phPipeline>, client: Client, status: phPipelineStatus) -> Result<(), Error> {
    let ns = pipeline.namespace().unwrap();
    let name = pipeline.name_any();
    let pipelines: Api<phPipeline> = Api::namespaced(client, &ns);

    let patch = Patch::Apply(json!({
        "apiVersion": "ph.io/v1alpha1",
        "kind": "phPipeline",
        "status": status,
    }));
    let ps = PatchParams::apply("ph-pipeline-controller").force();

    pipelines
        .patch_status(&name, &ps, &patch)
        .await
        .map_err(|e| Error::StatusUpdateError(e.to_string()))?;

    Ok(())
}

/// Error handling function for the reconciliation loop.
pub async fn on_error(pipeline: Arc<phPipeline>, error: &Error, ctx: Arc<Context>) -> Action {
    eprintln!("Reconciliation error for phPipeline '{}': {:?}", pipeline.name_any(), error);

    let failed_status = phPipelineStatus {
        phase: Some(PipelinePhase::Failed),
        completion_time: Some(Utc::now().to_rfc3339()),
        ..pipeline.status.clone().unwrap_or_default()
    };

    if let Err(e) = update_status(pipeline, ctx.client.clone(), failed_status).await {
        eprintln!("Failed to update status on error: {}", e);
    }

    // Requeue with a backoff.
    Action::requeue(Duration::from_secs(30))
}