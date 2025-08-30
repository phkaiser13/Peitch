# `ph` Commands for Kubernetes (`k8s.md`)

This document provides a comprehensive overview of all `ph` commands related to Kubernetes, including their purpose, usage, and the underlying mechanisms.

## 1\. `ph kube`: Core Kubernetes Commands

The `ph kube` command group is the primary interface for interacting with Kubernetes clusters. It provides functionalities for synchronization, deployment, access control, and multi-cluster orchestration.

### 1.1. `ph kube sync`

Synchronizes manifests from a Git repository to a cluster. It can detect drift, create Pull Requests, or apply changes directly.

**Usage:**

```bash
ph kube sync --path <path> --cluster <cluster-name> [--context <context>] [--dry-run] [--apply] [--force] [--skip-signature-verification]
```

**Options:**

  * `--path`: Path to the manifests. (Required)
  * `--cluster`: Target cluster name.
  * `--context`: Kubernetes context to use.
  * `--dry-run`: Simulate the synchronization without applying changes.
  * `--apply`: Apply the changes directly to the cluster.
  * `--force`: Force the application of changes.
  * `--skip-signature-verification`: Skip commit signature verification.

### 1.2. `ph kube drift`

Detects configuration drift between a Git repository and a cluster.

**Usage:**

```bash
ph kube drift --cluster <cluster-name> [--path <path>] [--since <time>] [--label <label>] [--open-pr] [--auto-apply]
```

### 1.3. `ph kube rollout`

Manages application rollouts with advanced strategies.

**Subcommands:**

  * `start`: Initiates a new release.
  * `status`: Checks the status of a rollout.
  * `promote`: Manually promotes an ongoing release.
  * `rollback`: Manually rolls back an ongoing release.
  * `plan`: Shows the execution plan for a rollout.

### 1.4. `ph kube multi`

Orchestrates actions across multiple clusters simultaneously.

**Usage:**

```bash
ph kube multi apply --clusters <cluster1,cluster2,...> --path <manifest-path> [--strategy <strategy>]
```

### 1.5. Cluster Management

  * `ph kube list-clusters`: Lists all clusters defined in the configuration.
  * `ph kube use-cluster <cluster-name>`: Sets the default cluster for subsequent commands.
  * `ph kube info [<cluster-name>]`: Displays information about the current or a specific cluster.

### 1.6. Access Control (RBAC)

  * `ph kube grant --role <role> --subject <user/group> [--cluster <cluster-name>]`: Grants a predefined role to a user or group by creating a declarative `PhgitRbacPolicy` resource. The `rbac-policy-controller` then reconciles this policy into a `RoleBinding` in the cluster.
  * `ph kube revoke --role <role> --subject <user/group> [--cluster <cluster-name>]`: Revokes a role from a user or group by deleting the corresponding `PhgitRbacPolicy` resource.

### 1.7. `ph kube failover`

Initiates a manual failover of an application from one cluster to another.

**Usage:**

```bash
ph kube failover --app <app-name> --from <source-cluster> --to <destination-cluster>
```

## 2\. `ph preview`: Preview Environments

Commands for managing ephemeral preview environments for pull requests.

  * `ph preview create --pr <pr-number> --repo <repo-url> --commit-sha <sha> [--ttl <hours>]`: Creates a new preview environment.
  * `ph preview status --pr <pr-number>`: Gets the status of a preview environment.
  * `ph preview teardown --pr <pr-number>`: Destroys a preview environment.
  * `ph preview logs --pr <pr-number> --component <component-name>`: Gets logs from a component in the preview.
  * `ph preview exec --pr <pr-number> --component <component-name> -- <command> [args...]`: Executes a command in a preview container.
  * `ph preview extend --pr <pr-number> --ttl <hours>`: Extends the TTL of a preview environment.
  * `ph preview gc --max-age-hours <hours>`: Garbage collects expired environments.

## 3\. `ph health` & `ph autoheal`: Health and Remediation

  * `ph health check --app <app-name> --cluster <cluster-name> [--full]`: Performs a health check on an application.
  * `ph autoheal enable --on <trigger> --actions <script-name> --cooldown <duration>`: Configures an auto-healing rule.

## 4\. `ph runners`: CI/CD Runner Management

  * `ph runners scale --min <min> --max <max> --autoscale-metric <metric> --cluster <cluster-name>`: Adjusts the scaling parameters of the runner deployment.
  * `ph runners hpa install --namespace <namespace> --metric <metric> --target <value>`: Installs the HorizontalPodAutoscaler for the runners.

## 5\. Kubernetes Custom Resources (CRDs)

`ph` leverages several CRDs to manage its workflows declaratively in a Kubernetes-native way.

  * **`phRelease`**: The cornerstone of the progressive delivery system, it provides a high-level API for managing complex release strategies like Canary and Blue-Green deployments.
  * **`phPreview`**: Defines and manages ephemeral preview environments, extending the Kubernetes API to create a new, declarative API endpoint that can be managed with standard tools like kubectl.
  * **`phPipeline`**: Provides a declarative, Kubernetes-native way to define entire CI/CD pipelines as code.
  * **`phAutoHealRule`**: Defines an automated healing rule that triggers a runbook in response to a specific alert.
  * **`PhgitDisasterRecovery`**: Orchestrates automated failover between two Kubernetes clusters.
  * **`PhgitRbacPolicy`**: Provides a declarative way to manage RBAC, making access control policies auditable and GitOps-friendly.

  ```mermaid
graph LR
  %% hubs (passo diagonal) — cada hub será ancorado ao respectivo subgraph
  PH1["ph\nkube"]
  PH2["preview"]
  PH3["health"]
  PH4["autoheal"]
  PH5["runners"]

  %% espaços invisíveis para criar o efeito diagonal
  PH1 --> i1(( ))
  i1 --> PH2
  PH2 --> i2(( ))
  i2 --> PH3
  PH3 --> i3(( ))
  i3 --> PH4
  PH4 --> i4(( ))
  i4 --> PH5

  class i1,i2,i3,i4 invisible
  classDef invisible fill:none,stroke:none,stroke-width:0

  %% ancoragens (liga cada hub ao nó raiz do seu subgraph)
  PH1 --- A
  PH2 --- B
  PH3 --- C
  PH4 --- D
  PH5 --- E

  %% ---- subgraphs ----
  subgraph "kube"
    direction TB
    A[kube]
    A --> A1[sync]
    A --> A2[drift]
    A --> A3[rollout]
    A --> A4[multi]
    A --> A5[list-clusters]
    A --> A6[use-cluster]
    A --> A7[info]
    A --> A8[grant]
    A --> A9[revoke]
    A --> A10[failover]
  end

  subgraph "rollout"
    direction TB
    A3 --> A3a[start]
    A3 --> A3b[status]
    A3 --> A3c[promote]
    A3 --> A3d[rollback]
    A3 --> A3e[plan]
  end

  subgraph "preview"
    direction TB
    B[preview]
    B --> B1[create]
    B --> B2[status]
    B --> B3[teardown]
    B --> B4[logs]
    B --> B5[exec]
    B --> B6[extend]
    B --> B7[gc]
  end

  subgraph "health"
    direction TB
    C[health]
    C --> C1[check]
  end

  subgraph "autoheal"
    direction TB
    D[autoheal]
    D --> D1[enable]
  end

  subgraph "runners"
    direction TB
    E[runners]
    E --> E1[scale]
    E --> E2[hpa install]
  end

```