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

use crate::cluster_manager::{ActionDetails, ClusterTarget, ExecutionStage};

/// A trait representing a deployment strategy.
///
/// A strategy's primary role is to determine the order and grouping of cluster
/// operations. It transforms a high-level intent into a detailed, multi-stage
/// execution plan.
pub trait DeploymentStrategy {
    /// Generates an execution plan based on the strategy's rules.
    ///
    /// # Arguments
    /// * `targets` - A slice of `Cluster` representing all clusters selected for the operation.
    /// * `manifests` - The raw string of Kubernetes manifests to be applied.
    /// * `app_name` - The logical name of the application.
    ///
    /// # Returns
    /// A `Result` containing a `Vec<ExecutionStage>`, which is the step-by-step plan.
    fn plan_execution(
        &self,
        targets: &[Cluster],
        manifests: &str,
        app_name: &str,
        namespace: &str,
    ) -> Result<Vec<ExecutionStage>>;
}

// --- Strategy Implementations ---

/// # Direct Strategy
///
/// Executes the action on all target clusters simultaneously. This is the
/// simplest strategy, treating all targets as a single stage.
pub struct DirectStrategy;

impl DeploymentStrategy for DirectStrategy {
    fn plan_execution(
        &self,
        targets: &[Cluster],
        manifests: &str,
        _app_name: &str,
        _namespace: &str,
    ) -> Result<Vec<ExecutionStage>> {
        if targets.is_empty() {
            return Ok(vec![]);
        }
        let plan = vec![ExecutionStage {
            targets: targets
                .iter()
                .map(|c| ClusterTarget { name: c.name.clone() })
                .collect(),
            action: ActionDetails::Apply {
                manifests: manifests.to_string(),
                variables: BTreeMap::new(),
            },
        }];
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
    fn plan_execution(
        &self,
        targets: &[Cluster],
        manifests: &str,
        app_name: &str,
        namespace: &str,
    ) -> Result<Vec<ExecutionStage>> {
        // Functionally identical to Direct for this implementation.
        DirectStrategy.plan_execution(targets, manifests, app_name, namespace)
    }
}

/// # Staged Strategy
///
/// Groups clusters by their defined `stage` number and executes them in
/// ascending order of stages. Clusters within the same stage are run concurrently.
pub struct StagedStrategy;

impl DeploymentStrategy for StagedStrategy {
    fn plan_execution(
        &self,
        targets: &[Cluster],
        manifests: &str,
        _app_name: &str,
        _namespace: &str,
    ) -> Result<Vec<ExecutionStage>> {
        let mut stages: BTreeMap<u32, Vec<ClusterTarget>> = BTreeMap::new();

        for target in targets {
            let stage_num = target.stage.unwrap_or(u32::MAX);
            stages
                .entry(stage_num)
                .or_default()
                .push(ClusterTarget { name: target.name.clone() });
        }

        let plan: Vec<ExecutionStage> = stages
            .into_values()
            .map(|cluster_targets| ExecutionStage {
                targets: cluster_targets,
                action: ActionDetails::Apply {
                    manifests: manifests.to_string(),
                    variables: BTreeMap::new(),
                },
            })
            .collect();

        Ok(plan)
    }
}

/// # Failover Strategy
///
/// Plans the execution on one cluster at a time, in a deterministic order.
/// The executor will stop after the first successful deployment.
pub struct FailoverStrategy;

impl DeploymentStrategy for FailoverStrategy {
    fn plan_execution(
        &self,
        targets: &[Cluster],
        manifests: &str,
        _app_name: &str,
        _namespace: &str,
    ) -> Result<Vec<ExecutionStage>> {
        let mut sorted_targets = targets.to_vec();
        sorted_targets.sort_by(|a, b| a.name.cmp(&b.name));

        let plan: Vec<ExecutionStage> = sorted_targets
            .into_iter()
            .map(|target| ExecutionStage {
                targets: vec![ClusterTarget { name: target.name }],
                action: ActionDetails::Apply {
                    manifests: manifests.to_string(),
                    variables: BTreeMap::new(),
                },
            })
            .collect();

        Ok(plan)
    }
}

/// # BlueGreen Strategy
///
/// Implements a multi-cluster blue-green deployment.
pub struct BlueGreenStrategy;

impl DeploymentStrategy for BlueGreenStrategy {
    fn plan_execution(
        &self,
        targets: &[Cluster],
        manifests: &str,
        app_name: &str,
        namespace: &str,
    ) -> Result<Vec<ExecutionStage>> {
        if targets.is_empty() {
            return Ok(vec![]);
        }

        let all_targets = targets
            .iter()
            .map(|c| ClusterTarget { name: c.name.clone() })
            .collect();

        // Stage 1: Deploy Green
        let mut green_vars = BTreeMap::new();
        green_vars.insert("color".to_string(), "green".to_string());
        let deploy_green = ExecutionStage {
            targets: all_targets.clone(),
            action: ActionDetails::Apply {
                manifests: manifests.to_string(),
                variables: green_vars,
            },
        };

        // Stage 2: Health Check Green
        let health_check_green = ExecutionStage {
            targets: all_targets.clone(),
            action: ActionDetails::HealthCheck {
                app_name: app_name.to_string(),
                namespace: namespace.to_string(),
                color: "green".to_string(),
            },
        };

        // Stage 3: Switch Traffic
        let switch_traffic = ExecutionStage {
            targets: all_targets.clone(),
            action: ActionDetails::SwitchTraffic {
                service_name: app_name.to_string(),
                new_target_color: "green".to_string(),
            },
        };

        // Stage 4: Decommission Blue
        let decommission_blue = ExecutionStage {
            targets: all_targets,
            action: ActionDetails::DeleteResources {
                app_name: app_name.to_string(),
                color_label: "blue".to_string(),
            },
        };

        Ok(vec![
            deploy_green,
            health_check_green,
            switch_traffic,
            decommission_blue,
        ])
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
        StrategyType::BlueGreen => Box::new(BlueGreenStrategy),
    }
}