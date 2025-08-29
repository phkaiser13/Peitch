/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* Archive: src/core/cli/commands/health_handler.c
*
* This file implements the handler for the 'health' and 'autoheal' command
* groups. It employs a dual-strategy approach based on the subcommand:
*
* - For 'health check', it acts as a standard FFI bridge. It parses CLI
*   arguments, constructs a JSON payload, and calls the `run_health_manager`
*   function in the `k8s_health` Rust module. The Rust backend then performs
*   all the complex Kubernetes API interactions for the health assessment.
*
* - For 'autoheal enable', it acts as a configuration generator. It parses
*   the auto-heal rule parameters, dynamically generates a manifest for the
*   `phAutoHealRule` Custom Resource (CR) as a YAML string, and then pipes
*   this manifest to `kubectl apply` running as a subprocess. This configures
*   the `ph-operator` in the cluster to enforce the desired auto-healing rule
*   without requiring the C code to have a built-in Kubernetes client.
*
* This hybrid design leverages the strengths of both approaches: Rust for safe,
* complex API logic, and C with standard system tools for simple, robust
* configuration tasks.
*
* SPDX-License-Identifier: Apache-2.0 */

#include "health_handler.h"
#include "ui/tui.h"
#include "libs/liblogger/Logger.hpp"
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <stdbool.h>
#include <sys/wait.h> // Required for WEXITSTATUS

// --- Foreign Function Interface (FFI) Declaration ---

/**
 * @brief External function exported by the Rust 'k8s_health' module.
 *
 * This is the entry point into the Rust logic for all health and auto-heal
 * operations. It accepts a JSON payload defining the action and its parameters.
 *
 * @param config_json A null-terminated UTF-8 string containing the JSON
 *                    configuration for the health management operation.
 * @return An integer exit code. 0 indicates success; non-zero indicates failure.
 */
extern int run_health_manager(const char* config_json);


// --- Private Helper Functions ---

/**
 * @brief Executes `kubectl apply -f -` and pipes the provided YAML to its stdin.
 * @param yaml_manifest The null-terminated string containing the YAML to apply.
 * @return phStatus indicating the outcome of the kubectl command.
 */
static phStatus apply_yaml_via_kubectl(const char* yaml_manifest) {
    logger_log(LOG_LEVEL_INFO, "HealthHandler", "Attempting to apply generated YAML via kubectl.");
    // "w" opens the command's stdin for writing.
    FILE* pipe = popen("kubectl apply -f -", "w");
    if (!pipe) {
        logger_log(LOG_LEVEL_ERROR, "HealthHandler", "Failed to open pipe to kubectl. Is kubectl in your PATH?");
        tui_print_error("Failed to execute kubectl. Please ensure it is installed and in your PATH.");
        return ph_ERROR_EXEC_FAILED;
    }

    // Write the YAML manifest to the command's stdin.
    if (fprintf(pipe, "%s", yaml_manifest) < 0) {
        pclose(pipe);
        logger_log(LOG_LEVEL_ERROR, "HealthHandler", "Failed to write YAML to kubectl pipe.");
        tui_print_error("An I/O error occurred while communicating with kubectl.");
        return ph_ERROR_IO;
    }

    // pclose waits for the command to terminate and returns its exit status.
    int status = pclose(pipe);
    if (WIFEXITED(status) && WEXITSTATUS(status) == 0) {
        logger_log(LOG_LEVEL_INFO, "HealthHandler", "kubectl apply completed successfully.");
        return ph_SUCCESS;
    } else {
        logger_log_fmt(LOG_LEVEL_ERROR, "HealthHandler", "kubectl apply failed with exit status: %d", WEXITSTATUS(status));
        tui_print_error("kubectl apply command failed. Please check kubectl logs or permissions.");
        return ph_ERROR_EXEC_FAILED;
    }
}

/**
 * @brief Handles the 'health check' subcommand.
 *
 * This function parses arguments for a health check, constructs a JSON payload,
 * and calls the Rust FFI function to perform the actual check.
 *
 * @param argc The number of arguments.
 * @param argv The argument vector.
 * @return phStatus indicating the outcome.
 */
static phStatus handle_check_subcommand(int argc, const char** argv) {
    const char* app = NULL;
    const char* cluster = NULL;
    bool full_check = false;

    for (int i = 0; i < argc; ++i) {
        if (strcmp(argv[i], "--app") == 0 && i + 1 < argc) {
            app = argv[++i];
        } else if (strcmp(argv[i], "--cluster") == 0 && i + 1 < argc) {
            cluster = argv[++i];
        } else if (strcmp(argv[i], "--full") == 0) {
            full_check = true;
        }
    }

    if (!app || !cluster) {
        tui_print_error("Missing required arguments for 'health check'. Use --app and --cluster.");
        return ph_ERROR_INVALID_ARGS;
    }

    char json_buffer[1024];
    snprintf(json_buffer, sizeof(json_buffer),
             "{\"action\":\"check\",\"parameters\":{\"app\":\"%s\",\"cluster\":\"%s\",\"full_check\":%s}}",
             app, cluster, full_check ? "true" : "false");

    logger_log_fmt(LOG_LEVEL_DEBUG, "HealthHandler", "Calling Rust FFI with JSON payload: %s", json_buffer);
    int rust_exit_code = run_health_manager(json_buffer);

    // The Rust module prints detailed status. We just reflect the final outcome.
    if (rust_exit_code == 0) {
        return ph_SUCCESS;
    } else {
        return ph_ERROR_EXEC_FAILED;
    }
}

/**
 * @brief Handles the 'autoheal enable' subcommand.
 *
 * This function parses the arguments for an auto-heal rule, generates the
 * YAML manifest for a `phAutoHealRule` Custom Resource, and applies it to the
 * Kubernetes cluster using `kubectl`.
 *
 * @param argc The number of arguments.
 * @param argv The argument vector.
 * @return phStatus indicating the outcome.
 */
static phStatus handle_autoheal_enable_subcommand(int argc, const char** argv) {
    const char* on_trigger = NULL;
    const char* actions = NULL;
    const char* cooldown = NULL;

    // Parse command-line arguments for the auto-heal rule.
    for (int i = 0; i < argc; ++i) {
        if (strcmp(argv[i], "--on") == 0 && i + 1 < argc) {
            on_trigger = argv[++i];
        } else if (strcmp(argv[i], "--actions") == 0 && i + 1 < argc) {
            actions = argv[++i];
        } else if (strcmp(argv[i], "--cooldown") == 0 && i + 1 < argc) {
            cooldown = argv[++i];
        }
    }

    if (!on_trigger || !actions || !cooldown) {
        tui_print_error("Missing required arguments for 'autoheal enable'. Use --on, --actions, and --cooldown.");
        return ph_ERROR_INVALID_ARGS;
    }

    char yaml_buffer[2048];
    // NOTE: The trigger name should be a valid Kubernetes resource name (DNS-1123).
    // The user is responsible for providing a sanitized name.
    const char* yaml_format =
        "apiVersion: ph.kaiser.io/v1alpha1\n"
        "kind: phAutoHealRule\n"
        "metadata:\n"
        "  # The resource name is derived from the trigger for uniqueness.\n"
        "  name: autoheal-rule-%s\n"
        "  # Assumes the ph-operator is configured to watch this namespace.\n"
        "  namespace: ph-operator\n"
        "spec:\n"
        "  # The name of the alert/trigger that activates this rule.\n"
        "  triggerName: \"%s\"\n"
        "  # The cooldown period to prevent the rule from firing too frequently.\n"
        "  cooldown: \"%s\"\n"
        "  # The list of actions to execute when the rule is triggered.\n"
        "  actions:\n"
        "    - runbook:\n"
        "        scriptName: \"%s\"\n";

    // Generate the YAML manifest string from the parsed arguments.
    int written = snprintf(yaml_buffer, sizeof(yaml_buffer), yaml_format,
                           on_trigger, // Used for metadata.name
                           on_trigger, // Used for spec.triggerName
                           cooldown,   // Used for spec.cooldown
                           actions);   // Used for spec.actions.runbook.scriptName

    if (written < 0 || (size_t)written >= sizeof(yaml_buffer)) {
        logger_log(LOG_LEVEL_ERROR, "HealthHandler", "Buffer too small to generate phAutoHealRule YAML.");
        tui_print_error("Internal error: could not generate auto-heal configuration.");
        return ph_ERROR_BUFFER_TOO_SMALL;
    }

    logger_log_fmt(LOG_LEVEL_DEBUG, "HealthHandler", "Generated phAutoHealRule YAML:\n%s", yaml_buffer);

    // Apply the generated manifest to the cluster.
    phStatus status = apply_yaml_via_kubectl(yaml_buffer);
    if (status == ph_SUCCESS) {
        tui_print_success("Auto-heal rule configured successfully in the cluster.");
    } else {
        tui_print_error("Failed to configure auto-heal rule.");
    }
    return status;
}

// --- Public Function Implementation ---

phStatus handle_health_command(int argc, const char** argv) {
    if (argc < 1 || argv[0] == NULL) {
        tui_print_error("No subcommand provided for 'health' or 'autoheal'.");
        return ph_ERROR_INVALID_ARGS;
    }

    const char* subcommand = argv[0];
    logger_log_fmt(LOG_LEVEL_INFO, "HealthHandler", "Dispatching subcommand: '%s'", subcommand);

    // Route to the appropriate handler based on the subcommand.
    if (strcmp(subcommand, "check") == 0) {
        // Handles 'ph health check ...'
        return handle_check_subcommand(argc - 1, &argv[1]);
    } else if (strcmp(subcommand, "enable") == 0) {
        // Handles 'ph autoheal enable ...'
        return handle_autoheal_enable_subcommand(argc - 1, &argv[1]);
    } else {
        char error_msg[128];
        snprintf(error_msg, sizeof(error_msg), "Unknown subcommand: '%s'. Use 'check' or 'enable'.", subcommand);
        tui_print_error(error_msg);
        return ph_ERROR_NOT_FOUND;
    }
}