/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/release_orchestrator/src/mesh/mod.rs
* This file defines the abstraction layer for interacting with various service
* meshes. It establishes a common `ServiceMeshClient` trait (interface) that
* decouples the high-level release strategies from the low-level, mesh-specific
* implementation details. This design makes the system highly extensible,
* allowing for new service meshes to be supported by simply providing a new
* implementation of the trait.
* SPDX-License-Identifier: Apache-2.0 */

// Declare the specific mesh implementation modules.
pub mod istio;
pub mod linkerd;

use crate::config::ServiceMesh;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use kube::Client;

/// Represents a desired traffic distribution for a specific application.
/// The weights should sum to 100.
pub struct TrafficSplit {
    /// The name of the application service.
    pub app_name: String,
    /// A vector of (version_subset, weight) tuples.
    pub weights: Vec<(String, u8)>,
}

/// A trait defining the common interface for interacting with a service mesh.
/// Any supported service mesh must implement this trait.
#[async_trait]
pub trait ServiceMeshClient {
    /// Updates the traffic routing rules in the service mesh to match the
    /// desired traffic split. This is the core operation for canary and
    /// blue-green deployments.
    ///
    /// # Arguments
    /// * `namespace` - The Kubernetes namespace of the resources.
    /// * `split` - The desired `TrafficSplit` configuration.
    async fn update_traffic_split(&self, namespace: &str, split: TrafficSplit) -> Result<()>;
}

/// Factory function to get a concrete implementation of the `ServiceMeshClient`.
///
/// Based on the configuration, this function returns a boxed trait object that
/// can be used by the strategy logic without needing to know the concrete type.
///
/// # Arguments
/// * `mesh_type` - The `ServiceMesh` enum variant from the configuration.
/// * `client` - The shared Kubernetes `Client`.
pub fn get_mesh_client(
    mesh_type: ServiceMesh,
    client: Client,
) -> Result<Box<dyn ServiceMeshClient + Send + Sync>> {
    match mesh_type {
        ServiceMesh::Istio => Ok(Box::new(istio::IstioClient::new(client))),
        ServiceMesh::Linkerd => Ok(Box::new(linkerd::LinkerdClient::new(client))), // <-- UPDATED
        ServiceMesh::Traefik => {
            // When Traefik support is added, its client will be instantiated here.
            Err(anyhow!(
                "Traefik is not yet supported by the release orchestrator."
            ))
        }
    }
}