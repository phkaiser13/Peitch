/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/release_orchestrator/src/strategies/canary.rs
* This file provides the concrete implementation of the `Strategy` trait for
* a Canary release. The logic is responsible for calculating the correct
* traffic weight distribution between the stable and new (canary) versions
* and then commanding the underlying service mesh client to apply that routing
* rule. It is completely decoupled from any specific service mesh implementation.
* SPDX-License-Identifier: Apache-2.0 */

use super::{Strategy, ServiceMeshClient};
use crate::config::ReleaseConfig;
use crate::mesh::TrafficSplit;
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;

/// Represents the Canary release strategy.
/// This is a stateless struct; its logic is contained in the `Strategy` trait implementation.
pub struct CanaryStrategy;

#[async_trait]
impl Strategy for CanaryStrategy {
    /// Executes the canary release logic.
    async fn execute(
        &self,
        config: &ReleaseConfig,
        mesh_client: &(dyn ServiceMeshClient + Send + Sync),
    ) -> Result<()> {
        // 1. Validate the configuration specific to this strategy.
        let canary_weight = config.traffic_percent.ok_or_else(|| {
            anyhow!("'traffic_percent' is required for a canary release strategy")
        })?;

        if canary_weight > 100 {
            return Err(anyhow!(
                "'traffic_percent' cannot be greater than 100. Found: {}",
                canary_weight
            ));
        }

        // 2. Calculate the traffic distribution.
        let stable_weight = 100 - canary_weight;

        println!(
            "Executing canary release for '{}': shifting {}% of traffic to version '{}' and {}% to version '{}'.",
            config.app_name, canary_weight, config.new_version, stable_weight, config.stable_version
        );

        // 3. Construct the generic `TrafficSplit` object.
        let split = TrafficSplit {
            app_name: config.app_name.clone(),
            weights: vec![
                (config.stable_version.clone(), stable_weight),
                (config.new_version.clone(), canary_weight),
            ],
        };

        // 4. Delegate the actual implementation to the service mesh client.
        // The strategy logic does not need to know *how* the traffic is split,
        // only that it *should* be split according to the weights.
        mesh_client
            .update_traffic_split(&config.namespace, split)
            .await
            .context("Failed to apply traffic split via the service mesh client")?;

        println!("Canary release strategy executed successfully.");
        Ok(())
    }
}