# ph-operator

This directory contains the source code for the `ph-operator`, a Kubernetes operator that manages the lifecycle of custom resources used by the `phgit` ecosystem.

## Controllers

The operator runs several controllers, each responsible for a specific Custom Resource Definition (CRD):

-   **`release_controller`**: Manages `phRelease` resources for progressive delivery (canary deployments).
-   **`preview_controller`**: Manages `phPreview` resources for ephemeral preview environments.
-   **`pipeline_controller`**: Manages `phPipeline` resources for CI/CD pipelines.
-   **`dr_controller`**: Manages `PhgitDisasterRecovery` resources for multi-cluster failover.
-   **`rbac_policy_controller`**: Manages `PhgitRbacPolicy` resources for declarative RBAC.
-   **`audit_controller`**: Manages `PhgitAudit` resources for audit logging.
-   **`autoheal_controller`**: Manages `phAutoHealRule` resources for automated remediation.

## Auto-Heal Webhook Configuration

The `autoheal_controller` includes an embedded webhook server that listens for alerts from a Prometheus Alertmanager instance. This allows you to trigger automated runbooks in response to specific alerts.

**Endpoint:** `/webhook`
**Port:** `8080`

### Example Alertmanager Configuration

To configure Alertmanager to send alerts to the `ph-operator`, you need to add a webhook receiver to your `alertmanager.yml` configuration file. The operator must be accessible from the Alertmanager instance, typically via a Kubernetes Service.

Assuming the operator is exposed via a service named `ph-operator-service` in the `phgit-system` namespace, the configuration would look like this:

```yaml
global:
  resolve_timeout: 5m

route:
  receiver: 'default-receiver'
  group_by: ['alertname', 'cluster', 'service']
  # Add a route for alerts that should trigger auto-healing.
  # This example routes any alert with the label `severity=critical` to the ph-operator.
  routes:
    - receiver: 'ph-operator-webhook'
      match:
        severity: critical
      continue: true # Set to 'true' if you also want other receivers to get the alert

receivers:
- name: 'default-receiver'
  # Your default receiver configuration (e.g., Slack, PagerDuty)
  slack_configs:
  - api_url: 'https://hooks.slack.com/services/...'
    channel: '#alerts'

- name: 'ph-operator-webhook'
  webhook_configs:
  - url: 'http://ph-operator-service.phgit-system.svc.cluster.local:8080/webhook'
    # send_resolved: false # Optional: prevents sending notifications when an alert is resolved
```

When an alert fires that matches the route, Alertmanager will send a POST request to the operator's `/webhook` endpoint. The `autoheal_controller` will then look for a `phAutoHealRule` with a `triggerName` that matches the `alertname` label in the alert. If a match is found, it will execute the actions defined in the rule.

## OpenTelemetry Tracing Configuration

The operator is instrumented with OpenTelemetry to provide end-to-end distributed tracing for operations that start from the `phgit` CLI and are handled by the operator's controllers (e.g., `preview_controller`, `release_controller`).

### Jaeger Exporter

The operator is configured by default to export traces to a **Jaeger Agent** via UDP on port `6831`. For traces to be collected, the operator's pod must be able to reach a Jaeger Agent.

There are two common ways to achieve this:

1.  **Sidecar Injection**: The recommended approach is to have a sidecar injector (like the one provided by the official Jaeger Operator) automatically inject a `jaeger-agent` container into the `ph-operator`'s pod.

2.  **DaemonSet**: You can run the `jaeger-agent` as a DaemonSet on your Kubernetes cluster. You must then ensure the `ph-operator` pod is deployed with `hostNetwork: true` or that the appropriate `hostPort` is configured so it can reach the agent on the node's IP address. Environment variables can be used to configure the exporter if the agent is not on `localhost`.
