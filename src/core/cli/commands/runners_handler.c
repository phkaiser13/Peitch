/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: runners_handler.c
*
* This file implements the handler for the 'runners' command group. It serves
* as the C-language front-end for the 'runner_manager' Rust module, translating
* user-friendly command-line arguments into a structured JSON payload that the
* Rust backend can execute.
*
* The handler supports subcommands such as:
* - 'scale': Adjusts the scaling parameters of the runner deployment, such as
*   minimum/maximum replicas and the custom metric for autoscaling.
* - 'hpa install': A one-shot command to programmatically install the necessary
*   Kubernetes HorizontalPodAutoscaler (HPA) resources based on predefined
*   templates.
*
* The core responsibility of this file is argument parsing and data marshalling,
* offloading all Kubernetes API interactions and complex logic to the safer
* and more robust Rust implementation.
*
* SPDX-License-Identifier: Apache-2.0 */

#include "runners_handler.h"
#include "ui/tui.h"
#include "libs/liblogger/Logger.hpp"
#include <stdio.h>
#include <string.h>
#include <stdlib.h>

// --- Foreign Function Interface (FFI) Declaration ---

/**
 * @brief External function exported by the Rust 'runner_manager' module.
 *
 * This is the entry point into the Rust logic for managing runners.
 * It accepts a single string argument containing a JSON payload that defines
 * the entire operation to be performed (e.g., scale, install HPA).
 *
 * @param config_json A null-terminated UTF-8 string containing the JSON
 *                    configuration for the runner management operation.
 * @return An integer exit code. 0 indicates success; non-zero indicates failure.
 */
extern int run_runner_manager(const char* config_json);


// --- Private Helper Functions ---

/**
 * @brief Handles the 'scale' subcommand.
 *
 * Parses arguments for scaling runners (--min, --max, --autoscale-metric),
 * builds the corresponding JSON payload, and calls the Rust FFI function.
 *
 * @param argc The argument count, starting from the arguments after 'scale'.
 * @param argv The argument vector, starting from the arguments after 'scale'.
 * @return phStatus indicating the result of the operation.
 */
static phStatus handle_scale_subcommand(int argc, const char** argv) {
    int min_replicas = -1;
    int max_replicas = -1;
    const char* metric = "build_queue_length"; // Default as per spec
    const char* cluster = NULL;

    // 1. Parse command-line arguments
    for (int i = 0; i < argc; ++i) {
        if (strcmp(argv[i], "--min") == 0 && i + 1 < argc) {
            min_replicas = atoi(argv[++i]);
        } else if (strcmp(argv[i], "--max") == 0 && i + 1 < argc) {
            max_replicas = atoi(argv[++i]);
        } else if (strcmp(argv[i], "--autoscale-metric") == 0 && i + 1 < argc) {
            metric = argv[++i];
        } else if (strcmp(argv[i], "--cluster") == 0 && i + 1 < argc) {
            cluster = argv[++i];
        }
    }

    // 2. Validate that all required arguments were provided and are logical
    if (min_replicas < 0 || max_replicas < 0) {
        tui_print_error("Missing required arguments for 'scale'. Use --min and --max.");
        return ph_ERROR_INVALID_ARGS;
    }
    if (min_replicas > max_replicas) {
        tui_print_error("Invalid arguments: --min cannot be greater than --max.");
        return ph_ERROR_INVALID_ARGS;
    }

    if (!cluster) {
        cluster = config_manager_get_current_cluster();
        if (!cluster) {
            tui_print_error("No cluster specified and no default cluster is set. Use --cluster or 'ph kube use-cluster'.");
            return ph_ERROR_NOT_FOUND;
        }
    }

    // 3. Build the JSON payload
    char json_buffer[1024];
    const char* json_format =
        "{"
        "  \"action\": \"scale\","
        "  \"parameters\": {"
        "    \"min_replicas\": %d,"
        "    \"max_replicas\": %d,"
        "    \"metric\": \"%s\","
        "    \"cluster\": \"%s\""
        "  }"
        "}";

    int written = snprintf(json_buffer, sizeof(json_buffer), json_format,
                           min_replicas, max_replicas, metric, cluster);

    if (written < 0 || (size_t)written >= sizeof(json_buffer)) {
        logger_log(LOG_LEVEL_ERROR, "RunnersHandler", "Failed to build JSON payload for 'scale': buffer overflow.");
        tui_print_error("Internal error: could not construct request.");
        return ph_ERROR_BUFFER_TOO_SMALL;
    }

    logger_log_fmt(LOG_LEVEL_DEBUG, "RunnersHandler", "Calling Rust FFI with JSON payload: %s", json_buffer);

    // 4. Call the Rust FFI function and translate the result
    int rust_exit_code = run_runner_manager(json_buffer);

    if (rust_exit_code == 0) {
        logger_log(LOG_LEVEL_INFO, "RunnersHandler", "Rust module for 'runners scale' executed successfully.");
        tui_print_success("Runner scaling configuration applied successfully.");
        return ph_SUCCESS;
    } else {
        logger_log_fmt(LOG_LEVEL_ERROR, "RunnersHandler", "Rust module for 'runners scale' failed with exit code: %d.", rust_exit_code);
        tui_print_error("Failed to apply runner scaling configuration. Check logs for details.");
        return ph_ERROR_EXEC_FAILED;
    }
}

/**
 * @brief Handles the 'hpa install' subcommand.
 *
 * Builds a simple JSON payload to trigger the HPA installation logic in Rust.
 *
 * @param argc The argument count (expected to be 0).
 * @param argv The argument vector (expected to be empty).
 * @return phStatus indicating the result of the operation.
 */
static phStatus handle_hpa_install_subcommand(int argc, const char** argv) {
    const char* ns = "phgit-runner";
    const char* metric = "phgit_build_queue_length";
    const char* target = NULL;

    for (int i = 0; i < argc; ++i) {
        if (strcmp(argv[i], "--namespace") == 0 && i + 1 < argc) ns = argv[++i];
        else if (strcmp(argv[i], "--metric") == 0 && i + 1 < argc) metric = argv[++i];
        else if (strcmp(argv[i], "--target") == 0 && i + 1 < argc) target = argv[++i];
    }

    if (!target) {
        tui_print_error("--target is required for 'runners hpa install'.");
        return ph_ERROR_INVALID_ARGS;
    }

    char json_buffer[1024];
    const char* json_format =
        "{"
        "  \"action\": \"hpa_install\","
        "  \"parameters\": {"
        "    \"namespace\": \"%s\","
        "    \"metric\": \"%s\","
        "    \"target\": %s"
        "  }"
        "}";

    int written = snprintf(json_buffer, sizeof(json_buffer), json_format, ns, metric, target);

    if (written < 0 || (size_t)written >= sizeof(json_buffer)) {
        logger_log(LOG_LEVEL_ERROR, "RunnersHandler", "Failed to build JSON payload for 'hpa install': buffer overflow.");
        tui_print_error("Internal error: could not construct request.");
        return ph_ERROR_BUFFER_TOO_SMALL;
    }
    
    logger_log_fmt(LOG_LEVEL_DEBUG, "RunnersHandler", "Calling Rust FFI with JSON payload: %s", json_buffer);
    int rust_exit_code = run_runner_manager(json_buffer);

    if (rust_exit_code == 0) {
        logger_log(LOG_LEVEL_INFO, "RunnersHandler", "Rust module for 'hpa install' executed successfully.");
        tui_print_success("Runner HPA resources installed successfully.");
        return ph_SUCCESS;
    } else {
        logger_log_fmt(LOG_LEVEL_ERROR, "RunnersHandler", "Rust module for 'hpa install' failed with exit code: %d.", rust_exit_code);
        tui_print_error("Failed to install runner HPA resources. Check logs for details.");
        return ph_ERROR_EXEC_FAILED;
    }
}


// --- Public Function Implementation ---

/**
 * @see runners_handler.h
 */
phStatus handle_runners_command(int argc, const char** argv) {
    if (argc < 1 || argv[0] == NULL) {
        tui_print_error("No subcommand provided for 'runners'. Use 'scale' or 'hpa install'.");
        return ph_ERROR_INVALID_ARGS;
    }

    const char* subcommand = argv[0];
    logger_log_fmt(LOG_LEVEL_INFO, "RunnersHandler", "Dispatching 'runners' subcommand: '%s'", subcommand);

    if (strcmp(subcommand, "scale") == 0) {
        // Pass the remaining arguments to the scale handler
        return handle_scale_subcommand(argc - 1, &argv[1]);
    } else if (strcmp(subcommand, "hpa") == 0) {
        // Handle multi-word commands like 'hpa install'
        if (argc > 1 && strcmp(argv[1], "install") == 0) {
            // Pass arguments after 'hpa install'
            return handle_hpa_install_subcommand(argc - 2, &argv[2]);
        } else {
            char error_msg[128];
            snprintf(error_msg, sizeof(error_msg), "Unknown subcommand for 'runners hpa'. Did you mean 'install'?");
            tui_print_error(error_msg);
            return ph_ERROR_NOT_FOUND;
        }
    } else {
        char error_msg[128];
        snprintf(error_msg, sizeof(error_msg), "Unknown subcommand for 'runners': '%s'", subcommand);
        tui_print_error(error_msg);
        return ph_ERROR_NOT_FOUND;
    }
}