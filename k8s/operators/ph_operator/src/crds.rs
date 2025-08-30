/*
* Copyright (C) 2025 Pedro Henrique / phkaiser13
*
* File: k8s/operators/ph_operator/src/crds.rs
*
* This file defines the Rust data structures that correspond to our Custom
* Resource Definitions (CRDs). By using the `kube::CustomResource` derive macro,
* we create a strongly-typed representation of our custom APIs, enabling safe
* and idiomatic interaction with the Kubernetes API server.
*
* Architecture:
* - Each top-level struct decorated with `#[derive(CustomResource)]` (e.g.,
*   `phPreview`, `phRelease`, `phPipeline`) represents a single API Kind.
* - The `#[kube(...)]` attribute provides the necessary metadata to map the Rust
*   struct to its corresponding CRD in the cluster (group, version, kind). This
*   metadata MUST exactly match the definitions in the YAML CRD files.
* - The standard Kubernetes object structure is followed by separating the user's
*   desired state (`spec`) from the operator's observed state (`status`).
* - This version extends the `phRelease` resource to support intelligent,
*   automated rollouts. New structs like `Analysis` and `Metric` have been
*   added to the `spec` to allow users to define health checks based on
*   Prometheus metrics.
* - The `status` for `phRelease` has also been enriched with `AnalysisRunStatus`
*   to track the progress of these health checks, enabling automated promotion
*   or rollback logic within the release_controller.
* - A new `phAutoHealRule` CRD is introduced to define auto-healing policies.
*   This allows the operator to react to Prometheus alerts by executing predefined
*   runbooks, creating a closed-loop remediation system.
* - `serde` attributes are used to map between idiomatic Rust `snake_case` and
*   idiomatic Kubernetes `camelCase`.
* - `schemars` is leveraged to automatically generate an OpenAPI v3 schema from the
*   Rust types, which is embedded into the CRD manifest for server-side validation.
*
* SPDX-License-Identifier: Apache-2.0
*/

use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// --- phPreview Custom Resource Definition ---

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "ph.io",
    version = "v1alpha1",
    kind = "phPreview",
    namespaced,
    status = "phPreviewStatus",
    printcolumn = r#"{"name":"Status", "type":"string", "jsonPath":".status.conditions[-1:].type"}"#,
    printcolumn = r#"{"name":"Namespace", "type":"string", "jsonPath":".status.namespace"}"#,
    printcolumn = r#"{"name":"Age", "type":"date", "jsonPath":".metadata.creationTimestamp"}"#,
    shortname = "pgprv"
)]
#[serde(rename_all = "camelCase")]
pub struct phPreviewSpec {
    pub repo_url: String,
    pub branch: String,
    pub manifest_path: String,
    pub app_name: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct phPreviewStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    pub conditions: Vec<StatusCondition>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatusCondition {
    #[serde(rename = "type")]
    pub type_: String,
    pub message: String,
}

impl StatusCondition {
    pub fn new(type_: String, message: String) -> Self {
        Self { type_, message }
    }
}


// --- phRelease Custom Resource Definition ---

/// Defines security-related configurations for a release.
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct Security {
    /// Configures Cosign signature verification for the container image.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_verification: Option<SignatureVerification>,
}

/// Specifies the details for signature verification.
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignatureVerification {
    /// A reference to a Kubernetes Secret containing the public key for verification.
    pub public_key_secret_ref: PublicKeySecretRef,
}

/// Holds the reference to a key within a Kubernetes Secret.
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PublicKeySecretRef {
    /// The name of the Secret in the same namespace.
    pub name: String,
    /// The key within the Secret's data that contains the PEM-encoded public key.
    pub key: String,
}

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "ph.io",
    version = "v1alpha1",
    kind = "phRelease",
    namespaced,
    status = "phReleaseStatus",
    shortname = "pgrls"
)]
#[serde(rename_all = "camelCase")]
pub struct phReleaseSpec {
    pub app_name: String,
    pub version: String,
    pub strategy: ReleaseStrategy,
    /// Security-related configurations for the release.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security: Option<Security>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseStrategy {
    #[serde(rename = "type")]
    pub strategy_type: StrategyType,
    pub canary: Option<CanaryStrategy>,
    pub blue_green: Option<BlueGreenStrategy>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub enum StrategyType {
    Canary,
    BlueGreen,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CanaryStrategy {
    pub traffic_percent: u8,
    #[serde(default)]
    pub auto_increment: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis: Option<Analysis>,
    #[serde(default)]
    pub auto_promote: bool,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub interval: String,
    pub threshold: u32,
    pub max_failures: u32,
    pub metrics: Vec<Metric>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Metric {
    pub name: String,
    pub query: String,
    pub on_success: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predictive_analysis: Option<PredictiveAnalysis>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PredictiveAnalysis {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trend_threshold: Option<f64>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BlueGreenStrategy {
    #[serde(default)]
    pub auto_promote: bool,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum ReleasePhase {
    Progressing,
    Paused,
    Succeeded,
    Failed,
    Promoting,
    RollingBack,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct phReleaseStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<ReleasePhase>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stable_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canary_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traffic_split: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_step: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis_run: Option<AnalysisRunStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progressing_start_time: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisRunStatus {
    pub success_count: u32,
    pub failure_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_check: Option<String>, // Using String to align with Kubernetes API conventions for timestamps
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric_history: Option<Vec<MetricHistory>>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetricHistory {
    pub name: String,
    pub values: Vec<HistoricalValue>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoricalValue {
    pub timestamp: String,
    pub value: f64,
}


// --- phPipeline Custom Resource Definition ---

/// # phPipeline
/// Represents a declarative CI/CD pipeline.
/// Creating a `phPipeline` resource defines a pipeline that the operator
/// can trigger and execute based on Git events or manual requests.
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "ph.io",
    version = "v1alpha1",
    kind = "phPipeline",
    namespaced,
    status = "phPipelineStatus",
    printcolumn = r#"{"name":"Status", "type":"string", "jsonPath":".status.phase"}"#,
    printcolumn = r#"{"name":"Age", "type":"date", "jsonPath":".metadata.creationTimestamp"}"#,
    shortname = "pgpipe"
)]
#[serde(rename_all = "camelCase")]
pub struct phPipelineSpec {
    /// A list of stages to be executed sequentially.
    pub stages: Vec<PipelineStage>,
}

/// A single stage in the pipeline, containing one or more steps.
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PipelineStage {
    /// The name of the stage (e.g., "build", "test", "deploy").
    pub name: String,
    /// The steps to be executed within this stage.
    pub steps: Vec<PipelineStep>,
}

/// A single step in a pipeline stage, corresponding to a Kubernetes Job.
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PipelineStep {
    /// The name of the step.
    pub name: String,
    /// The special type of the step, e.g., 'generate-sbom'.
    #[serde(rename = "stepType", default, skip_serializing_if = "Option::is_none")]
    pub step_type: Option<String>,
    /// The container image to run for this step.
    pub image: String,
    /// The command to execute. If not provided, the image's entrypoint is used.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<String>,
    /// The arguments to pass to the command.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Artifacts produced by this step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outputs: Option<Vec<PipelineStepOutput>>,
}

/// Defines an output artifact from a pipeline step.
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PipelineStepOutput {
    /// The logical name of the output artifact.
    pub name: String,
    /// The path within the container where the artifact can be found.
    pub path: String,
}

/// The observed state of the phPipeline resource.
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct phPipelineStatus {
    /// The current phase of the pipeline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<PipelinePhase>,
    /// The timestamp when the pipeline started execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time: Option<String>,
    /// The timestamp when the pipeline completed or failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_time: Option<String>,
    /// The index of the current stage being executed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_stage_index: Option<usize>,
    /// The index of the current step being executed within the stage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_step_index: Option<usize>,
}

/// An enum representing the possible phases of a pipeline's lifecycle.
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum PipelinePhase {
    Pending,
    Running,
    Succeeded,
    Failed,
}

// --- phAutoHealRule Custom Resource Definition ---

/// # phAutoHealRule
/// Defines an automated healing rule that triggers a runbook in response to a specific alert.
/// The ph-operator watches for these resources and executes the defined actions when a
/// corresponding Prometheus alert is received via Alertmanager.
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "ph.kaiser.io",
    version = "v1alpha1",
    kind = "phAutoHealRule",
    namespaced,
    status = "phAutoHealRuleStatus",
    printcolumn = r#"{"name":"Trigger", "type":"string", "jsonPath":".spec.triggerName"}"#,
    printcolumn = r#"{"name":"Status", "type":"string", "jsonPath":".status.state"}"#,
    printcolumn = r#"{"name":"Last Execution", "type":"date", "jsonPath":".status.lastExecutionTime"}"#,
    printcolumn = r#"{"name":"Age", "type":"date", "jsonPath":".metadata.creationTimestamp"}"#,
    shortname = "phahr"
)]
#[serde(rename_all = "camelCase")]
pub struct phAutoHealRuleSpec {
    /// The name of the alert/trigger that activates this rule. This should match
    /// the `alertname` label from a Prometheus alert.
    pub trigger_name: String,

    /// The cooldown period after an execution to prevent the rule from firing
    /// too frequently. The format should be a duration string like "5m", "1h", "30s".
    pub cooldown: String,

    /// A list of actions to execute sequentially when the rule is triggered.
    pub actions: Vec<ActionSpec>,
}

/// Defines a single action to be performed by the auto-heal controller.
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct ActionSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redeploy: Option<RedeployAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale_up: Option<ScaleUpAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runbook: Option<RunbookSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notify: Option<NotifyAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<SnapshotAction>,
}

/// Defines the parameters for a diagnostic snapshot action.
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotAction {
    /// A name for the snapshot, used for file naming.
    pub name: String,
    /// Whether to include pod logs in the snapshot.
    #[serde(default)]
    pub include_logs: bool,
    /// Whether to include OpenTelemetry traces in the snapshot.
    #[serde(default)]
    pub include_traces: bool,
    /// Whether to trigger and include a database dump in the snapshot.
    #[serde(default)]
    pub include_db_dump: bool,
}

/// Defines the parameters for a notification action.
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct NotifyAction {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slack: Option<SlackNotify>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue: Option<IssueNotify>,
}

/// Parameters for sending a Slack notification.
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct SlackNotify {
    /// The name of a Secret in the same namespace containing a 'webhookUrl' key.
    pub webhook_url_secret_ref: String,
    /// A message template. Can include placeholders like {{ .alert.name }}.
    pub message: String,
}

/// Parameters for creating an issue in a tracker.
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct IssueNotify {
    /// The project key or name in the issue tracker.
    pub project: String,
    /// A title template for the issue.
    pub title: String,
    /// A body/description template for the issue.
    pub body: String,
}

/// Action to redeploy a target.
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
pub struct RedeployAction {
    pub target: String,
}

/// Action to scale up a target.
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct ScaleUpAction {
    pub target: String,
    pub replicas: i32,
}

/// Contains the details for executing a specific runbook (a script).
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct RunbookSpec {
    /// The name of the script to execute. This script is expected to be available
    /// to the operator, for example, from a ConfigMap.
    pub script_name: String,
}

/// An enum representing the possible states of an auto-heal rule's lifecycle.
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum HealState {
    /// The rule is active and waiting for a trigger.
    Idle,
    /// The rule has been triggered by an alert and is pending action.
    Triggered,
    /// The action (e.g., runbook) is currently being executed.
    Executing,
    /// The rule has recently executed and is in a cooldown period.
    Cooldown,
    /// The last execution of the rule failed.
    Failed,
}

/// The observed state of the phAutoHealRule resource, managed by the operator.
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct phAutoHealRuleStatus {
    /// The current state of the auto-heal rule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<HealState>,

    /// The timestamp of the last time the rule's actions were executed.
    /// Stored in RFC 3339 format.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_execution_time: Option<String>,

    /// A counter for the total number of times this rule has been successfully executed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executions_count: Option<u32>,

    /// Human-readable status conditions for the resource.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<StatusCondition>,
}

// --- PhgitAudit Custom Resource Definition ---

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "ph.io",
    version = "v1alpha1",
    kind = "PhgitAudit",
    scope = "Cluster",
    printcolumn = r#"{"name":"Component", "type":"string", "jsonPath":".spec.component"}"#,
    printcolumn = r#"{"name":"Verb", "type":"string", "jsonPath":".spec.verb"}"#,
    printcolumn = r#"{"name":"User", "type":"string", "jsonPath":".spec.actor.user"}"#,
    printcolumn = r#"{"name":"Age", "type":"date", "jsonPath":".spec.timestamp"}"#,
    shortname = "pgaud"
)]
#[serde(rename_all = "camelCase")]
pub struct PhgitAuditSpec {
    pub timestamp: String,
    pub verb: String,
    pub component: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<Actor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub details: std::collections::BTreeMap<String, String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct Actor {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ip: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}


// --- PhgitDisasterRecovery Custom Resource Definition ---

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "ph.io",
    version = "v1alpha1",
    kind = "PhgitDisasterRecovery",
    namespaced,
    status = "PhgitDisasterRecoveryStatus",
    shortname = "phdr"
)]
#[serde(rename_all = "camelCase")]
pub struct PhgitDisasterRecoverySpec {
    pub primary_cluster: ClusterRef,
    pub dr_cluster: ClusterRef,
    pub target_application: TargetApplication,
    pub policy: DRPolicy,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterRef {
    pub kubeconfig_secret_ref: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TargetApplication {
    pub deployment_name: String,
    pub namespace: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DRPolicy {
    pub health_check: HealthCheckPolicy,
    pub failover_trigger: FailoverTrigger,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheckPolicy {
    pub prometheus_query: String,
    pub interval: String,
    pub failure_threshold: u32,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum FailoverTrigger {
    Automatic,
    Manual,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct PhgitDisasterRecoveryStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_cluster: Option<ActiveCluster>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<DRState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_health_check_time: Option<String>,
    #[serde(default)]
    pub consecutive_failures: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<StatusCondition>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum ActiveCluster {
    Primary,
    DR,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum DRState {
    Monitoring,
    Degraded,
    FailingOver,
    ActiveOnDR,
    Failed,
}

// --- PhgitRbacPolicy Custom Resource Definition ---

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "ph.io",
    version = "v1alpha1",
    kind = "PhgitRbacPolicy",
    namespaced,
    status = "PhgitRbacPolicyStatus",
    printcolumn = r#"{"name":"Role", "type":"string", "jsonPath":".spec.role"}"#,
    printcolumn = r#"{"name":"Subject Kind", "type":"string", "jsonPath":".spec.subject.kind"}"#,
    printcolumn = r#"{"name":"Subject Name", "type":"string", "jsonPath":".spec.subject.name"}"#,
    printcolumn = r#"{"name":"Status", "type":"string", "jsonPath":".status.conditions[-1:].type"}"#,
    shortname = "phrbac"
)]
#[serde(rename_all = "camelCase")]
pub struct PhgitRbacPolicySpec {
    pub role: String,
    pub subject: Subject,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Subject {
    pub kind: String,
    pub name: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct PhgitRbacPolicyStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding_name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<StatusCondition>,
}