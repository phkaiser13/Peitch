/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/release_orchestrator/src/strategies/mod.rs
* This file defines the abstraction layer for different release strategies. It
* establishes a common `Strategy` trait that decouples the main orchestrator
* logic from the specific implementation details of any given strategy (e.g.,
* Canary, Blue-Green). This allows for new strategies to be added in a modular
* fashion without altering the core workflow.
* SPDX-License-Identifier: Apache-2.0 */

// Declare the specific strategy implementation modules.
pub mod blue_green;
pub mod canary;

use crate::config::{ReleaseConfig, ReleaseStrategy};
use crate::mesh::ServiceMeshClient;
use anyhow::{anyhow, Result};
use async_trait::async_trait;

/// A trait defining the common interface for a release strategy.
/// Each supported strategy (Canary, Blue-Green, etc.) must implement this trait.
#[async_trait]
pub trait Strategy {
    /// Executes the release strategy.
    ///
    /// This method contains the high-level logic for a given strategy,
    /// orchestrating calls to the `ServiceMeshClient` to manipulate traffic
    /// routing as needed.
    ///
    /// # Arguments
    /// * `config` - The release configuration.
    /// * `mesh_client` - A trait object providing access to the service mesh.
    async fn execute(
        &self,
        config: &ReleaseConfig,
        // The mesh_client is passed as a dynamic trait object, allowing this
        // method to work with any service mesh that implements the trait.
        mesh_client: &(dyn ServiceMeshClient + Send + Sync),
    ) -> Result<()>;
}

/// Factory function to get a concrete implementation of a `Strategy`.
///
/// Based on the configuration, this function returns a boxed trait object
/// representing the chosen strategy.
///
/// # Arguments
/// * `strategy_type` - The `ReleaseStrategy` enum variant from the configuration.
pub fn get_strategy(
    strategy_type: ReleaseStrategy,
) -> Result<Box<dyn Strategy + Send + Sync>> {
    match strategy_type {
        ReleaseStrategy::Canary | ReleaseStrategy::TrafficShift => {
            Ok(Box::new(canary::CanaryStrategy))
        }
        ReleaseStrategy::BlueGreen => Ok(Box::new(blue_green::BlueGreenStrategy)), // <-- UPDATED
    }
}