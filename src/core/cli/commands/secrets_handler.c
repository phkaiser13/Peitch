/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* File: secrets_handler.c
*
* This file implements the handler for the 'secrets' command group. Its main
* responsibility is to act as an intelligent bridge between the command-line
* interface and the powerful 'secret_manager' Rust module. It parses user-
* provided arguments for subcommands like 'sync' and 'rotate', constructs a
* precise JSON payload representing the user's request, and then passes this
* payload to the corresponding Rust module function via FFI for actual execution.
*
* The core logic involves:
* 1. Subcommand Dispatching: Identifying the action to perform (e.g., 'sync', 'rotate').
* 2. Argument Parsing: Iterating through command-line flags (e.g.,
*    `--provider`, `--k8s-secret`, `--path`) and extracting their values.
* 3. Data Marshalling: Carefully constructing a JSON string that matches the
*    data structure expected by the Rust FFI function. This includes dynamically
*    building a JSON array for multiple input paths in the 'sync' case.
* 4. FFI Call: Invoking the external Rust functions `run_secret_sync` or
*    `run_secret_rotation`.
* 5. Status Translation: Converting the integer return code from Rust into the
*    application's standard `phStatus` for consistent error handling.
*
* This approach leverages C for its simplicity in CLI integration and Rust for
* its safety and robustness in handling complex, security-sensitive tasks like
* connecting to secret providers (Vault, SOPS) and interacting with Kubernetes.
*
* SPDX-License-Identifier: Apache-2.0 */

#include "secrets_handler.h"
#include "ipc/include/ph_core_api.h"
#include "ui/tui.hh"
#include "libs/liblogger/Logger.hpp"
#include "config/config_manager.h" // The real configuration manager
#include <stdio.h>
#include <string.h>
#include <stdlib.h>

#define MAX_SECRET_PATHS 32
#define JSON_BUFFER_SIZE 4096

// --- FFI Declarations ---
// These functions are implemented in the Rust 'secret_manager' module.
extern int run_secret_sync(const char* config_json);
extern int run_secret_rotation(const char* config_json);

// --- Private Helper Functions ---

/**
 * @brief Handles the 'sync' subcommand.
 *
 * Parses arguments for syncing secrets, builds the JSON payload with a dynamic
 * array of secrets, and calls the Rust FFI function.
 */
static phStatus handle_sync_subcommand(int argc, const char** argv) {
    const char* provider = NULL;
    const char* k8s_secret_full = NULL;
    const char* paths[MAX_SECRET_PATHS];
    int path_count = 0;

    // 1. Parse command-line arguments, collecting all --path instances
    for (int i = 0; i < argc; ++i) {
        if (strcmp(argv[i], "--provider") == 0 && i + 1 < argc) {
            provider = argv[++i];
        } else if (strcmp(argv[i], "--k8s-secret") == 0 && i + 1 < argc) {
            k8s_secret_full = argv[++i];
        } else if (strcmp(argv[i], "--path") == 0 && i + 1 < argc) {
            if (path_count < MAX_SECRET_PATHS) {
                paths[path_count++] = argv[++i];
            } else {
                tui_print_error("Exceeded maximum number of --path arguments.");
                return ph_ERROR_BUFFER_TOO_SMALL;
            }
        }
    }

    // 2. Validate that all required arguments were provided
    if (!provider || !k8s_secret_full || path_count == 0) {
        tui_print_error("Missing required arguments for 'sync'. Use --provider, --k8s-secret, and at least one --path.");
        return ph_ERROR_INVALID_ARGS;
    }

    // 3. Fetch provider details from the actual config manager
    const char* provider_address = config_manager_get_provider_address(provider);
    const char* provider_token = config_manager_get_provider_token(provider);
    if (!provider_address) {
        char err_msg[256];
        snprintf(err_msg, sizeof(err_msg), "Configuration for provider '%s' not found or address is missing.", provider);
        tui_print_error(err_msg);
        return ph_ERROR_CONFIG_READ;
    }
    if (!provider_token) {
        char err_msg[256];
        snprintf(err_msg, sizeof(err_msg), "Token for provider '%s' not found. Ensure it is set in your configuration or environment.", provider);
        tui_print_error(err_msg);
        return ph_ERROR_CONFIG_READ;
    }

    // 4. Deconstruct k8s-secret argument
    char namespace[64] = {0}, secret_name[128] = {0};
    const char* slash_pos = strchr(k8s_secret_full, '/');
    if (!slash_pos) {
        tui_print_error("Invalid format for --k8s-secret. Expected 'namespace/secret_name'.");
        return ph_ERROR_INVALID_ARGS;
    }
    size_t namespace_len = slash_pos - k8s_secret_full;
    if (namespace_len > sizeof(namespace) - 1) {
        tui_print_error("Namespace part of --k8s-secret is too long.");
        return ph_ERROR_INVALID_ARGS;
    }
    strncpy(namespace, k8s_secret_full, namespace_len);
    namespace[namespace_len] = '\0'; // Ensure null termination
    strncpy(secret_name, slash_pos + 1, sizeof(secret_name) - 1);

    // 5. Build the JSON payload dynamically
    char json_buffer[JSON_BUFFER_SIZE];
    char* ptr = json_buffer;
    const char* end = json_buffer + JSON_BUFFER_SIZE;

    ptr += snprintf(ptr, end - ptr,
                    "{\"provider\":{\"provider\":\"%s\",\"address\":\"%s\",\"token\":\"%s\"},"
                    "\"namespace\":\"%s\",\"secret_name\":\"%s\",\"secrets\":[",
                    provider, provider_address, provider_token, namespace, secret_name);

    for (int i = 0; i < path_count; ++i) {
        char k8s_key[128] = {0}, value_from[256] = {0};
        const char* eq_pos = strchr(paths[i], '=');
        if (!eq_pos) {
            tui_print_error("Invalid format for --path. Expected 'K8S_KEY=PROVIDER_PATH'.");
            return ph_ERROR_INVALID_ARGS;
        }
        size_t key_len = eq_pos - paths[i];
        if (key_len > sizeof(k8s_key) - 1) {
            tui_print_error("K8S_KEY part of --path is too long.");
            return ph_ERROR_INVALID_ARGS;
        }
        strncpy(k8s_key, paths[i], key_len);
        k8s_key[key_len] = '\0'; // Ensure null termination
        strncpy(value_from, eq_pos + 1, sizeof(value_from) - 1);

        ptr += snprintf(ptr, end - ptr, "{\"name\":\"%s\",\"value_from\":\"%s\"}", k8s_key, value_from);
        if (i < path_count - 1) {
            ptr += snprintf(ptr, end - ptr, ",");
        }
    }

    ptr += snprintf(ptr, end - ptr, "]}");

    if (ptr >= end) {
        logger_log(LOG_LEVEL_ERROR, "SecretsHandler", "Failed to build JSON payload: buffer overflow.");
        tui_print_error("Internal error: request is too large.");
        return ph_ERROR_BUFFER_TOO_SMALL;
    }

    // 6. Call the Rust FFI function and translate the result
    logger_log_fmt(LOG_LEVEL_DEBUG, "SecretsHandler", "Calling Rust FFI with JSON payload: %s", json_buffer);
    int rust_exit_code = run_secret_sync(json_buffer);

    if (rust_exit_code == 0) {
        tui_print_success("Secrets synchronized successfully.");
        return ph_SUCCESS;
    } else {
        tui_print_error("Failed to synchronize secrets. Check logs for details.");
        return ph_ERROR_EXEC_FAILED;
    }
}

/**
 * @brief Handles the 'rotate' subcommand.
 *
 * Parses arguments for rotating a secret, builds the JSON payload, and calls
 * the corresponding Rust FFI function.
 */
static phStatus handle_rotate_subcommand(int argc, const char** argv) {
    const char* provider = NULL;
    const char* secret_path = NULL;
    bool force = false;

    // 1. Parse command-line arguments
    for (int i = 0; i < argc; ++i) {
        if (strcmp(argv[i], "--provider") == 0 && i + 1 < argc) {
            provider = argv[++i];
        } else if (strcmp(argv[i], "--secret-path") == 0 && i + 1 < argc) {
            secret_path = argv[++i];
        } else if (strcmp(argv[i], "--force") == 0) {
            force = true;
        }
    }

    // 2. Validate that all required arguments were provided
    if (!provider || !secret_path) {
        tui_print_error("Missing required arguments for 'rotate'. Use --provider and --secret-path.");
        return ph_ERROR_INVALID_ARGS;
    }

    // 3. Fetch provider details from the configuration manager
    const char* provider_address = config_manager_get_provider_address(provider);
    const char* provider_token = config_manager_get_provider_token(provider);
    if (!provider_address || !provider_token) {
        char err_msg[256];
        snprintf(err_msg, sizeof(err_msg), "Configuration for provider '%s' not found or is incomplete.", provider);
        tui_print_error(err_msg);
        return ph_ERROR_CONFIG_READ;
    }

    // 4. Build the JSON payload for the rotation request
    char json_buffer[1024]; // A smaller buffer is sufficient for this simpler payload
    int written = snprintf(json_buffer, sizeof(json_buffer),
             "{\"provider\":{\"provider\":\"%s\",\"address\":\"%s\",\"token\":\"%s\"},\"path\":\"%s\",\"force\":%s}",
             provider, provider_address, provider_token, secret_path, force ? "true" : "false");

    if (written < 0 || (size_t)written >= sizeof(json_buffer)) {
        logger_log(LOG_LEVEL_ERROR, "SecretsHandler", "Failed to build JSON payload for rotate: buffer overflow or encoding error.");
        tui_print_error("Internal error: request is too large.");
        return ph_ERROR_BUFFER_TOO_SMALL;
    }

    logger_log_fmt(LOG_LEVEL_DEBUG, "SecretsHandler", "Calling Rust FFI 'run_secret_rotation' with JSON payload: %s", json_buffer);

    // 5. Call the Rust FFI function and translate the result
    int rust_exit_code = run_secret_rotation(json_buffer);

    if (rust_exit_code == 0) {
        tui_print_success("Secret rotated successfully.");
        return ph_SUCCESS;
    } else {
        tui_print_error("Failed to rotate secret. Check logs for details.");
        return ph_ERROR_EXEC_FAILED;
    }
}


// --- Public Function Implementation ---

phStatus handle_secrets_command(int argc, const char** argv) {
    if (argc < 1 || argv[0] == NULL) {
        tui_print_error("No subcommand provided for 'secrets'. Use 'sync' or 'rotate'.");
        return ph_ERROR_INVALID_ARGS;
    }

    const char* subcommand = argv[0];
    logger_log_fmt(LOG_LEVEL_INFO, "SecretsHandler", "Dispatching 'secrets' subcommand: '%s'", subcommand);

    if (strcmp(subcommand, "sync") == 0) {
        // Pass the remaining arguments (argc-1, starting from argv[1]) to the handler
        return handle_sync_subcommand(argc - 1, &argv[1]);
    } else if (strcmp(subcommand, "rotate") == 0) {
        // Replace the placeholder with the call to the new handler
        return handle_rotate_subcommand(argc - 1, &argv[1]);
    } else {
        char error_msg[128];
        snprintf(error_msg, sizeof(error_msg), "Unknown subcommand for 'secrets': '%s'", subcommand);
        tui_print_error(error_msg);
        return ph_ERROR_NOT_FOUND;
    }
}