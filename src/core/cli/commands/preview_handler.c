/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 *
 * This file implements the routing logic for all "phgit preview" subcommands.
 * It translates command-line arguments into structured JSON payloads for the
 * k8s_preview Rust module, which manages the lifecycle of ephemeral preview
 * environments.
 *
 * SPDX-License-Identifier: Apache-2.0
 */

#include "preview_handler.h"
#include "ipc/include/ph_core_api.h"
#include "ui/tui.h"
#include "libs/liblogger/Logger.hpp"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdbool.h>

// FFI Declaration for the k8s_preview Rust module
extern int run_preview_manager(const char* json_payload);

// FFI Declarations for the tracing_layer module
extern char* start_trace_for_command(const char* command_name);
extern void free_rust_string(char* s);


// Forward declarations for static helpers
static void print_preview_usage(void);
static phStatus handle_preview_create_command(int argc, const char** argv);
// Stubs for other commands to be implemented
static phStatus handle_preview_status_command(int argc, const char** argv);
static phStatus handle_preview_teardown_command(int argc, const char** argv);
static phStatus handle_preview_logs_command(int argc, const char** argv);
static phStatus handle_preview_exec_command(int argc, const char** argv);
static phStatus handle_preview_extend_command(int argc, const char** argv);
static phStatus handle_preview_gc_command(int argc, const char** argv);


phStatus handle_preview_command(int argc, const char** argv) {
    if (argc < 1) {
        tui_print_error("No preview subcommand provided.");
        print_preview_usage();
        return ph_ERROR_INVALID_ARGS;
    }

    const char* subcommand = argv[0];
    logger_log_fmt(LOG_LEVEL_INFO, "PreviewHandler", "Dispatching 'preview' subcommand: '%s'", subcommand);

    if (strcmp(subcommand, "create") == 0) {
        return handle_preview_create_command(argc - 1, &argv[1]);
    } else if (strcmp(subcommand, "status") == 0) {
        return handle_preview_status_command(argc - 1, &argv[1]);
    } else if (strcmp(subcommand, "teardown") == 0) {
        return handle_preview_teardown_command(argc - 1, &argv[1]);
    } else if (strcmp(subcommand, "logs") == 0) {
        return handle_preview_logs_command(argc - 1, &argv[1]);
    } else if (strcmp(subcommand, "exec") == 0) {
        return handle_preview_exec_command(argc - 1, &argv[1]);
    } else if (strcmp(subcommand, "extend") == 0) {
        return handle_preview_extend_command(argc - 1, &argv[1]);
    } else if (strcmp(subcommand, "gc") == 0) {
        return handle_preview_gc_command(argc - 1, &argv[1]);
    } else {
        char error_msg[128];
        snprintf(error_msg, sizeof(error_msg), "Unknown preview subcommand '%s'.", subcommand);
        tui_print_error(error_msg);
        print_preview_usage();
        return ph_ERROR_NOT_FOUND;
    }
}

static void print_preview_usage(void) {
    tui_print_info("\nUsage: ph preview <subcommand> [options]\n\n"
                   "Preview Environment Lifecycle:\n"
                   "  create          Create a new ephemeral preview environment for a pull request.\n"
                   "  status          Get the status of an existing preview environment.\n"
                   "  teardown        Destroy a preview environment.\n"
                   "  logs            Get logs from a component in the preview.\n"
                   "  exec            Execute a command in a preview container.\n"
                   "  extend          Extend the TTL of a preview environment.\n"
                   "  gc              Garbage collect expired environments.\n\n"
                   "Run 'ph preview <subcommand> --help' for more information.");
}

#include <errno.h> // For strtol

// Helper to find a value in a simple JSON string.
// This is not a robust JSON parser, but sufficient for this specific use case.
static char* find_json_value(char* json, const char* key) {
    char* key_ptr = strstr(json, key);
    if (!key_ptr) return NULL;

    char* value_start = strchr(key_ptr, ':');
    if (!value_start) return NULL;

    value_start++; // Move past ':'
    while (*value_start == ' ' || *value_start == '\"') {
        value_start++;
    }

    char* value_end = strchr(value_start, '\"');
    if (!value_end) return NULL;

    *value_end = '\0'; // Null-terminate the value.
    return value_start;
}

static phStatus handle_preview_create_command(int argc, const char** argv) {
    const char* pr_str = NULL;
    const char* repo_url = NULL;
    const char* image = NULL;
    const char* commit_sha = NULL;
    const char* ttl_str = NULL;

    for (int i = 0; i < argc; ++i) {
        if (strcmp(argv[i], "--pr") == 0 && i + 1 < argc) pr_str = argv[++i];
        else if (strcmp(argv[i], "--repo") == 0 && i + 1 < argc) repo_url = argv[++i];
        else if (strcmp(argv[i], "--image") == 0 && i + 1 < argc) image = argv[++i];
        else if (strcmp(argv[i], "--ttl") == 0 && i + 1 < argc) ttl_str = argv[++i];
        else if (strcmp(argv[i], "--commit-sha") == 0 && i + 1 < argc) commit_sha = argv[++i];
    }

    if (!pr_str || !repo_url) {
        tui_print_error("--pr and --repo are required for 'preview create'.");
        return ph_ERROR_INVALID_ARGS;
    }

    char* endptr;
    errno = 0;
    long pr_number = strtol(pr_str, &endptr, 10);
    if (errno != 0 || *endptr != '\0' || pr_number <= 0) {
        tui_print_error("Invalid --pr value. Must be a positive integer.");
        return ph_ERROR_INVALID_ARGS;
    }

    // --- OpenTelemetry: Start Trace ---
    char* trace_context_json = start_trace_for_command("preview_create");
    char* traceparent = NULL;
    if (trace_context_json) {
        // The returned string is mutable, so we can modify it in place.
        traceparent = find_json_value(trace_context_json, "traceparent");
    }
    
    char json_payload[4096];
    char* ptr = json_payload;
    const char* end = json_payload + sizeof(json_payload);
    
    ptr += snprintf(ptr, end - ptr, "{\"action\":\"create\",\"pr_number\":%ld,\"git_repo_url\":\"%s\"", pr_number, repo_url);

    if (commit_sha) {
        ptr += snprintf(ptr, end - ptr, ",\"commit_sha\":\"%s\"", commit_sha);
    }
    
    if (ttl_str) {
        long ttl_hours = strtol(ttl_str, NULL, 10);
        ptr += snprintf(ptr, end - ptr, ",\"new_ttl\":%ld", ttl_hours);
    }

    // --- OpenTelemetry: Inject Context ---
    if (traceparent) {
        ptr += snprintf(ptr, end - ptr, ",\"annotations\":{\"ph.io/trace-context\":\"%s\"}", traceparent);
    }

    snprintf(ptr, end - ptr, "}");

    logger_log_fmt(LOG_LEVEL_DEBUG, "PreviewHandler", "Calling 'run_preview_manager' with payload: %s", json_payload);
    int result = run_preview_manager(json_payload);

    // --- OpenTelemetry: Cleanup ---
    if (trace_context_json) {
        free_rust_string(trace_context_json);
    }

    if (result == 0) {
        tui_print_success("Preview environment creation process initiated.");
        return ph_SUCCESS;
    } else {
        tui_print_error("Failed to initiate preview environment creation.");
        return ph_ERROR_EXEC_FAILED;
    }
}

static phStatus handle_preview_logs_command(int argc, const char** argv) {
    const char* pr_str = NULL;
    const char* component_name = NULL;

    for (int i = 0; i < argc; ++i) {
        if (strcmp(argv[i], "--pr") == 0 && i + 1 < argc) pr_str = argv[++i];
        else if (strcmp(argv[i], "--component") == 0 && i + 1 < argc) component_name = argv[++i];
    }

    if (!pr_str || !component_name) {
        tui_print_error("--pr and --component are required for 'preview logs'.");
        return ph_ERROR_INVALID_ARGS;
    }

    long pr_number = strtol(pr_str, NULL, 10);
    if (pr_number <= 0) {
        tui_print_error("Invalid --pr value. Must be a positive integer.");
        return ph_ERROR_INVALID_ARGS;
    }

    char json_payload[1024];
    snprintf(json_payload, sizeof(json_payload),
             "{\"action\":\"logs\",\"pr_number\":%ld,\"component_name\":\"%s\"}",
             pr_number, component_name);

    logger_log_fmt(LOG_LEVEL_DEBUG, "PreviewHandler", "Calling 'run_preview_manager' for logs with payload: %s", json_payload);
    int result = run_preview_manager(json_payload);

    return result == 0 ? ph_SUCCESS : ph_ERROR_EXEC_FAILED;
}

static phStatus handle_preview_exec_command(int argc, const char** argv) {
    const char* pr_str = NULL;
    const char* component_name = NULL;
    int command_start_index = -1;

    for (int i = 0; i < argc; ++i) {
        if (strcmp(argv[i], "--pr") == 0 && i + 1 < argc) {
            pr_str = argv[++i];
        } else if (strcmp(argv[i], "--component") == 0 && i + 1 < argc) {
            component_name = argv[++i];
        } else if (strcmp(argv[i], "--") == 0) {
            command_start_index = i + 1;
            break;
        }
    }

    if (!pr_str || !component_name || command_start_index == -1 || command_start_index >= argc) {
        tui_print_error("Usage: ph preview exec --pr <pr> --component <comp> -- <command> [args...]");
        return ph_ERROR_INVALID_ARGS;
    }
    
    long pr_number = strtol(pr_str, NULL, 10);
    if (pr_number <= 0) {
        tui_print_error("Invalid --pr value. Must be a positive integer.");
        return ph_ERROR_INVALID_ARGS;
    }

    char json_payload[4096];
    char* ptr = json_payload;
    const char* end = json_payload + sizeof(json_payload);

    ptr += snprintf(ptr, end - ptr, "{\"action\":\"exec\",\"pr_number\":%ld,\"component_name\":\"%s\",\"command_to_exec\":[", pr_number, component_name);

    for (int i = command_start_index; i < argc; ++i) {
        ptr += snprintf(ptr, end - ptr, "\"%s\"%s", argv[i], (i == argc - 1) ? "" : ",");
    }

    ptr += snprintf(ptr, end - ptr, "]}");

    logger_log_fmt(LOG_LEVEL_DEBUG, "PreviewHandler", "Calling 'run_preview_manager' for exec with payload: %s", json_payload);
    int result = run_preview_manager(json_payload);
    
    return result == 0 ? ph_SUCCESS : ph_ERROR_EXEC_FAILED;
}

static phStatus handle_preview_extend_command(int argc, const char** argv) {
    const char* pr_str = NULL;
    const char* ttl_str = NULL;

    for (int i = 0; i < argc; ++i) {
        if (strcmp(argv[i], "--pr") == 0 && i + 1 < argc) pr_str = argv[++i];
        else if (strcmp(argv[i], "--ttl") == 0 && i + 1 < argc) ttl_str = argv[++i];
    }

    if (!pr_str || !ttl_str) {
        tui_print_error("--pr and --ttl are required for 'preview extend'.");
        return ph_ERROR_INVALID_ARGS;
    }

    long pr_number = strtol(pr_str, NULL, 10);
    long ttl_hours = strtol(ttl_str, NULL, 10);
    if (pr_number <= 0 || ttl_hours <= 0) {
        tui_print_error("Invalid --pr or --ttl value. Must be positive integers.");
        return ph_ERROR_INVALID_ARGS;
    }

    char json_payload[1024];
    snprintf(json_payload, sizeof(json_payload),
             "{\"action\":\"extend\",\"pr_number\":%ld,\"new_ttl\":%ld}",
             pr_number, ttl_hours);

    logger_log_fmt(LOG_LEVEL_DEBUG, "PreviewHandler", "Calling 'run_preview_manager' for extend with payload: %s", json_payload);
    int result = run_preview_manager(json_payload);

    return result == 0 ? ph_SUCCESS : ph_ERROR_EXEC_FAILED;
}

static phStatus handle_preview_gc_command(int argc, const char** argv) {
    const char* max_age_str = NULL;

    for (int i = 0; i < argc; ++i) {
        if (strcmp(argv[i], "--max-age-hours") == 0 && i + 1 < argc) max_age_str = argv[++i];
    }

    if (!max_age_str) {
        tui_print_error("--max-age-hours is required for 'preview gc'.");
        return ph_ERROR_INVALID_ARGS;
    }

    long max_age_hours = strtol(max_age_str, NULL, 10);
    if (max_age_hours < 0) { // 0 could be a valid value to mean "collect all"
        tui_print_error("Invalid --max-age-hours value. Must be a non-negative integer.");
        return ph_ERROR_INVALID_ARGS;
    }

    char json_payload[1024];
    snprintf(json_payload, sizeof(json_payload),
             "{\"action\":\"gc\",\"max_age_hours\":%ld}",
             max_age_hours);

    logger_log_fmt(LOG_LEVEL_DEBUG, "PreviewHandler", "Calling 'run_preview_manager' for gc with payload: %s", json_payload);
    int result = run_preview_manager(json_payload);

    return result == 0 ? ph_SUCCESS : ph_ERROR_EXEC_FAILED;
}

// Stubs for the other functions
static phStatus handle_preview_status_command(int argc, const char** argv) {
    const char* pr_str = NULL;

    for (int i = 0; i < argc; ++i) {
        if (strcmp(argv[i], "--pr") == 0 && i + 1 < argc) pr_str = argv[++i];
    }

    if (!pr_str) {
        tui_print_error("--pr is required for 'preview status'.");
        return ph_ERROR_INVALID_ARGS;
    }
    
    char* endptr;
    errno = 0;
    long pr_number = strtol(pr_str, &endptr, 10);
    if (errno != 0 || *endptr != '\0' || pr_number <= 0) {
        tui_print_error("Invalid --pr value. Must be a positive integer.");
        return ph_ERROR_INVALID_ARGS;
    }

    char json_payload[1024];
    snprintf(json_payload, sizeof(json_payload),
             "{\"action\":\"status\",\"pr_number\":%ld}",
             pr_number);

    logger_log_fmt(LOG_LEVEL_DEBUG, "PreviewHandler", "Calling 'run_preview_manager' with payload: %s", json_payload);
    int result = run_preview_manager(json_payload);

    if (result != 0) {
        tui_print_error("Failed to get preview environment status.");
        return ph_ERROR_EXEC_FAILED;
    }

    return ph_SUCCESS;
}

static phStatus handle_preview_teardown_command(int argc, const char** argv) {
    const char* pr_str = NULL;
    // The 'force' flag is not in the Rust config struct, so we ignore it for now.
    // bool force = false;

    for (int i = 0; i < argc; ++i) {
        if (strcmp(argv[i], "--pr") == 0 && i + 1 < argc) pr_str = argv[++i];
        // else if (strcmp(argv[i], "--force") == 0) force = true;
    }

    if (!pr_str) {
        tui_print_error("--pr is required for 'preview teardown'.");
        return ph_ERROR_INVALID_ARGS;
    }
    
    char* endptr;
    errno = 0;
    long pr_number = strtol(pr_str, &endptr, 10);
    if (errno != 0 || *endptr != '\0' || pr_number <= 0) {
        tui_print_error("Invalid --pr value. Must be a positive integer.");
        return ph_ERROR_INVALID_ARGS;
    }

    char json_payload[1024];
    snprintf(json_payload, sizeof(json_payload),
             "{\"action\":\"destroy\",\"pr_number\":%ld}",
             pr_number);

    logger_log_fmt(LOG_LEVEL_DEBUG, "PreviewHandler", "Calling 'run_preview_manager' with payload: %s", json_payload);
    int result = run_preview_manager(json_payload);

    if (result == 0) {
        tui_print_success("Preview environment teardown process initiated.");
        return ph_SUCCESS;
    } else {
        tui_print_error("Failed to initiate preview environment teardown.");
        return ph_ERROR_EXEC_FAILED;
    }
}
