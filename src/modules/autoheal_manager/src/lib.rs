/*
 * Copyright (C) 2025 Pedro Henrique / phkaiser13
 *
 * File: src/modules/autoheal_manager/src/lib.rs
 *
 * This module provides the FFI bridge for the `phgit autoheal enable` command.
 * It receives a JSON payload from the C CLI, deserializes it, and creates
 * a `phAutoHealRule` custom resource in the cluster.
 *
 * SPDX-License-Identifier: Apache-2.0
 */

use anyhow::{Context, Result};
use kube::{
    api::{Api, ObjectMeta, Patch, PatchParams},
    Client, CustomResource,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::ffi::{c_char, CStr};
use std::panic;

// --- CRD Structs (Duplicated from operator crate for simplicity) ---

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[kube(
    group = "ph.kaiser.io",
    version = "v1alpha1",
    kind = "phAutoHealRule",
    namespaced
)]
#[serde(rename_all = "camelCase")]
pub struct PhAutoHealRuleSpec {
    pub trigger_name: String,
    pub cooldown: String,
    pub actions: Vec<HealAction>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct HealAction {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redeploy: Option<RedeployAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale_up: Option<ScaleUpAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runbook: Option<RunbookAction>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
pub struct RedeployAction {
    pub target: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct ScaleUpAction {
    pub target: String,
    pub replicas: i32,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct RunbookAction {
    pub script_name: String,
}

// --- FFI Payload Struct ---

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct AutoHealRequest {
    trigger_name: String,
    cooldown: String,
    namespace: String,
    actions_str: String,
}

// --- FFI Entry Point ---

#[no_mangle]
pub extern "C" fn run_autoheal_manager(config_json: *const c_char) -> i32 {
    let result = panic::catch_unwind(|| {
        if config_json.is_null() {
            log::error!("[autoheal_manager] FFI Error: Received a null pointer.");
            return -1;
        }
        let c_str = unsafe { CStr::from_ptr(config_json) };
        let rust_str = match c_str.to_str() {
            Ok(s) => s,
            Err(e) => {
                log::error!("[autoheal_manager] FFI Error: Invalid UTF-8 in payload: {}", e);
                return -2;
            }
        };

        let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
            Ok(rt) => rt,
            Err(e) => {
                log::error!("[autoheal_manager] Runtime Error: {}", e);
                return -4;
            }
        };

        match rt.block_on(run_internal(rust_str)) {
            Ok(_) => 0,
            Err(e) => {
                log::error!("[autoheal_manager] Execution Error: {:?}", e);
                -4
            }
        }
    });

    result.unwrap_or(-5)
}

async fn run_internal(json_str: &str) -> Result<()> {
    let request: AutoHealRequest = serde_json::from_str(json_str)
        .context("Failed to deserialize JSON payload")?;

    let actions = parse_actions_str(&request.actions_str)?;

    let client = Client::try_default().await.context("Failed to create Kubernetes client")?;
    let api: Api<phAutoHealRule> = Api::namespaced(client, &request.namespace);

    let rule_name = format!("autoheal-rule-{}", request.trigger_name);

    let rule = phAutoHealRule {
        metadata: ObjectMeta {
            name: Some(rule_name.clone()),
            namespace: Some(request.namespace.clone()),
            ..Default::default()
        },
        spec: PhAutoHealRuleSpec {
            trigger_name: request.trigger_name,
            cooldown: request.cooldown,
            actions,
        },
        status: None,
    };

    let ssapply = PatchParams::apply("ph.autoheal-manager");
    api.patch(&rule_name, &ssapply, &Patch::Apply(&rule)).await
        .with_context(|| format!("Failed to apply phAutoHealRule '{}'", rule_name))?;

    log::info!("Successfully applied phAutoHealRule '{}'.", rule_name);
    Ok(())
}

fn parse_actions_str(actions_str: &str) -> Result<Vec<HealAction>> {
    let mut actions = Vec::new();
    for action_part in actions_str.split(',') {
        let parts: Vec<&str> = action_part.trim().split(':').collect();
        let action_name = parts.get(0).ok_or_else(|| anyhow!("Empty action part"))?;

        let heal_action = match *action_name {
            "redeploy" => {
                let target = parts.get(1).map(|s| s.trim()).ok_or_else(|| anyhow!("Missing target for redeploy action"))?;
                HealAction {
                    redeploy: Some(RedeployAction { target: target.to_string() }),
                    ..Default::default()
                }
            }
            "scale-up" => {
                let target = parts.get(1).map(|s| s.trim()).ok_or_else(|| anyhow!("Missing target for scale-up action"))?;
                let replicas_str = parts.get(2).map(|s| s.trim()).ok_or_else(|| anyhow!("Missing replica count for scale-up action"))?;
                let replicas = replicas_str.parse::<i32>().context("Invalid replica count for scale-up")?;
                HealAction {
                    scale_up: Some(ScaleUpAction { target: target.to_string(), replicas }),
                    ..Default::default()
                }
            }
            "runbook" => {
                let script_name = parts.get(1).map(|s| s.trim()).ok_or_else(|| anyhow!("Missing scriptName for runbook action"))?;
                HealAction {
                    runbook: Some(RunbookAction { script_name: script_name.to_string() }),
                    ..Default::default()
                }
            }
            _ => return Err(anyhow!("Unknown action type: {}", action_name)),
        };
        actions.push(heal_action);
    }
    Ok(actions)
}
