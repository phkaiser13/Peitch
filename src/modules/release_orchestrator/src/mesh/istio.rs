/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/release_orchestrator/src/mesh/istio.rs
* This file provides the concrete implementation of the `ServiceMeshClient`
* trait for the Istio service mesh. It defines the Rust structs corresponding
* to Istio's `VirtualService` and `DestinationRule` Custom Resources (CRs) and
* contains the logic to manipulate these resources to control traffic routing
* for canary and blue-green deployments.
* SPDX-License-Identifier: Apache-2.0 */

use super::{ServiceMeshClient, TrafficSplit};
use anyhow::{Context, Result};
use async_trait::async_trait;
use kube::{
    api::{Api, Patch, PatchParams, ResourceExt},
    Client, CustomResource,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// --- Custom Resource Definitions for Istio ---

/// Defines the Rust struct for Istio's `VirtualService` CRD.
/// We only define the fields relevant to traffic splitting.
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "networking.istio.io",
    version = "v1beta1",
    kind = "VirtualService",
    plural = "virtualservices"
)]
#[kube(namespaced)]
pub struct VirtualServiceSpec {
    pub hosts: Vec<String>,
    pub http: Vec<HTTPRoute>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct HTTPRoute {
    pub route: Vec<HTTPRouteDestination>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct HTTPRouteDestination {
    pub destination: Destination,
    pub weight: Option<u8>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct Destination {
    pub host: String,
    pub subset: String,
}

/// Defines the Rust struct for Istio's `DestinationRule` CRD.
/// This resource defines the subsets (versions) of a service.
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "networking.istio.io",
    version = "v1beta1",
    kind = "DestinationRule",
    plural = "destinationrules"
)]
#[kube(namespaced)]
pub struct DestinationRuleSpec {
    pub host: String,
    pub subsets: Vec<Subset>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct Subset {
    pub name: String,
    pub labels: BTreeMap<String, String>,
}

// --- Istio Client Implementation ---

/// A client for interacting with Istio's specific CRDs.
pub struct IstioClient {
    client: Client,
}

impl IstioClient {
    /// Creates a new `IstioClient`.
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// Ensures a `DestinationRule` exists with the required subsets.
    /// This is a prerequisite for traffic shifting in a `VirtualService`.
    async fn ensure_destination_rule(&self, namespace: &str, split: &TrafficSplit) -> Result<()> {
        let dr_api: Api<DestinationRule> = Api::namespaced(self.client.clone(), namespace);
        let subsets: Vec<Subset> = split
            .weights
            .iter()
            .map(|(version, _)| {
                let mut labels = BTreeMap::new();
                labels.insert("version".to_string(), version.clone());
                Subset {
                    name: version.clone(),
                    labels,
                }
            })
            .collect();

        let dr = DestinationRule::new(
            &split.app_name,
            DestinationRuleSpec {
                host: split.app_name.clone(),
                subsets,
            },
        );

        // Use Server-Side Apply to create or update the DestinationRule.
        let ssapply = PatchParams::apply("peitch.release_orchestrator");
        dr_api
            .patch(&split.app_name, &ssapply, &Patch::Apply(&dr))
            .await
            .with_context(|| format!("Failed to apply DestinationRule for '{}'", split.app_name))?;

        Ok(())
    }

    /// Applies a `VirtualService` to distribute traffic according to the weights.
    async fn apply_virtual_service(&self, namespace: &str, split: TrafficSplit) -> Result<()> {
        let vs_api: Api<VirtualService> = Api::namespaced(self.client.clone(), namespace);
        let routes: Vec<HTTPRouteDestination> = split
            .weights
            .into_iter()
            .map(|(version, weight)| HTTPRouteDestination {
                destination: Destination {
                    host: split.app_name.clone(),
                    subset: version,
                },
                weight: Some(weight),
            })
            .collect();

        let vs = VirtualService::new(
            &split.app_name,
            VirtualServiceSpec {
                hosts: vec![split.app_name.clone()],
                http: vec![HTTPRoute { route: routes }],
            },
        );

        // Use Server-Side Apply to create or update the VirtualService.
        let ssapply = PatchParams::apply("peitch.release_orchestrator");
        vs_api
            .patch(&split.app_name, &ssapply, &Patch::Apply(&vs))
            .await
            .with_context(|| format!("Failed to apply VirtualService for '{}'", split.app_name))?;

        Ok(())
    }
}

#[async_trait]
impl ServiceMeshClient for IstioClient {
    /// Updates Istio routing rules to match the desired traffic split.
    async fn update_traffic_split(&self, namespace: &str, split: TrafficSplit) -> Result<()> {
        println!(
            "Updating Istio traffic split for '{}' in namespace '{}'...",
            split.app_name, namespace
        );

        // 1. Ensure the DestinationRule with the correct subsets exists.
        self.ensure_destination_rule(namespace, &split).await?;
        println!("Successfully applied DestinationRule for '{}'.", split.app_name);

        // 2. Apply the VirtualService with the new traffic weights.
        self.apply_virtual_service(namespace, split).await?;
        println!("Successfully applied VirtualService to shift traffic.");

        Ok(())
    }
}