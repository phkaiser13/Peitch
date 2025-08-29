/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/release_orchestrator/src/strategies/blue_green.rs
* This file provides the concrete implementation of the `Strategy` trait for
* a Blue-Green deployment. This strategy performs a complete traffic switch
* from the stable ("blue") version to the new ("green") version. It achieves
* this by commanding the service mesh to route 100% of traffic to the new
* version and 0% to the old one, while keeping the old version running to
* facilitate a rapid rollback if necessary.
* SPDX-License-Identifier: Apache-2.0 */

use super::{ServiceMeshClient, Strategy};
use crate::config::ReleaseConfig;
use crate::mesh::TrafficSplit;
use anyhow::{Context, Result};
use async_trait::async_trait;

/// Represents the Blue-Green release strategy.
pub struct BlueGreenStrategy;

#[async_trait]
impl Strategy for BlueGreenStrategy {
    /// Executes the blue-green release logic.
    async fn execute(
        &self,
        config: &ReleaseConfig,
        mesh_client: &(dyn ServiceMeshClient + Send + Sync),
    ) -> Result<()> {
        println!(
            "Executing Blue-Green deployment for '{}': shifting 100% of traffic to new version '{}'.",
            config.app_name, config.new_version
        );

        // For a blue-green deployment, the traffic split is binary:
        // 100% to the new ("green") version.
        // 0% to the stable ("blue") version.
        let split = TrafficSplit {
            app_name: config.app_name.clone(),
            weights: vec![
                (config.new_version.clone(), 100),
                (config.stable_version.clone(), 0),
            ],
        };

        // Delegate the traffic switch to the underlying service mesh client.
        // The strategy itself is agnostic to whether it's Istio, Linkerd, etc.
        mesh_client
            .update_traffic_split(&config.namespace, split)
            .await
            .context("Failed to apply 100/0 traffic split for blue-green deployment")?;

        println!("Blue-Green deployment strategy executed successfully.");
        Ok(())
    }
}