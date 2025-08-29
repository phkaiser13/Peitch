/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/runner_manager/src/lib.rs
*
* This file is the main entry point and implementation for the runner_manager module.
* It handles FFI, configuration parsing, and the logic for `scale` and `hpa_install` actions.
*
* SPDX-License-Identifier: Apache-2.0 */

pub mod config;

use anyhow::Result;
use config::{RunnerPayload, ScaleConfig, HpaInstallConfig};
use k8s_openapi::api::autoscaling::v2::HorizontalPodAutoscaler;
use kube::{
    api::{Api, ObjectMeta, Patch, PatchParams, PostParams},
    Client,
};
use std::ffi::{c_char, CStr};
use std::panic;

async fn handle_scale(config: ScaleConfig) -> Result<()> {
    println!("🚀 Scaling runners in cluster '{}'...", config.cluster);
    // In a real scenario, this would likely patch a custom 'RunnerSet' resource
    // or a Deployment. For this example, we'll just print the intended action.
    println!("   -> Min Replicas: {}", config.min);
    println!("   -> Max Replicas: {}", config.max);
    println!("   -> Metric: {}", config.metric);
    println!("✅ [SIMULATION] Applied scaling parameters.");
    Ok(())
}

async fn handle_hpa_install(config: HpaInstallConfig) -> Result<()> {
    println!("🚀 Installing HPA for runners in namespace '{}'...", config.namespace);
    let client = Client::try_default().await?;
    let hpas: Api<HorizontalPodAutoscaler> = Api::namespaced(client, &config.namespace);

    let hpa_manifest: HorizontalPodAutoscaler = serde_json::from_value(serde_json::json!({
        "apiVersion": "autoscaling/v2",
        "kind": "HorizontalPodAutoscaler",
        "metadata": {
            "name": "phgit-runner-hpa"
        },
        "spec": {
            "scaleTargetRef": {
                "apiVersion": "apps/v1",
                "kind": "Deployment",
                "name": "phgit-runner"
            },
            "minReplicas": 1, // These would typically come from the 'scale' command config
            "maxReplicas": 10,
            "metrics": [{
                "type": "Pods",
                "pods": {
                    "metric": {
                        "name": config.metric
                    },
                    "target": {
                        "type": "AverageValue",
                        "averageValue": config.target.to_string()
                    }
                }
            }]
        }
    }))?;

    hpas.create(&PostParams::default(), &hpa_manifest).await?;
    println!("✅ HorizontalPodAutoscaler 'phgit-runner-hpa' created successfully.");
    Ok(())
}

#[no_mangle]
pub extern "C" fn run_runner_manager(config_json: *const c_char) -> i32 {
    let result = panic::catch_unwind(|| {
        if config_json.is_null() { return -1; }
        let c_str = unsafe { CStr::from_ptr(config_json) };
        let rust_str = c_str.to_str().unwrap();
        let config: RunnerPayload = serde_json::from_str(rust_str).unwrap();

        let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
        
        let exec_result = runtime.block_on(async {
            match config {
                RunnerPayload::Scale(cfg) => handle_scale(cfg).await,
                RunnerPayload::HpaInstall(cfg) => handle_hpa_install(cfg).await,
            }
        });

        match exec_result {
            Ok(_) => 0,
            Err(e) => {
                eprintln!("[runner_manager] Error: {:?}", e);
                -4
            }
        }
    });
    result.unwrap_or(-5)
}
