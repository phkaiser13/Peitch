/*
* Copyright (C) 2025 Pedro Henrique / phkaiser13
*
* File: src/modules/release_orchestrator/src/mesh/mod.rs
*
* This module provides a unified interface for interacting with different
* traffic management systems, including service meshes (like Istio, Linkerd)
* and dedicated controllers like Argo Rollouts. It abstracts away the specific
* details of how each system handles traffic splitting, promotion, and rollbacks,
* allowing the release_controller to work with a generic `TrafficManagerClient`.
*
* The goal is to make the traffic management logic pluggable. New systems
* can be supported by adding a new client that implements the `TrafficManagerClient` trait.
*
* SPDX-License-Identifier: Apache-2.0
*/

use anyhow::Result;
use async_trait::async_trait;

pub mod argo;
pub mod istio;
pub mod linkerd;

/// Represents a desired traffic split configuration.
pub struct TrafficSplit {
    /// The name of the application or service being targeted.
    pub app_name: String,
    /// A list of service versions and their corresponding traffic weights.
    /// e.g., `[("stable", 90), ("canary", 10)]`
    pub weights: Vec<(String, u8)>,
}

/// A generic trait for clients that can manage progressive delivery.
#[async_trait]
pub trait TrafficManagerClient {
    /// Updates the traffic management configuration to apply the given traffic split.
    async fn update_traffic_split(&self, ns: &str, split: TrafficSplit) -> Result<()>;

    /// Promotes a release, typically shifting 100% of traffic to the new version.
    async fn promote(&self, ns: &str, app_name: &str) -> Result<()>;

    /// Rolls back a release, shifting 100% of traffic to the stable version.
    async fn rollback(&self, ns: &str, app_name: &str) -> Result<()>;
}