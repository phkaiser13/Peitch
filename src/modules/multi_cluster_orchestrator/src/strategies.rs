/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/multi_cluster_orchestrator/src/strategies.rs
*
* This file defines the deployment strategy logic for the multi-cluster
* orchestrator. It introduces a `DeploymentStrategy` trait, which acts as an
* interface for different deployment patterns. Each specific strategy (e.g.,
* Direct, Staged, Failover) is implemented as a struct that conforms to this
* trait.
*
* The core function of this module is `plan_execution`, which takes a list of
* target clusters and returns a structured plan as a series of stages. Each
* stage is a group of clusters to be acted upon concurrently. This abstraction
* allows the `ClusterManager` to remain agnostic of the specific strategy details,
* focusing solely on executing the planned stages. A factory function,
* `get_strategy`, is provided to instantiate the appropriate strategy object
* based on user configuration.
*
* SPDX-License-Identifier: Apache-2.0 */

use anyhow::Result;
// These types are expected to be defined and made public in the cluster_manager module.
use crate::cluster_manager::{Cluster, StrategyType};
use anyhow::Result;
use std::collections::BTreeMap;

/// A trait representing a deployment strategy.
///
/// A strategy's primary role is to determine the order and grouping of cluster
/// operations. It transforms a flat list of targets into a structured execution
/// plan, which is a vector of stages. Each stage is a vector of targets that
/// should be processed concurrently.
pub trait DeploymentStrategy {
    /// Generates an execution plan based on the strategy's rules.
    ///
    /// # Arguments
    /// * `targets` - A slice of `ClusterTarget` representing all clusters selected
    ///   for the operation.
    ///
    /// # Returns
    /// A `Result` containing a `Vec<Vec<ClusterTarget>>`.
    /// - The outer `Vec` represents the sequence of stages.
    /// - The inner `Vec` represents the set of clusters to be deployed to
    ///   concurrently within a single stage.
    fn plan_execution(&self, targets: &[ClusterTarget]) -> Result<Vec<Vec<ClusterTarget>>>;
}

// --- Strategy Implementations ---

/// # Direct Strategy
///
/// Executes the action on all target clusters simultaneously. This is the
/// simplest strategy, treating all targets as a single stage.
pub struct DirectStrategy;

impl DeploymentStrategy for DirectStrategy {
    fn plan_execution(&self, targets: &[ClusterTarget]) -> Result<Vec<Vec<ClusterTarget>>> {
        if targets.is_empty() {
            return Ok(vec![]);
        }
        // All targets are placed in a single stage for concurrent execution.
        let plan = vec![targets.to_vec()];
        Ok(plan)
    }
}

/// # Parallel Strategy
///
/// Executes the action on all target clusters simultaneously. This is
/// functionally identical to the Direct strategy but provides a more explicit
/// name for parallel operations.
pub struct ParallelStrategy;

impl DeploymentStrategy for ParallelStrategy {
    fn plan_execution(&self, targets: &[ClusterTarget]) -> Result<Vec<Vec<ClusterTarget>>> {
        if targets.is_empty() {
            return Ok(vec![]);
        }
        // All targets are placed in a single stage for concurrent execution.
        let plan = vec![targets.to_vec()];
        Ok(plan)
    }
}


use std::collections::BTreeMap;

/// # Staged Strategy
///
/// Groups clusters by their defined `stage` number and executes them in
/// ascending order of stages. Clusters within the same stage are run concurrently.
pub struct StagedStrategy;

impl DeploymentStrategy for StagedStrategy {
    fn plan_execution(&self, targets: &[ClusterTarget]) -> Result<Vec<Vec<ClusterTarget>>> {
        let mut stages: BTreeMap<u32, Vec<ClusterTarget>> = BTreeMap::new();

        for target in targets {
            // Use the target's stage number, or a default high number if not specified.
            let stage_num = target.stage.unwrap_or(u32::MAX);
            stages.entry(stage_num).or_default().push(target.clone());
        }

        // The BTreeMap automatically keeps the stages sorted by key (stage number).
        // We just need to collect the values (the groups of clusters).
        let plan: Vec<Vec<ClusterTarget>> = stages.into_values().collect();

        Ok(plan)
    }
}

/// # Failover Strategy
///
/// Plans the execution on one cluster at a time, in a deterministic order,
/// similar to the Staged strategy. The key difference is in how the executor
/// (`ClusterManager`) handles this plan: it will stop execution after the
/// first successful deployment. This planner's job is simply to provide the
/// ordered sequence of single-cluster stages.
pub struct FailoverStrategy;

impl DeploymentStrategy for FailoverStrategy {
    fn plan_execution(&self, targets: &[ClusterTarget]) -> Result<Vec<Vec<ClusterTarget>>> {
        // A predictable order is crucial for a failover strategy. We sort by name
        // to define the primary, secondary, etc., clusters.
        let mut sorted_targets = targets.to_vec();
        sorted_targets.sort_by(|a, b| a.name.cmp(&b.name));

        // The plan is identical to 'Staged': one cluster per stage. The executor
        // will provide the failover logic by halting the plan on success.
        let plan: Vec<Vec<ClusterTarget>> = sorted_targets
            .into_iter()
            .map(|target| vec![target])
            .collect();

        Ok(plan)
    }
}

// --- Factory Function ---

/// Instantiates and returns a boxed strategy implementation based on the `StrategyType`.
///
/// This factory function allows the `ClusterManager` to dynamically select the
/// correct strategy logic at runtime without needing to know the concrete types.
///
/// # Arguments
/// * `strategy_type` - The enum variant specifying which strategy to use.
///
/// # Returns
/// A `Box<dyn DeploymentStrategy>` containing an instance of the chosen strategy.
pub fn get_strategy(strategy_type: &StrategyType) -> Box<dyn DeploymentStrategy> {
    match strategy_type {
        StrategyType::Direct => Box::new(DirectStrategy),
        StrategyType::Staged => Box::new(StagedStrategy),
        StrategyType::Failover => Box::new(FailoverStrategy),
        StrategyType::Parallel => Box::new(ParallelStrategy),
    }
}