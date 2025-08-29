/*
* Copyright (C) 2025 Pedro Henrique / phkaiser13
*
* File: k8s/operators/ph_operator/src/metrics.rs
*
* This module defines and registers the custom Prometheus metrics that the
* ph-operator exposes. These metrics provide insights into the operator's
* behavior and the lifecycle of the resources it manages.
*
* Using `lazy_static`, we ensure that the metrics are created only once and are
* available globally and safely across all concurrent reconciliation loops and
* web server threads.
*
* SPDX-License-Identifier: Apache-2.0
*/

use lazy_static::lazy_static;
use prometheus::{
    opts, register_histogram, register_int_counter, register_int_counter_vec, register_int_gauge,
    Histogram, IntCounter, IntCounterVec, IntGauge, Registry,
};

// --- Metric Definitions ---

lazy_static! {
    /// A counter for the total number of PhgitPreview resources successfully created.
    pub static ref PHGIT_PREVIEW_CREATED_TOTAL: IntCounter =
        register_int_counter!(opts!(
            "phgit_preview_created_total",
            "Total number of PhgitPreview resources successfully created."
        )).unwrap();

    /// A gauge that shows the current number of active PhgitPreview resources.
    pub static ref PHGIT_PREVIEW_ACTIVE: IntGauge =
        register_int_gauge!(opts!(
            "phgit_preview_active",
            "Current number of active PhgitPreview resources."
        )).unwrap();

    /// A counter for the total number of rollouts, labeled by strategy and status.
    pub static ref PHGIT_ROLLOUTS_TOTAL: IntCounterVec =
        register_int_counter_vec!(
            "phgit_rollouts_total",
            "Total number of rollouts.",
            &["strategy", "status"]
        ).unwrap();

    /// A histogram that measures the latency of each step in a canary rollout.
    /// The buckets are defined in seconds.
    pub static ref PHGIT_ROLLOUT_STEP_LATENCY_SECONDS: Histogram =
        register_histogram!(
            "phgit_rollout_step_latency_seconds",
            "Latency of each step in a canary rollout.",
            // Buckets in seconds: 10s, 30s, 1m, 2m, 5m, 10m
            vec![10.0, 30.0, 60.0, 120.0, 300.0, 600.0]
        ).unwrap();
}

/// Creates a new Prometheus registry and registers all custom metrics.
///
/// This function is intended to be called once at operator startup.
///
/// # Returns
/// A `Result` containing the `Registry` or a `prometheus::Error`.
pub fn create_and_register_metrics() -> Result<Registry, prometheus::Error> {
    let r = Registry::new();
    r.register(Box::new(PHGIT_PREVIEW_CREATED_TOTAL.clone()))?;
    r.register(Box::new(PHGIT_PREVIEW_ACTIVE.clone()))?;
    r.register(Box::new(PHGIT_ROLLOUTS_TOTAL.clone()))?;
    r.register(Box::new(PHGIT_ROLLOUT_STEP_LATENCY_SECONDS.clone()))?;
    Ok(r)
}
