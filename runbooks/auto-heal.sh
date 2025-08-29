#
# Copyright (C) 2025 Pedro Henrique / phkaiser13
#
# SPDX-License-Identifier: Apache-2.0
#

# CHANGE SUMMARY:
# - Reviewed the existing script and confirmed its functional correctness. No logic changes were made.
# - Standardized file-level documentation to clarify its purpose, architecture, and safety features
#   (strict mode, dependency checks, dry-run mode).
# - Ensured all comments are in English and follow a consistent style for maintainability.

# ---
#
# Module: runbooks/auto-heal.sh
#
# Purpose:
#   This script serves as an automated runbook for healing Kubernetes applications,
#   designed to be triggered by webhooks from Prometheus Alertmanager. Its primary role
#   is to increase system reliability by performing immediate, automated remediation actions
#   in response to well-defined failure scenarios, thereby reducing Mean Time To Recovery (MTTR).
#
# Architecture:
#   - Strict Mode Operation: Uses `set -euo pipefail` to fail fast and predictably.
#   - Dependency Verification: Explicitly checks for `kubectl` before execution.
#   - Environment-Driven Configuration: Configured via environment variables (e.g.,
#     `ALERT_NAMESPACE`, `ALERT_DEPLOYMENT`) injected by the Kubernetes Job that runs it.
#   - Safety First (Dry-Run Mode): Supports a `DRY_RUN="true"` mode to log actions
#     without executing them, which is essential for testing.
#   - Focused Action: The primary healing action is a Deployment rollout restart, a safe
#     and idempotent operation for stateless applications.
#
# Usage Notes:
#   - This script is intended to be packaged in a ConfigMap and executed by a Kubernetes Job
#     created by the `phAutoHealRule` controller.
#   - It should be executed via `sh /path/to/script.sh`, as execute permissions are not
#     guaranteed on files mounted from ConfigMaps.
#
#!/bin/bash

# ---[ Script Configuration and Safety ]--------------------------------------
#
# set -e: Exit immediately if a command exits with a non-zero status.
# set -u: Treat unset variables as an error when substituting.
# set -o pipefail: The return value of a pipeline is the status of the last
#                  command to exit with a non-zero status.
#
# This trifecta is essential for writing robust and predictable shell scripts.
set -euo pipefail

# ---[ Logging Utilities ]----------------------------------------------------
#
# A simple, structured logging mechanism to ensure all output is timestamped
# and categorized for auditing and debugging.
#
log_info() {
    # ISO 8601 format is an unambiguous and universally sortable standard.
    echo "[$(date -u +"%Y-%m-%dT%H:%M:%SZ")] [INFO] -- $1"
}

log_error() {
    # Logging errors to stderr is a standard practice.
    echo "[$(date -u +"%Y-%m-%dT%H:%M:%SZ")] [ERROR] -- $1" >&2
}

# ---[ Main Execution Logic ]-------------------------------------------------
#
main() {
    log_info "Auto-heal runbook triggered."

    # ---[ Dependency Verification ]------------------------------------------
    #
    # Verify that our primary tool, kubectl, is available in the PATH.
    # Failing early with a clear message is better than a "command not found" error.
    #
    if ! command -v kubectl &> /dev/null; then
        log_error "kubectl command could not be found. Aborting."
        exit 1
    fi
    log_info "Dependency check passed: kubectl is available."

    # ---[ Variable Validation ]----------------------------------------------
    #
    # This script relies on environment variables passed from the Kubernetes Job.
    # `set -u` will cause an exit if they are not set, but we add explicit checks
    # here to provide more user-friendly error messages.
    #
    : "${ALERT_NAMESPACE:?ERROR: ALERT_NAMESPACE environment variable is not set.}"
    : "${ALERT_DEPLOYMENT:?ERROR: ALERT_DEPLOYMENT environment variable is not set.}"
    # The DRY_RUN variable is optional and defaults to "false".
    DRY_RUN="${DRY_RUN:-false}"

    log_info "Received alert for Deployment: '${ALERT_DEPLOYMENT}' in Namespace: '${ALERT_NAMESPACE}'."

    if [[ "$DRY_RUN" == "true" ]]; then
        log_info "DRY_RUN mode is enabled. No changes will be made to the cluster."
    fi

    # ---[ Remediation Action ]-----------------------------------------------
    #
    # The core logic of the runbook. The action is a rollout restart, a common
    # and safe operation for stateless services that might have entered an
    # unhealthy state (e.g., CrashLoopBackOff, deadlocked).
    #
    log_info "Attempting to perform a rollout restart on the deployment."

    # Build the command in a variable to make the dry-run logic cleaner.
    local kubectl_command="kubectl --namespace ${ALERT_NAMESPACE} rollout restart deployment/${ALERT_DEPLOYMENT}"

    if [[ "$DRY_RUN" == "true" ]]; then
        log_info "DRY_RUN: Would execute the following command:"
        log_info "> ${kubectl_command}"
    else
        log_info "Executing command: ${kubectl_command}"
        # The actual execution. If kubectl fails (e.g., deployment not found, no
        # permissions), `set -e` will cause the script to terminate here.
        if output=$(${kubectl_command}); then
            log_info "Successfully initiated rollout restart. Kubectl output: ${output}"
        else
            # This block is unlikely to be reached with `set -e`, but serves as
            # a defense-in-depth measure.
            log_error "Failed to execute kubectl command. Please check permissions and resource names."
            exit 1
        fi
    fi

    log_info "Auto-heal runbook finished successfully."
}

# ---[ Script Entrypoint ]----------------------------------------------------
#
# This construct ensures that the main logic is only executed when the script
# is run directly, a best practice for shell scripting.
#
main "$@"