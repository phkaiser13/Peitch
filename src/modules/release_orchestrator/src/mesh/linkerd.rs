/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/release_orchestrator/src/mesh/linkerd.rs
* This file provides the concrete implementation of the `ServiceMeshClient`
* trait for the Linkerd service mesh. It leverages the Service Mesh Interface
* (SMI) specification, specifically manipulating the `TrafficSplit` Custom
* Resource to control traffic routing. This adheres to Linkerd's design of
* working with community-standard APIs.
* SPDX-License-Identifier: Apache-2.0 */

use super::{TrafficManagerClient, TrafficSplit as InternalTrafficSplit};
use anyhow::{Context, Result};
use async_trait::async_trait;
use kube::{
    api::{Api, Patch, PatchParams, ResourceExt},
    Client, CustomResource,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// --- Custom Resource Definition for SMI TrafficSplit ---

/// Defines the Rust struct for the SMI `TrafficSplit` CRD.
/// Linkerd implements this standard for traffic shifting.
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "split.smi-spec.io",
    version = "v1alpha2",
    kind = "TrafficSplit",
    plural = "trafficsplits"
)]
#[kube(namespaced)]
pub struct TrafficSplitSpec {
    /// The root service that clients route to.
    pub service: String,
    /// The list of backend services to route traffic to.
    pub backends: Vec<TrafficSplitBackend>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct TrafficSplitBackend {
    /// The name of the backend service (the versioned service).
    pub service: String,
    /// The percentage of traffic to send to this backend.
    pub weight: u16,
}

// --- Linkerd Client Implementation ---

/// A client for interacting with Linkerd via SMI CRDs.
pub struct LinkerdClient {
    client: Client,
}

impl LinkerdClient {
    /// Creates a new `LinkerdClient`.
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl TrafficManagerClient for LinkerdClient {
    /// Updates SMI TrafficSplit rules to match the desired traffic distribution.
    async fn update_traffic_split(
        &self,
        namespace: &str,
        split: InternalTrafficSplit,
    ) -> Result<()> {
        println!(
            "Updating Linkerd (SMI) traffic split for '{}' in namespace '{}'...",
            split.app_name, namespace
        );

        let ts_api: Api<TrafficSplit> = Api::namespaced(self.client.clone(), namespace);

        // The name of the TrafficSplit resource is typically the same as the root service.
        let resource_name = &split.app_name;

        // Convert our internal `TrafficSplit` representation to the SMI spec.
        // Note: SMI weights are `u16`, while ours are `u8`.
        let backends: Vec<TrafficSplitBackend> = split
            .weights
            .into_iter()
            .map(|(version_name, weight)| {
                // In Linkerd, the backend service name is often the app name suffixed with the version.
                let backend_service_name = format!("{}-{}", split.app_name, version_name);
                TrafficSplitBackend {
                    service: backend_service_name,
                    weight: weight as u16,
                }
            })
            .collect();

        let ts = TrafficSplit::new(
            resource_name,
            TrafficSplitSpec {
                service: split.app_name.clone(),
                backends,
            },
        );

        // Use Server-Side Apply to create or update the TrafficSplit resource.
        let ssapply = PatchParams::apply("peitch.release_orchestrator");
        ts_api
            .patch(resource_name, &ssapply, &Patch::Apply(&ts))
            .await
            .with_context(|| format!("Failed to apply SMI TrafficSplit for '{}'", resource_name))?;

        println!("Successfully applied SMI TrafficSplit for '{}'.", resource_name);
        Ok(())
    }

    /// Promotes a release by shifting 100% of traffic to the "canary" backend.
    async fn promote(&self, ns: &str, app_name: &str) -> Result<()> {
        println!("Promoting release for '{}' in namespace '{}' via Linkerd/SMI", app_name, ns);
        let split = InternalTrafficSplit {
            app_name: app_name.to_string(),
            weights: vec![
                ("stable".to_string(), 0),
                ("canary".to_string(), 100),
            ],
        };
        self.update_traffic_split(ns, split).await
    }

    /// Rolls back a release by shifting 100% of traffic to the "stable" backend.
    async fn rollback(&self, ns: &str, app_name: &str) -> Result<()> {
        println!("Rolling back release for '{}' in namespace '{}' via Linkerd/SMI", app_name, ns);
        let split = InternalTrafficSplit {
            app_name: app_name.to_string(),
            weights: vec![
                ("stable".to_string(), 100),
                ("canary".to_string(), 0),
            ],
        };
        self.update_traffic_split(ns, split).await
    }
}