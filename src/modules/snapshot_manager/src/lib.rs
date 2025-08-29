/*
 * Copyright (C) 2025 Pedro Henrique / phkaiser13
 *
 * File: src/modules/snapshot_manager/src/lib.rs
 *
 * This module provides the logic for taking diagnostic snapshots of applications.
 *
 * SPDX-License-Identifier: Apache-2.0
 */

use anyhow::{anyhow, Context, Result};
use futures::{StreamExt, TryStreamExt};
use k8s_openapi::api::core::v1::Pod;
use kube::{
    api::{Api, Attached, ListParams, LogParams},
    Client, ResourceExt,
};
use opentelemetry::{
    global,
    trace::{Span, Tracer},
};
use opentelemetry_jaeger::Propagator;
use std::fs::File;
use std::io::Write;

// --- Public Data Structures ---

pub struct SnapshotConfig<'a> {
    pub app_name: &'a str,
    pub namespace: &'a str,
    pub snapshot_name: &'a str,
    pub include_logs: bool,
    pub include_traces: bool,
    pub include_db_dump: bool,
}

// --- Public Function ---

pub async fn take_snapshot(client: Client, config: SnapshotConfig<'_>) -> Result<String> {
    let filename = format!("/tmp/{}-{}.snapshot", config.snapshot_name, chrono::Utc::now().timestamp());
    let mut file = File::create(&filename)
        .with_context(|| format!("Failed to create snapshot file: {}", filename))?;

    writeln!(file, "--- Snapshot for {}/{} ---", config.namespace, config.app_name)?;

    if config.include_logs {
        writeln!(file, "\n--- Pod Logs ---")?;
        let logs = get_pod_logs(&client, config.namespace, config.app_name).await?;
        writeln!(file, "{}", logs)?;
    }

    if config.include_traces {
        writeln!(file, "\n--- OpenTelemetry Traces ---")?;
        let traces = get_traces(config.app_name)?;
        writeln!(file, "{}", traces)?;
    }

    if config.include_db_dump {
        writeln!(file, "\n--- Database Dump ---")?;
        let dump = trigger_db_dump(&client, config.namespace, config.app_name).await?;
        writeln!(file, "{}", dump)?;
    }

    Ok(filename)
}

// --- Private Helpers ---

async fn get_pod_logs(client: &Client, namespace: &str, app_name: &str) -> Result<String> {
    log::info!("Fetching logs for app: {}", app_name);
    let pods: Api<Pod> = Api::namespaced(client.clone(), namespace);
    let lp = ListParams::default().labels(&format!("app.kubernetes.io/name={}", app_name));
    let pod_list = pods.list(&lp).await?;
    let mut all_logs = String::new();

    for pod in pod_list {
        let pod_name = pod.name_any();
        all_logs.push_str(&format!("\n--- Logs for Pod: {} ---\n", pod_name));
        let lp = LogParams {
            all_containers: true,
            tail_lines: Some(1000),
            ..Default::default()
        };
        match pods.logs(&pod_name, &lp).await {
            Ok(logs) => all_logs.push_str(&logs),
            Err(e) => {
                let err_msg = format!("Warning: Could not retrieve logs for pod {}: {}", pod_name, e);
                log::warn!("{}", err_msg);
                all_logs.push_str(&err_msg);
            }
        }
    }
    Ok(all_logs)
}

fn get_traces(service_name: &str) -> Result<String> {
    log::info!("Exporting traces for service: {}", service_name);
    
    let tracer = opentelemetry_jaeger::new_agent_pipeline()
        .with_service_name(service_name)
        .install_simple()?;
    
    let mut span = tracer.start("collect-snapshot-traces");
    span.add_event("This is a sample event for the trace snapshot.".to_string(), vec![]);
    
    let result = format!("Traces for service '{}' are being exported via the Jaeger agent. Captured one sample span with ID: {:?}", service_name, span.span_context().trace_id());
    span.end();
    
    global::shutdown_tracer_provider();
    
    Ok(result)
}

async fn trigger_db_dump(client: &Client, namespace: &str, app_name: &str) -> Result<String> {
    log::info!("Triggering DB dump for app: {}", app_name);
    let pods: Api<Pod> = Api::namespaced(client.clone(), namespace);
    let lp = ListParams::default().labels(&format!("app.kubernetes.io/name={},role=db-dumper", app_name));
    
    let pod_list = pods.list(&lp).await?;
    let dumper_pod = pod_list.items.into_iter().next()
        .ok_or_else(|| anyhow!("No pod found with label role=db-dumper for app {}", app_name))?;
    
    let pod_name = dumper_pod.name_any();
    log::info!("Found dumper pod: {}", pod_name);

    let mut attached = pods.exec(&pod_name, ["/dump.sh"], &Default::default()).await?;
    
    let stdout = tokio::io::read_to_string(attached.stdout().unwrap()).await?;
    let stderr = tokio::io::read_to_string(attached.stderr().unwrap()).await?;
    
    let status = attached.take_status().unwrap().await?;
    if !status.success() {
        return Err(anyhow!("DB dump command failed with exit code {:?}:\n{}", status.code, stderr));
    }
    
    Ok(stdout)
}
