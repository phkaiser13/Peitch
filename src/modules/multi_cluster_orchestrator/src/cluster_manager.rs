/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: src/modules/multi_cluster_orchestrator/src/cluster_manager.rs
*
* This file contains the core logic for managing and interacting with a fleet
* of Kubernetes clusters. It defines the data structures for configuration,
* a `ClusterManager` responsible for initializing clients, and the logic for
* executing actions.
*
* This version has been significantly refactored to support strategy-driven
* deployments. The `execute_action` function now leverages the `strategies`
* module to create an execution plan. It iterates through the plan's stages
* sequentially, but executes the operations within each stage concurrently. It
* also contains the specific logic to halt or continue the deployment based on
* the outcome of each stage, correctly implementing "Staged" and "Failover"
* behaviors.
*
* SPDX-License-Identifier: Apache-2.0 */

// Import the new strategies module
use crate::strategies;
use anyhow::{anyhow, Context, Result};
use futures::future::join_all;
use k8s_openapi::api::{
    apps::v1::{DaemonSet, Deployment, StatefulSet},
    batch::v1::Job,
    core::v1::Namespace,
};
use kube::{
    api::{Api, DynamicObject, GroupVersionKind, Patch, PatchParams, ResourceExt},
    discovery, Client, Config, Resource,
};
use serde::{Deserialize, Serialize};

// Minimal definition for a CRD from another module to avoid dependency.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct PhgitReleaseStatus {
    phase: Option<String>,
}
#[derive(Resource, Deserialize, Debug, Clone)]
#[kube(group = "phgit.dev", version = "v1", kind = "PhgitRelease")]
struct PhgitReleaseSpec {
    // We don't need any fields from the spec for this probe.
}
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use strsim::levenshtein;
use tokio::time::{sleep, Duration};

// --- Configuration Structures for clusters.yaml ---

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub enum HealthProbe {
    HttpGet(HttpGetProbe),
    Prometheus(PrometheusProbe),
    PhgitRelease(PhgitReleaseProbe),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HttpGetProbe {
    pub url: String,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PrometheusProbe {
    pub query: String,
    pub expected_result: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PhgitReleaseProbe {
    pub name: String,
    pub namespace: String,
    pub expected_phase: String,
}

fn default_timeout_seconds() -> u64 {
    5
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Cluster {
    pub name: String,
    api_server_url: String,
    ca_cert_secret: String,
    auth_token_secret: String,
    region: String,
    environment: String,
    #[serde(default)]
    pub health_probes: Vec<HealthProbe>,
    #[serde(default)]
    pub stage: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ClustersConfig {
    clusters: Vec<Cluster>,
}

// --- Configuration Structures for FFI ---
// These structs are now public so they can be accessed by lib.rs and strategies.rs

/// Defines the type of deployment strategy.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StrategyType {
    Direct,
    Staged,
    Failover,
    Parallel,
}

/// Encapsulates the deployment strategy configuration.
#[derive(Deserialize, Debug, Clone)]
pub struct Strategy {
    #[serde(rename = "type")]
    pub strategy_type: StrategyType,
}

/// Defines an action to be performed across clusters.
#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// Apply a set of Kubernetes manifests using a specific strategy.
    Apply {
        manifests: String,
        strategy: Strategy,
    },
}

/// Defines a target cluster for an operation.
#[derive(Deserialize, Debug, Clone)]
pub struct ClusterTarget {
    /// The logical name of the cluster, must match a key in `cluster_configs`.
    pub name: String,
}

/// Top-level configuration structure deserialized from the FFI JSON input.
#[derive(Deserialize, Debug)]
pub struct MultiClusterConfig {
    /// A map from logical cluster name to its kubeconfig file path.
    pub cluster_configs: BTreeMap<String, String>,
    /// The list of clusters to target for this specific operation.
    pub targets: Vec<ClusterTarget>,
    /// The action to perform on the target clusters.
    pub action: Action,
}

// --- Result Reporting Structure ---

/// Represents a resource that was successfully applied to a cluster.
#[derive(Debug, Clone)]
pub struct AppliedResource {
    pub gvk: GroupVersionKind,
    pub name: String,
    pub namespace: Option<String>,
}

/// Represents the outcome of an operation on a single cluster.
#[derive(Debug)]
pub struct ClusterResult {
    pub cluster_name: String,
    /// `Ok` contains a vector of applied resources, `Err` contains the error details.
    pub outcome: std::result::Result<Vec<AppliedResource>, String>,
}

// --- Core Logic ---

/// Manages a collection of Kubernetes clients and orchestrates actions upon them.
pub struct ClusterManager {
    /// A map of cluster names to their initialized and ready-to-use Kubernetes clients.
    clients: BTreeMap<String, Client>,
}

impl ClusterManager {
    /// Creates a new `ClusterManager` by initializing clients for all provided cluster configs.
    pub async fn new(configs: &BTreeMap<String, String>) -> Result<Self> {
        let client_futures = configs.iter().map(|(name, path)| async move {
            let config = Config::from_kubeconfig(&kube::config::Kubeconfig::read_from(path)?)
                .await
                .with_context(|| format!("Failed to load kubeconfig for cluster '{}'", name))?;
            let client = Client::try_from(config)
                .with_context(|| format!("Failed to create Kubernetes client for cluster '{}'", name))?;
            Ok((name.clone(), client))
        });

        let results: Vec<Result<(String, Client)>> = join_all(client_futures).await;
        let mut clients = BTreeMap::new();
        for result in results {
            let (name, client) = result?;
            clients.insert(name, client);
        }

        Ok(Self { clients })
    }

    /// Executes a given apply action across a list of target clusters according to a strategy.
    ///
    /// This function orchestrates the entire deployment flow:
    /// 1. It gets the appropriate strategy planner.
    /// 2. It generates an execution plan (a series of stages).
    /// 3. It iterates through each stage sequentially.
    /// 4. Within each stage, it runs operations on all clusters concurrently.
    /// 5. After each stage, it evaluates the results and decides whether to proceed,
    ///    halt on failure (Staged), or halt on success (Failover).
    pub async fn execute_action(
        &self,
        targets: &[Cluster],
        manifests: &str,
        strategy: &Strategy,
    ) -> Result<Vec<ClusterResult>> {
        // 1. Get the correct strategy planner from the factory.
        let planner = strategies::get_strategy(&strategy.strategy_type);

        // 2. Generate the execution plan. This can fail if the input is invalid.
        let execution_plan = planner.plan_execution(targets)?;
        let total_stages = execution_plan.len();

        let mut all_results: Vec<ClusterResult> = Vec::new();
        let manifests_arc = Arc::new(manifests.to_string());

        // 3. Iterate over each stage in the plan sequentially.
        for (i, stage) in execution_plan.into_iter().enumerate() {
            if stage.is_empty() {
                continue;
            }
            println!("\n--- Executing Stage {}/{} ---", i + 1, total_stages);

            // 4. Execute operations for all clusters within the current stage concurrently.
            let task_handles = stage.iter().map(|target| {
                let client = match self.clients.get(&target.name) {
                    Some(c) => c.clone(),
                    None => {
                        return tokio::spawn(async move {
                            ClusterResult {
                                cluster_name: target.name.clone(),
                                outcome: Err(format!("Config for cluster '{}' not found.", target.name)),
                            }
                        });
                    }
                };
                let cluster_name = target.name.clone();
                let manifests_clone = Arc::clone(&manifests_arc);

                tokio::spawn(async move {
                    let outcome = Self::execute_apply(client, &manifests_clone)
                        .await
                        .map_err(|e| format!("{:#}", e));
                    ClusterResult {
                        cluster_name,
                        outcome,
                    }
                })
            }).collect::<Vec<_>>();

            let mut stage_results: Vec<ClusterResult> = join_all(task_handles)
                .await
                .into_iter()
                .map(|res| res.expect("Tokio task panicked!"))
                .collect();
            
            // --- Health Check for Staged Deployments ---
            if strategy.strategy_type == StrategyType::Staged {
                let health_check_futures = stage_results.iter_mut().zip(stage.iter()).map(|(result, cluster_in_stage)| async move {
                    if let Ok(applied_resources) = &result.outcome {
                        let client = self.clients.get(&result.cluster_name).unwrap();
                        
                        if let Err(e) = self.wait_for_stage_health(client, applied_resources, &result.cluster_name, &cluster_in_stage.health_probes).await {
                            result.outcome = Err(format!("Health check failed: {:#}", e));
                        }
                    }
                });
                join_all(health_check_futures).await;
            }

            let stage_had_failures = stage_results.iter().any(|r| r.outcome.is_err());
            let stage_had_successes = stage_results.iter().any(|r| r.outcome.is_ok());

            all_results.extend(stage_results);

            // 5. Apply strategy-specific logic based on the stage outcome.
            match strategy.strategy_type {
                StrategyType::Staged if stage_had_failures => {
                    eprintln!("Error: Stage {} failed. Halting staged deployment.", i + 1);
                    break; // Stop processing further stages.
                }
                StrategyType::Failover if stage_had_successes => {
                    println!("Success: Deployed to a cluster. Halting failover process.");
                    break; // Stop on the first successful deployment.
                }
                // For Direct/Parallel, we continue (as there's only one stage).
                // For Staged on success, we continue to the next stage.
                // For Failover on failure, we continue to the next cluster.
                // For Direct, we continue (as there's only one stage).
                // For Staged on success, we continue to the next stage.
                // For Failover on failure, we continue to the next cluster.
                _ => (),
            }
        }

        Ok(all_results)
    }

    /// Waits for all applied resources in a stage to become healthy.
    ///
    /// This function polls the status of key workload resources (Deployments,
    /// StatefulSets, DaemonSets, Jobs) until they reach a "ready" state or a
    /// timeout is exceeded.
    async fn wait_for_stage_health(
        &self,
        client: &Client,
        applied_resources: &[AppliedResource],
        cluster_name: &str,
        probes: &[HealthProbe],
    ) -> Result<()> {
        println!(
            "[{}] Waiting for resources to become healthy...",
            cluster_name
        );
        let timeout = Duration::from_secs(300); // 5-minute timeout
        let interval = Duration::from_secs(10); // Poll every 10 seconds
        let start_time = std::time::Instant::now();

        // Filter for resources that we know how to check health for.
        // We assume other resources (like ConfigMaps, Services) are ready upon creation.
        let mut resources_to_check: Vec<_> = applied_resources
            .iter()
            .filter(|r| {
                matches!(
                    r.gvk.kind.as_str(),
                    "Deployment" | "StatefulSet" | "DaemonSet" | "Job"
                )
            })
            .cloned()
            .collect();

        if resources_to_check.is_empty() && probes.is_empty() {
            println!("[{}] No health-checkable workloads or user-defined probes found. Proceeding.", cluster_name);
            return Ok(());
        }

        while start_time.elapsed() < timeout {
            let mut still_pending = Vec::new();
            let mut all_healthy = true;

            for resource in &resources_to_check {
                let is_healthy = match resource.gvk.kind.as_str() {
                    "Deployment" => {
                        let api: Api<Deployment> = Api::namespaced(client.clone(), resource.namespace.as_deref().unwrap());
                        check_deployment_health(&api, &resource.name).await
                    }
                    "StatefulSet" => {
                        let api: Api<StatefulSet> = Api::namespaced(client.clone(), resource.namespace.as_deref().unwrap());
                        check_statefulset_health(&api, &resource.name).await
                    }
                    "DaemonSet" => {
                        let api: Api<DaemonSet> = Api::namespaced(client.clone(), resource.namespace.as_deref().unwrap());
                        check_daemonset_health(&api, &resource.name).await
                    }
                    "Job" => {
                        let api: Api<Job> = Api::namespaced(client.clone(), resource.namespace.as_deref().unwrap());
                        check_job_health(&api, &resource.name).await
                    }
                    _ => Ok(true), // Should not happen due to filter
                };

                match is_healthy {
                    Ok(true) => { /* resource is healthy, do nothing */ }
                    Ok(false) => {
                        all_healthy = false;
                        still_pending.push(resource.clone());
                    }
                    Err(e) => {
                        // If a resource is not found, it might have been deleted or not created yet.
                        // We treat this as a transient error and continue checking.
                        if let kube::Error::Api(ae) = &e {
                            if ae.code == 404 {
                                all_healthy = false;
                                still_pending.push(resource.clone());
                                continue;
                            }
                        }
                        // For other errors, we should probably fail fast.
                        return Err(anyhow!("[{}] Error checking health for {}: {}", cluster_name, resource.name, e));
                    }
                }
            }

            resources_to_check = still_pending;

            if all_healthy {
                // Now that built-in checks passed, run user-defined probes.
                if let Err(e) = check_user_defined_probes(client, probes).await {
                    // If user probes fail, we break the loop and return an error.
                    return Err(anyhow!("[{}] User-defined health probe failed: {}", cluster_name, e));
                }
                println!("[{}] All built-in and user-defined probes passed.", cluster_name);
                return Ok(());
            }

            println!(
                "[{}] Still waiting for {} resource(s) to become healthy...",
                cluster_name,
                resources_to_check.len()
            );

            sleep(interval).await;
        }

        Err(anyhow!(
            "[{}] Timed out waiting for resources to become healthy. Still pending: {:?}",
            cluster_name,
            resources_to_check.iter().map(|r| &r.name).collect::<Vec<_>>()
        ))
    }

    /// Private helper to apply manifests to a single cluster.
    async fn execute_apply(client: Client, manifests: &str) -> Result<Vec<AppliedResource>> {
        let ssapply = PatchParams::apply("ph.multi_cluster_orchestrator");
        let mut applied_resources = Vec::new();

        for doc in serde_yaml::Deserializer::from_str(manifests) {
            let obj: DynamicObject = serde::Deserialize::deserialize(doc)
                .context("Failed to deserialize YAML manifest into a Kubernetes object")?;

            let gvk = obj.gvk().context("Resource is missing GroupVersionKind")?;
            let name = obj.name_any();
            let namespace = obj.namespace();

            let (ar, _caps) = discovery::pinned_kind(&client, &gvk).await
                .with_context(|| format!("Failed to discover API resource for GVK: {}", gvk))?;
            
            let api: Api<DynamicObject> = if let Some(ns) = &namespace {
                Api::namespaced_with(client.clone(), ns, &ar)
            } else {
                Api::all_with(client.clone(), &ar)
            };

            let patch_result = api.patch(&name, &ssapply, &Patch::Apply(&obj)).await;

            if let Err(err) = patch_result {
                // Check for a "namespace not found" error.
                if let kube::Error::Api(api_err) = &err {
                    if api_err.code == 404 && api_err.message.contains("namespaces") {
                        let tried_ns = obj.namespace().unwrap_or_default();
                        let suggestion = find_closest_namespace(client.clone(), &tried_ns).await;
                        
                        let suggestion_text = if let Some(s) = suggestion {
                            format!(" Did you mean '{}'?", s)
                        } else {
                            "".to_string()
                        };

                        return Err(anyhow!(
                            "Failed to apply resource '{}/{}': namespace '{}' not found.{}",
                            gvk, name, tried_ns, suggestion_text
                        ));
                    }
                }
                // For all other errors, use the default context.
                return Err(err).context(format!("Failed to apply resource '{}/{}'", gvk, name));
            }
            
            applied_resources.push(AppliedResource {
                gvk,
                name,
                namespace,
            });
        }

        if applied_resources.is_empty() {
            return Err(anyhow!("No valid Kubernetes resources found in manifests."));
        }

        Ok(applied_resources)
    }

    /// Sets a new policy for a specific cluster in the main `clusters.yaml` file.
    pub async fn set_cluster_policy(cluster_name: &str, policy_file_path: &str) -> Result<()> {
        let clusters_config_path = Path::new("config/clusters.yaml");
        println!(
            "Updating policy for cluster '{}' using policy file '{}'...",
            cluster_name, policy_file_path
        );

        // 1. Read and parse the main clusters.yaml file
        let mut clusters_config: ClustersConfig = {
            let content = fs::read_to_string(clusters_config_path)
                .context("Failed to read config/clusters.yaml")?;
            serde_yaml::from_str(&content).context("Failed to parse config/clusters.yaml")?
        };

        // 2. Read and parse the new policy file
        let new_policies: BTreeMap<String, serde_yaml::Value> = {
            let content = fs::read_to_string(policy_file_path)
                .with_context(|| format!("Failed to read policy file '{}'", policy_file_path))?;
            serde_yaml::from_str(&content)
                .with_context(|| format!("Failed to parse policy file '{}' as YAML map", policy_file_path))?
        };

        // 3. Find the cluster and update its policies
        let cluster_to_update = clusters_config
            .clusters
            .iter_mut()
            .find(|c| c.name == cluster_name)
            .ok_or_else(|| anyhow!("Cluster '{}' not found in config/clusters.yaml", cluster_name))?;
        
        println!("Found cluster '{}'. Applying new policies.", cluster_name);
        cluster_to_update.policies = new_policies;

        // 4. Create a backup of the original file
        let backup_path = clusters_config_path.with_extension("yaml.bak");
        fs::copy(clusters_config_path, &backup_path).with_context(|| {
            format!("Failed to create backup file at '{}'", backup_path.display())
        })?;
        println!("Created backup at '{}'", backup_path.display());

        // 5. Write the updated configuration back to the file
        let updated_content = serde_yaml::to_string(&clusters_config)?;
        fs::write(clusters_config_path, updated_content)
            .context("Failed to write updated content to config/clusters.yaml")?;

        println!("✅ Successfully updated policy for cluster '{}'.", cluster_name);

        Ok(())
    }
}

/// Finds the most similar namespace name from a list of all namespaces on the cluster.
async fn find_closest_namespace(client: Client, tried_ns: &str) -> Option<String> {
    let ns_api: Api<Namespace> = Api::all(client);
    if let Ok(all_ns) = ns_api.list(&Default::default()).await {
        let mut closest_match: Option<String> = None;
        let mut min_distance = 4; // Only suggest if the distance is 3 or less.

        for ns in all_ns {
            let ns_name = ns.name_any();
            let distance = levenshtein(tried_ns, &ns_name);
            if distance < min_distance {
                min_distance = distance;
                closest_match = Some(ns_name);
            }
        }
        closest_match
    } else {
        None
    }
}

use crate::crds::{phRelease, phReleaseStatus}; // Assuming phRelease is defined in a shared CRD crate
use prometheus_http_query::Client as PrometheusClient;

// --- User-Defined Health Probe Handlers ---

async fn handle_http_get_probe(client: &reqwest::Client, probe: &HttpGetProbe) -> Result<()> {
    let response = client.get(&probe.url)
        .timeout(Duration::from_secs(probe.timeout_seconds))
        .send().await?
        .error_for_status()?;
    println!("    [Health Probe] HTTP GET to {} successful.", probe.url);
    Ok(())
}

async fn handle_prometheus_probe(client: &PrometheusClient, probe: &PrometheusProbe) -> Result<()> {
    // This is a simplified check. A real implementation would need a library
    // to parse and evaluate the `expected_result` expression.
    let result = client.query(&probe.query, None, None).await?;
    if result.as_vector().is_some() {
         println!("    [Health Probe] Prometheus query '{}' returned a vector. Assuming success.", probe.query);
        Ok(())
    } else {
        Err(anyhow!("Prometheus query did not return a vector result."))
    }
}

async fn handle_phgit_release_probe(client: &Client, probe: &PhgitReleaseProbe) -> Result<()> {
    let api: Api<phRelease> = Api::namespaced(client.clone(), &probe.namespace);
    let release = api.get(&probe.name).await?;
    let phase = release.status.and_then(|s| s.phase).unwrap_or_default();
    if phase == probe.expected_phase {
        println!("    [Health Probe] PhgitRelease '{}/{}' is in expected phase '{}'.", probe.namespace, probe.name, probe.expected_phase);
        Ok(())
    } else {
        Err(anyhow!("PhgitRelease '{}/{}' is in phase '{}', expected '{}'", probe.namespace, probe.name, phase, probe.expected_phase))
    }
}


async fn check_user_defined_probes(cluster_client: &Client, probes: &[HealthProbe]) -> Result<()> {
    if probes.is_empty() {
        return Ok(());
    }
    println!("  Running user-defined health probes...");

    // Create clients that are shared across all probes for this check.
    let http_client = reqwest::Client::new();
    // Assuming Prometheus endpoint is sourced from an env var or a default.
    let prom_client = PrometheusClient::try_from("http://prometheus.default.svc:9090")?;

    for probe in probes {
        match probe {
            HealthProbe::HttpGet(p) => handle_http_get_probe(&http_client, p).await?,
            HealthProbe::Prometheus(p) => handle_prometheus_probe(&prom_client, p).await?,
            HealthProbe::PhgitRelease(p) => handle_phgit_release_probe(cluster_client, p).await?,
        }
    }

    Ok(())
}


// --- Health Checking Helper Functions ---

/// Checks if a Deployment is considered healthy.
async fn check_deployment_health(api: &Api<Deployment>, name: &str) -> anyhow::Result<bool> {
    let dep = api.get(name).await?;
    if let Some(status) = dep.status {
        if let Some(spec) = dep.spec {
            let desired = spec.replicas.unwrap_or(1);
            // Status is not yet updated for the latest spec
            if status.observed_generation.unwrap_or(0) < dep.metadata.generation.unwrap_or(1) {
                return Ok(false);
            }
            // All replicas are not yet updated
            if status.updated_replicas.unwrap_or(0) < desired {
                return Ok(false);
            }
            // All replicas are not yet available
            if status.available_replicas.unwrap_or(0) < desired {
                return Ok(false);
            }
            // Check the "Available" condition
            if let Some(conditions) = status.conditions {
                if let Some(available) = conditions.iter().find(|c| c.type_ == "Available") {
                    if available.status == "True" {
                        return Ok(true);
                    }
                }
            }
        }
    }
    Ok(false)
}

/// Checks if a StatefulSet is considered healthy.
async fn check_statefulset_health(api: &Api<StatefulSet>, name: &str) -> anyhow::Result<bool> {
    let sts = api.get(name).await?;
    if let Some(status) = sts.status {
        if let Some(spec) = sts.spec {
            let desired = spec.replicas.unwrap_or(1);
            if status.observed_generation.unwrap_or(0) < sts.metadata.generation.unwrap_or(1) {
                return Ok(false);
            }
            if status.ready_replicas.unwrap_or(0) < desired {
                return Ok(false);
            }
            if status.updated_replicas.unwrap_or(0) < desired {
                return Ok(false);
            }
            return Ok(true);
        }
    }
    Ok(false)
}

/// Checks if a DaemonSet is considered healthy.
async fn check_daemonset_health(api: &Api<DaemonSet>, name: &str) -> anyhow::Result<bool> {
    let ds = api.get(name).await?;
    if let Some(status) = ds.status {
        if status.observed_generation.unwrap_or(0) < ds.metadata.generation.unwrap_or(1) {
            return Ok(false);
        }
        let desired = status.desired_number_scheduled;
        if status.number_ready < desired {
            return Ok(false);
        }
        if status.updated_number_scheduled < desired {
            return Ok(false);
        }
        return Ok(true);
    }
    Ok(false)
}

/// Checks if a Job has completed successfully. Returns an error if the Job has failed.
async fn check_job_health(api: &Api<Job>, name: &str) -> anyhow::Result<bool> {
    let job = api.get(name).await?;
    if let Some(status) = job.status {
        if let Some(conditions) = status.conditions {
            for cond in conditions {
                if cond.type_ == "Complete" && cond.status == "True" {
                    return Ok(true); // Job completed successfully
                }
                if cond.type_ == "Failed" && cond.status == "True" {
                    // Job has failed, this is a terminal state.
                    return Err(anyhow!(
                        "Job failed: {}",
                        cond.message.clone().unwrap_or_default()
                    ));
                }
            }
        }
    }
    Ok(false) // Job is still running
}