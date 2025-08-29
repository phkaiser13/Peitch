/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* Archive: k8s/operators/ph_operator/src/main.rs
*
* This file is the main entry point for the ph Kubernetes Operator. It is
* responsible for setting up and running the controller manager, which in turn
* hosts and executes the reconciliation loops for all custom resources managed
* by this operator.
*
* Architecture:
* The program follows the standard `kube-rs` operator structure.
* 1.  **Initialization**: It begins by initializing a Kubernetes client and
* setting up `tracing` for structured logging.
* 2.  **CRD Registration**: The `main` function discovers all Custom
* Resource Definitions (CRDs) that this operator manages: `phPreview`,
* `phRelease`, `phPipeline`, and the new `phAutoHealRule`.
* 3.  **Controller Manager**:
*     - For `phPreview`, `phRelease`, and `phPipeline`, a standard `Controller`
*       from `kube-rs` is instantiated to manage the watch and reconcile loop.
*     - For `phAutoHealRule`, a dedicated `run` function is called. This function
*       encapsulates both the CRD reconciler (for managing an in-memory cache)
*       and an embedded HTTP webhook server (for receiving alerts from Alertmanager).
* 4.  **Shared Context**: A shared `Context` object, containing the Kubernetes
* client, is created for the traditional controllers. The auto-heal controller
* manages its own state internally.
* 5.  **Concurrent Execution**: All controller tasks are run concurrently using
* `tokio::join!`. This allows the operator to handle events for all resource
* types simultaneously and independently, making the system highly responsive
* and scalable.
*
* This top-level orchestration ensures that each piece of the operator's logic
* is properly initialized and executed within the asynchronous `tokio` runtime,
* forming a complete, production-grade Kubernetes operator.
*
* SPDX-License-Identifier: Apache-2.0 */

use futures::stream::StreamExt;
use kube::Client;
use kube_runtime::Controller;
use opentelemetry::global;
use opentelemetry_jaeger::Uninstall;
use prometheus::{Encoder, Registry, TextEncoder};
use std::sync::Arc;
use tokio;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};
use warp::Filter;

// Import the CRDs and controller modules.
mod crds;
mod metrics;
mod controllers {
    pub mod autoheal_controller; // New controller for auto-healing logic
    pub mod dr_controller;
    pub mod pipeline_controller;
    pub mod preview_controller;
    pub mod release_controller;
}

// Re-exporting the CRDs for easier access.
use crds::{phAutoHealRule, phPipeline, phPreview, phRelease, PhgitDisasterRecovery};

// The shared context struct passed to the traditional controllers.
pub struct Context {
    pub client: Client,
    // Add the prometheus client to the shared context
    pub prometheus_client: controllers::metrics_analyzer::PrometheusClient,
}

/// Initializes the OpenTelemetry pipeline for Jaeger.
fn init_telemetry() -> Result<Uninstall, Box<dyn std::error::Error>> {
    let tracer = opentelemetry_jaeger::new_agent_pipeline()
        .with_service_name("ph-operator")
        .install_batch(opentelemetry::runtime::Tokio)?;

    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let fmt_layer = tracing_subscriber::fmt::layer().json();

    tracing_subscriber::registry()
        .with(filter)
        .with(telemetry)
        .with(fmt_layer)
        .try_init()?;

    Ok(global::shutdown_tracer_provider)
}

/// Renders the metrics into the Prometheus text format.
async fn metrics_handler(registry: Arc<Registry>) -> Result<impl warp::Reply, warp::Rejection> {
    let encoder = TextEncoder::new();
    let mut buffer = vec![];
    encoder
        .encode(&registry.gather(), &mut buffer)
        .expect("Failed to encode metrics");

    let response = String::from_utf8(buffer.clone()).expect("Failed to convert metrics to string");
    Ok(warp::reply::with_header(
        response,
        "Content-Type",
        encoder.format_type(),
    ))
}

/// Runs the HTTP server to expose the /metrics endpoint.
async fn run_metrics_server(registry: Arc<Registry>) {
    let metrics_route = warp::path("metrics")
        .and(warp::get())
        .and(warp::any().map(move || Arc::clone(&registry)))
        .and_then(metrics_handler);

    info!("Starting metrics server on 0.0.0.0:9090");
    warp::serve(metrics_route).run(([0, 0, 0, 0], 9090)).await;
}

/// The main entry point of the operator.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Initialize Telemetry and Logging
    let shutdown_tracer = init_telemetry()?;
    info!("Telemetry initialized.");

    // 2. Initialize Kubernetes Client
    let client = Client::try_default().await?;

    // 2. Create APIs for our Custom Resources
    let previews = kube::Api::<phPreview>::all(client.clone());
    let releases = kube::Api::<phRelease>::all(client.clone());
    let pipelines = kube::Api::<phPipeline>::all(client.clone());
    let dr_resources = kube::Api::<PhgitDisasterRecovery>::all(client.clone());
    
    // 3. Create the shared context for traditional controllers
    // This includes initializing the Prometheus client.
    let prometheus_endpoint = std::env::var("PROMETHEUS_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:9090".to_string());
    
    let context = Arc::new(Context {
        client: client.clone(),
        prometheus_client: controllers::metrics_analyzer::PrometheusClient::new(&prometheus_endpoint),
    });

    // 4. Initialize metrics registry
    let registry = Arc::new(metrics::create_and_register_metrics()?);
    info!("Custom metrics registered.");

    info!("ph Operator starting...");

    // 5. Set up and run the controllers and metrics server concurrently
    tokio::join!(
        // --- Metrics Server ---
        run_metrics_server(registry.clone()),

        // --- Auto-Heal Controller and Webhook Server ---
        controllers::autoheal_controller::run(client.clone()),

        // --- Preview Controller ---
        Controller::new(previews, Default::default())
            .run(
                controllers::preview_controller::reconcile,
                controllers::preview_controller::on_error,
                context.clone(),
            )
            .for_each(|res| async move {
                match res {
                    Ok(o) => info!("Reconciled phPreview: {:?}", o),
                    Err(e) => tracing::error!("phPreview reconcile error: {}", e),
                }
            }),

        // --- Release Controller ---
        Controller::new(releases, Default::default())
            .run(
                controllers::release_controller::reconcile,
                controllers::release_controller::on_error,
                context.clone(),
            )
            .for_each(|res| async move {
                match res {
                    Ok(o) => info!("Reconciled phRelease: {:?}", o),
                    Err(e) => tracing::error!("phRelease reconcile error: {}", e),
                }
            }),

        // --- Pipeline Controller ---
        Controller::new(pipelines, Default::default())
            .run(
                controllers::pipeline_controller::reconcile,
                controllers::pipeline_controller::on_error,
                context.clone(),
            )
            .for_each(|res| async move {
                match res {
                    Ok(o) => info!("Reconciled phPipeline: {:?}", o),
                    Err(e) => tracing::error!("phPipeline reconcile error: {}", e),
                }
            }),

        // --- DR Controller ---
        Controller::new(dr_resources, Default::default())
            .run(
                controllers::dr_controller::reconcile,
                controllers::dr_controller::on_error,
                context.clone(),
            )
            .for_each(|res| async move {
                match res {
                    Ok(o) => info!("Reconciled PhgitDisasterRecovery: {:?}", o),
                    Err(e) => tracing::error!("PhgitDisasterRecovery reconcile error: {}", e),
                }
            })
    );

    info!("ph Operator shutting down.");
    
    // Shutdown the tracer provider.
    global::shutdown_tracer_provider();

    Ok(())
}