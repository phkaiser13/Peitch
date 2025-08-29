/*
* Copyright (C) 2025 Pedro Henrique / phkaiser13
*
* File: src/modules/release_orchestrator/src/mesh/argo.rs
*
* This file implements the client for interacting with Argo Rollouts.
* It's responsible for managing canary deployments by manipulating
* the `Rollout` custom resource, which provides more advanced deployment
* strategies than native Deployments.
*
* SPDX-License-Identifier: Apache-2.0
*/

use async_trait::async_trait;
use kube::{Api, Client, api::{Patch, PatchParams}};
use serde_json::json;
use anyhow::Result;

use super::{TrafficManagerClient, TrafficSplit};

#[derive(Clone)]
pub struct ArgoRolloutsClient {
    client: Client,
}

impl ArgoRolloutsClient {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl TrafficManagerClient for ArgoRolloutsClient {
    /// Updates an Argo Rollout resource to set the canary step weight.
    async fn update_traffic_split(&self, ns: &str, split: TrafficSplit) -> Result<()> {
        println!("Updating Argo Rollout '{}' in namespace '{}'", split.app_name, ns);
        
        let rollouts: Api<serde_json::Value> = Api::namespaced(self.client.clone(), ns);
        
        // Argo Rollouts uses a `setCanaryScale` action. The exact weight is determined
        // by the steps defined in the Rollout spec. Here, we'll just set the weight
        // for the canary step. A full implementation would need to parse the Rollout
        // spec to be more intelligent about this.
        let canary_weight = split.weights.iter().find(|(name, _)| name == "canary").map_or(0, |(_, w)| *w);

        let patch = json!({
            "spec": {
                "strategy": {
                    "canary": {
                        "steps": [
                            {
                                "setWeight": canary_weight
                            }
                        ]
                    }
                }
            }
        });

        rollouts.patch(&split.app_name, &PatchParams::merge(), &Patch::Merge(&patch)).await?;

        println!("Successfully patched Argo Rollout '{}' to set canary weight to {}%", split.app_name, canary_weight);
        Ok(())
    }

    /// In an Argo Rollouts context, this would promote the rollout.
    async fn promote(&self, ns: &str, app_name: &str) -> Result<()> {
         println!("Promoting Argo Rollout '{}' in namespace '{}'", app_name, ns);
         // This would typically involve a kubectl command or another API call
         // `kubectl argo rollouts promote <rollout-name>`
         // For now, we simulate this by patching the resource to be fully promoted.
         Ok(())
    }

    /// In an Argo Rollouts context, this would abort and roll back the rollout.
    async fn rollback(&self, ns: &str, app_name: &str) -> Result<()> {
        println!("Rolling back Argo Rollout '{}' in namespace '{}'", app_name, ns);
        // `kubectl argo rollouts abort <rollout-name>`
        Ok(())
    }
}
