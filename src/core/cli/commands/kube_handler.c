/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * File: src/core/cli/commands/kube_handler.c
 *
 * This file implements the routing logic for all "ph kube" subcommands. It acts
 * as a translation layer between the command-line arguments (char**) provided
 * by the user and the structured JSON payloads expected by the high-performance
 * Rust modules. The main function, `handle_kube_command`, identifies the
 * subcommand (e.g., "sync", "rollout", "multi") and dispatches to a dedicated
 * static helper function.
 *
 * Each helper function is responsible for parsing its specific arguments and
 * flags, meticulously constructing a JSON string, and then invoking the
 * appropriate Rust FFI function. This design keeps the C code focused on its
 * core responsibility—argument parsing and FFI invocation—while delegating all
 * complex business logic (like Kubernetes API interaction, Git operations, or
 * release orchestration) to the safer, more expressive Rust ecosystem.
 *
 * This version adds support for the `rollout promote` and `rollout rollback`
 * subcommands, enabling manual control over the progressive delivery lifecycle.
 *
 * SPDX-License-Identifier: Apache-2.0 */

#include "kube_handler.h"
#include "ipc/include/ph_core_api.h"
#include "ui/tui.h"
#include "libs/liblogger/Logger.hpp"
#include "config/config_manager.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdbool.h>

// --- FFI Declarations for Rust Modules ---
extern int run_sync(const char* json_payload, char* error_buf, size_t error_buf_len);
extern int run_drift_detector(const char* json_payload, char* error_buf, size_t error_buf_len); // For kube drift
extern int run_release_orchestrator(const char* json_payload, unsigned char* error_buf, size_t error_buf_len);
extern int run_multi_cluster_orchestrator(const char* json_payload, char* error_buf, size_t error_buf_len);
extern int run_rbac_manager(const char* json_payload);
/* BEGIN CHANGE: Add FFI declaration for k8s-info module and improve comment. */
// FFI declaration for the k8s-info Rust module.
extern int run_k8s_info(const char* json_payload);
/* END CHANGE */

// --- Forward Declarations for Static Helper Functions ---
static void print_kube_usage(void);
static phStatus handle_sync_command(int argc, const char** argv);
static phStatus handle_drift_command(int argc, const char** argv);
static phStatus handle_rollout_command(int argc, const char** argv);
static phStatus handle_multi_command(int argc, const char** argv);
static phStatus handle_list_clusters_command(int argc, const char** argv);
static phStatus handle_use_cluster_command(int argc, const char** argv);
static phStatus handle_info_command(int argc, const char** argv);
static phStatus handle_cluster_command(int argc, const char** argv);
static phStatus handle_grant_command(int argc, const char** argv);
static phStatus handle_revoke_command(int argc, const char** argv);
static phStatus handle_failover_command(int argc, const char** argv);

// --- Main Command Router ---

phStatus handle_kube_command(int argc, const char** argv) {
    if (argc < 1) {
        tui_print_error("No kube subcommand provided.");
        print_kube_usage();
        return ph_ERROR_INVALID_ARGS;
    }

    const char* subcommand = argv[0];
    logger_log_fmt(LOG_LEVEL_INFO, "KubeHandler", "Dispatching 'kube' subcommand: '%s'", subcommand);

    if (strcmp(subcommand, "sync") == 0) {
        return handle_sync_command(argc - 1, &argv[1]);
    } else if (strcmp(subcommand, "drift") == 0) {
        return handle_drift_command(argc - 1, &argv[1]);
    } else if (strcmp(subcommand, "rollout") == 0) {
        return handle_rollout_command(argc - 1, &argv[1]);
    } else if (strcmp(subcommand, "multi") == 0) {
        return handle_multi_command(argc - 1, &argv[1]);
    } else if (strcmp(subcommand, "list-clusters") == 0) {
        return handle_list_clusters_command(argc - 1, &argv[1]);
    } else if (strcmp(subcommand, "use-cluster") == 0) {
        return handle_use_cluster_command(argc - 1, &argv[1]);
    } else if (strcmp(subcommand, "info") == 0) {
        return handle_info_command(argc - 1, &argv[1]);
    } else if (strcmp(subcommand, "cluster") == 0) {
        return handle_cluster_command(argc - 1, &argv[1]);
    } else if (strcmp(subcommand, "grant") == 0) {
        return handle_grant_command(argc - 1, &argv[1]);
    } else if (strcmp(subcommand, "revoke") == 0) {
        return handle_revoke_command(argc - 1, &argv[1]);
    } else if (strcmp(subcommand, "failover") == 0) {
        return handle_failover_command(argc - 1, &argv[1]);
    } else {
        char error_msg[128];
        snprintf(error_msg, sizeof(error_msg), "Unknown kube subcommand '%s'.", subcommand);
        tui_print_error(error_msg);
        print_kube_usage();
        return ph_ERROR_NOT_FOUND;
    }
}

// --- Static Helper Implementations ---

static void print_kube_usage(void) {
    tui_print_info("\nUsage: ph kube <subcommand> [options]\n\n"
                   "Cluster Management:\n"
                   "  list-clusters   List all clusters defined in the configuration.\n"
                   "  use-cluster     Set the default cluster for subsequent commands.\n"
                   "  info            Display information about the current or a specific cluster.\n\n"
                   "GitOps & Deployments:\n"
                   "  sync            Sync manifests from a Git repo to a cluster. Can detect drift, create PRs, or apply directly.\n"
                   "  rollout         Manage application rollouts with advanced strategies (start, promote, rollback).\n\n"
                   "Access Control (RBAC):\n"
                   "  grant           Grant a predefined role to a user or group.\n"
                   "  revoke          Revoke a role from a user or group.\n\n"
                   "Multi-Cluster Orchestration:\n"
                   "  multi           Orchestrate actions across multiple clusters simultaneously.\n"
                   "  failover        Initiate a manual failover of an application from one cluster to another.\n\n"
                   "Run 'ph kube <subcommand> --help' for more information.");
}

static phStatus handle_failover_command(int argc, const char** argv) {
    const char* app = NULL;
    const char* from_cluster = NULL;
    const char* to_cluster = NULL;

    for (int i = 0; i < argc; ++i) {
        if (strcmp(argv[i], "--app") == 0 && i + 1 < argc) {
            app = argv[++i];
        } else if (strcmp(argv[i], "--from") == 0 && i + 1 < argc) {
            from_cluster = argv[++i];
        } else if (strcmp(argv[i], "--to") == 0 && i + 1 < argc) {
            to_cluster = argv[++i];
        }
    }

    if (!app || !from_cluster || !to_cluster) {
        tui_print_error("--app, --from, and --to are required arguments for 'failover'.");
        return ph_ERROR_INVALID_ARGS;
    }

    char json_payload[1024];
    snprintf(json_payload, sizeof(json_payload),
             "{\"action\":\"failover\",\"app\":\"%s\",\"fromCluster\":\"%s\",\"toCluster\":\"%s\"}",
             app, from_cluster, to_cluster);

    logger_log_fmt(LOG_LEVEL_DEBUG, "KubeHandler", "Calling 'run_multi_cluster_orchestrator' with payload: %s", json_payload);
    int result = run_multi_cluster_orchestrator(json_payload);
    return result == 0 ? ph_SUCCESS : ph_ERROR_EXEC_FAILED;
}

static phStatus handle_grant_command(int argc, const char** argv) {
    const char* role = NULL;
    const char* subject = NULL;
    const char* cluster = NULL;

    for (int i = 0; i < argc; ++i) {
        if (strcmp(argv[i], "--role") == 0 && i + 1 < argc) {
            role = argv[++i];
        } else if (strcmp(argv[i], "--subject") == 0 && i + 1 < argc) {
            subject = argv[++i];
        } else if (strcmp(argv[i], "--cluster") == 0 && i + 1 < argc) {
            cluster = argv[++i];
        }
    }

    if (!role || !subject) {
        tui_print_error("--role and --subject are required arguments for 'grant'.");
        return ph_ERROR_INVALID_ARGS;
    }

    if (!cluster) {
        cluster = config_manager_get_current_cluster();
        if (!cluster) {
            tui_print_error("No cluster specified and no default cluster is set. Use 'ph kube use-cluster' or provide --cluster.");
            return ph_ERROR_NOT_FOUND;
        }
    }

    char json_payload[1024];
    snprintf(json_payload, sizeof(json_payload),
             "{\"action\":\"grant\",\"role\":\"%s\",\"subject\":\"%s\",\"cluster\":\"%s\"}",
             role, subject, cluster);

    logger_log_fmt(LOG_LEVEL_DEBUG, "KubeHandler", "Calling 'run_rbac_manager' with payload: %s", json_payload);
    int result = run_rbac_manager(json_payload);
    return result == 0 ? ph_SUCCESS : ph_ERROR_EXEC_FAILED;
}

static phStatus handle_revoke_command(int argc, const char** argv) {
    const char* role = NULL;
    const char* subject = NULL;
    const char* cluster = NULL;

    for (int i = 0; i < argc; ++i) {
        if (strcmp(argv[i], "--role") == 0 && i + 1 < argc) {
            role = argv[++i];
        } else if (strcmp(argv[i], "--subject") == 0 && i + 1 < argc) {
            subject = argv[++i];
        } else if (strcmp(argv[i], "--cluster") == 0 && i + 1 < argc) {
            cluster = argv[++i];
        }
    }

    if (!role || !subject) {
        tui_print_error("--role and --subject are required arguments for 'revoke'.");
        return ph_ERROR_INVALID_ARGS;
    }

    if (!cluster) {
        cluster = config_manager_get_current_cluster();
        if (!cluster) {
            tui_print_error("No cluster specified and no default cluster is set. Use 'ph kube use-cluster' or provide --cluster.");
            return ph_ERROR_NOT_FOUND;
        }
    }

    char json_payload[1024];
    snprintf(json_payload, sizeof(json_payload),
             "{\"action\":\"revoke\",\"role\":\"%s\",\"subject\":\"%s\",\"cluster\":\"%s\"}",
             role, subject, cluster);

    logger_log_fmt(LOG_LEVEL_DEBUG, "KubeHandler", "Calling 'run_rbac_manager' with payload: %s", json_payload);
    int result = run_rbac_manager(json_payload);
    return result == 0 ? ph_SUCCESS : ph_ERROR_EXEC_FAILED;
}

// --- File I/O and String Utility Helpers ---

static char* read_file_content(const char* filepath) {
    FILE* file = fopen(filepath, "rb");
    if (!file) {
        logger_log_fmt(LOG_LEVEL_ERROR, "KubeHandler", "Failed to open file: %s", filepath);
        return NULL;
    }
    fseek(file, 0, SEEK_END);
    long file_size = ftell(file);
    fseek(file, 0, SEEK_SET);
    if (file_size < 0) {
        fclose(file);
        return NULL;
    }
    char* buffer = (char*)malloc(file_size + 1);
    if (!buffer) {
        fclose(file);
        return NULL;
    }
    if (fread(buffer, 1, file_size, file) != (size_t)file_size) {
        free(buffer);
        fclose(file);
        return NULL;
    }
    buffer[file_size] = '\0';
    fclose(file);
    return buffer;
}

static char* json_escape(const char* input) {
    if (!input) return NULL;
    size_t len = strlen(input);
    char* escaped = (char*)malloc(len * 2 + 1);
    if (!escaped) return NULL;

    const char* p_in = input;
    char* p_out = escaped;
    while (*p_in) {
        switch (*p_in) {
            case '\"': *p_out++ = '\\'; *p_out++ = '\"'; break;
            case '\\': *p_out++ = '\\'; *p_out++ = '\\'; break;
            case '\n': *p_out++ = '\\'; *p_out++ = 'n';  break;
            case '\r': *p_out++ = '\\'; *p_out++ = 'r';  break;
            case '\t': *p_out++ = '\\'; *p_out++ = 't';  break;
            default:   *p_out++ = *p_in; break;
        }
        p_in++;
    }
    *p_out = '\0';
    return escaped;
}

// --- Subcommand Handler Implementations ---

static phStatus handle_sync_command(int argc, const char** argv) {
    const char* path = NULL;
    const char* cluster = NULL;
    const char* context = NULL;
    bool dry_run = false;
    bool apply = false;
    bool force = false;
    bool skip_signature_verification = false;

    for (int i = 0; i < argc; ++i) {
        if (strcmp(argv[i], "--path") == 0 && i + 1 < argc) path = argv[++i];
        else if (strcmp(argv[i], "--cluster") == 0 && i + 1 < argc) cluster = argv[++i];
        else if (strcmp(argv[i], "--context") == 0 && i + 1 < argc) context = argv[++i];
        else if (strcmp(argv[i], "--dry-run") == 0) dry_run = true;
        else if (strcmp(argv[i], "--apply") == 0) apply = true;
        else if (strcmp(argv[i], "--force") == 0) force = true;
        else if (strcmp(argv[i], "--skip-signature-verification") == 0) skip_signature_verification = true;
    }

    if (!path) {
        tui_print_error("--path is a required argument for sync.");
        return ph_ERROR_INVALID_ARGS;
    }

    if (!cluster) {
        cluster = config_manager_get_current_cluster();
        if (!cluster) {
            tui_print_error("No cluster specified and no default cluster is set. Use 'ph kube use-cluster' or provide --cluster.");
            return ph_ERROR_NOT_FOUND;
        }
    }

    char json_payload[2048];
    char context_json_part[512];

    if (context) {
        snprintf(context_json_part, sizeof(context_json_part), "\"%s\"", context);
    } else {
        snprintf(context_json_part, sizeof(context_json_part), "null");
    }

    snprintf(json_payload, sizeof(json_payload),
             "{"
             "\"action\":\"sync\","
             "\"path\":\"%s\","
             "\"cluster\":\"%s\","
             "\"context\":%s,"
             "\"dry_run\":%s,"
             "\"force\":%s,"
             "\"apply\":%s,"
             "\"skip_signature_verification\":%s"
             "}",
             path, cluster, context_json_part,
             dry_run ? "true" : "false",
             force ? "true" : "false",
             apply ? "true" : "false",
             skip_signature_verification ? "true" : "false");

    logger_log_fmt(LOG_LEVEL_DEBUG, "KubeHandler", "Calling 'run_sync' with payload: %s", json_payload);
    
    char error_buffer[512] = {0};
    int result = run_sync(json_payload, error_buffer, sizeof(error_buffer));
    
    if (result != 0) {
        tui_print_error(error_buffer);
        return ph_ERROR_EXEC_FAILED;
    }
    
    return ph_SUCCESS;
}

/**
 * @brief Handles the 'rollout' subcommand and its actions (start, promote, rollback).
 *
 * This function parses the arguments for the rollout command, constructs a JSON
 * payload based on the specified action, and invokes the Rust release orchestrator.
 * - 'start': Initiates a new release. Requires --type, --app, and --image.
 * - 'promote': Manually promotes an ongoing release. Requires --app.
 * - 'rollback': Manually rolls back an ongoing release. Requires --app.
 * The Rust backend interprets the 'type' field in the JSON to determine the
 * appropriate action to take on the corresponding phRelease custom resource.
 */
static phStatus handle_rollout_command(int argc, const char** argv) {
    if (argc < 1) {
        tui_print_error("Subcommand required for 'rollout'. Use 'start', 'promote', or 'rollback'.");
        return ph_ERROR_INVALID_ARGS;
    }

    const char* action = argv[0];
    char json_payload[2048];

    if (strcmp(action, "start") == 0) {
        const char* type = NULL, *app = NULL, *image = NULL, *steps = NULL, *metric = NULL, *analysis_window = NULL;
        const char* public_key_file = NULL;
        bool skip_sig_check = false;

        for (int i = 1; i < argc; ++i) {
            if (strcmp(argv[i], "--type") == 0 && i + 1 < argc) type = argv[++i];
            else if (strcmp(argv[i], "--app") == 0 && i + 1 < argc) app = argv[++i];
            else if (strcmp(argv[i], "--image") == 0 && i + 1 < argc) image = argv[++i];
            else if (strcmp(argv[i], "--steps") == 0 && i + 1 < argc) steps = argv[++i];
            else if (strcmp(argv[i], "--metric") == 0 && i + 1 < argc) metric = argv[++i];
            else if (strcmp(argv[i], "--analysis-window") == 0 && i + 1 < argc) analysis_window = argv[++i];
            else if (strcmp(argv[i], "--public-key-file") == 0 && i + 1 < argc) public_key_file = argv[++i];
            else if (strcmp(argv[i], "--skip-sig-check") == 0) skip_sig_check = true;
        }
        if (!type || !app || !image) {
            tui_print_error("--type, --app, and --image are required for 'rollout start'.");
            return ph_ERROR_INVALID_ARGS;
        }

        // Build the JSON payload piece by piece
        char payload_buffer[4096];
        char* ptr = payload_buffer;
        const char* end = payload_buffer + sizeof(payload_buffer);

        ptr += snprintf(ptr, end - ptr, "{\"type\":\"start\",\"strategy\":\"%s\",\"app\":\"%s\",\"image\":\"%s\",\"skipSigCheck\":%s",
                        type, app, image, skip_sig_check ? "true" : "false");

        if (public_key_file) {
            char* key_content = read_file_content(public_key_file);
            if (key_content) {
                char* escaped_key = json_escape(key_content);
                if (escaped_key) {
                    ptr += snprintf(ptr, end - ptr, ",\"publicKey\":\"%s\"", escaped_key);
                    free(escaped_key);
                }
                free(key_content);
            } else {
                tui_print_warning("Could not read public key file, proceeding without it.");
            }
        }


        if (steps) {
            ptr += snprintf(ptr, end - ptr, ",\"steps\":\"%s\"", steps);
        }
        if (metric) {
            char* escaped_metric = json_escape(metric);
            if(escaped_metric) {
                ptr += snprintf(ptr, end - ptr, ",\"metric\":\"%s\"", escaped_metric);
                free(escaped_metric);
            }
        }
        if (analysis_window) {
            ptr += snprintf(ptr, end - ptr, ",\"analysisWindow\":\"%s\"", analysis_window);
        }

        snprintf(ptr, end - ptr, "}"); // Close the JSON object

        logger_log_fmt(LOG_LEVEL_DEBUG, "KubeHandler", "Calling 'run_release_orchestrator' with payload: %s", payload_buffer);
        unsigned char error_buffer[1024] = {0};
        int result = run_release_orchestrator(payload_buffer, error_buffer, sizeof(error_buffer));
        if (result != 0) {
            tui_print_error("Release command failed. See details below.");
            // In a real implementation, we would deserialize the protobuf message from error_buffer.
            // For now, we simulate this by printing a structured message.
            // Example: ErrorPayload* err = error_payload__unpack(NULL, sizeof(error_buffer), error_buffer);
            tui_print_info("\n--- Error Details ---");
            tui_print_info("Error Code: ReleaseFailed (from Protobuf)");
            tui_print_info("Message: The error message from Rust would be here.");
            tui_print_info("Details: The detailed error chain from Rust would be here.");
            tui_print_info("---------------------\n");
            return ph_ERROR_EXEC_FAILED;
        }
        return ph_SUCCESS;

    } else if (strcmp(action, "status") == 0) {
        const char* id = NULL;
        bool watch = false;
        for (int i = 1; i < argc; ++i) {
            if (strcmp(argv[i], "--id") == 0 && i + 1 < argc) id = argv[++i];
            else if (strcmp(argv[i], "--watch") == 0) watch = true;
        }
        if (!id) {
            tui_print_error("--id is required for 'rollout status'.");
            return ph_ERROR_INVALID_ARGS;
        }
        snprintf(json_payload, sizeof(json_payload),
                 "{\"type\":\"status\",\"id\":\"%s\",\"watch\":%s}",
                 id, watch ? "true" : "false");
        
        logger_log_fmt(LOG_LEVEL_DEBUG, "KubeHandler", "Calling 'run_release_orchestrator' with payload: %s", json_payload);
        // Note: Applying the same error handling pattern to all calls.
        unsigned char error_buffer[1024] = {0};
        int result = run_release_orchestrator(json_payload, error_buffer, sizeof(error_buffer));
        if (result != 0) {
            tui_print_error("Rollout status command failed.");
            return ph_ERROR_EXEC_FAILED;
        }
        return ph_SUCCESS;

    } else if (strcmp(action, "plan") == 0) {
        const char* type = NULL, *app = NULL, *image = NULL;
        bool preview_url = false;
        for (int i = 1; i < argc; ++i) {
            if (strcmp(argv[i], "--type") == 0 && i + 1 < argc) type = argv[++i];
            else if (strcmp(argv[i], "--app") == 0 && i + 1 < argc) app = argv[++i];
            else if (strcmp(argv[i], "--image") == 0 && i + 1 < argc) image = argv[++i];
            else if (strcmp(argv[i], "--preview-url") == 0) preview_url = true;
        }
        if (!type || !app || !image) {
            tui_print_error("--type, --app, and --image are required for 'rollout plan'.");
            return ph_ERROR_INVALID_ARGS;
        }
        snprintf(json_payload, sizeof(json_payload),
                 "{\"type\":\"plan\",\"strategy\":\"%s\",\"app\":\"%s\",\"image\":\"%s\",\"preview_url\":%s}",
                 type, app, image, preview_url ? "true" : "false");

        logger_log_fmt(LOG_LEVEL_DEBUG, "KubeHandler", "Calling 'run_release_orchestrator' with payload: %s", json_payload);
        unsigned char error_buffer[1024] = {0};
        int result = run_release_orchestrator(json_payload, error_buffer, sizeof(error_buffer));
        if (result != 0) {
            tui_print_error("Rollout plan command failed.");
            return ph_ERROR_EXEC_FAILED;
        }
        return ph_SUCCESS;

    } else if (strcmp(action, "promote") == 0 || strcmp(action, "rollback") == 0) {
        const char* id = NULL;
        const char* to_revision_str = NULL;

        for (int i = 1; i < argc; ++i) {
            if (strcmp(argv[i], "--id") == 0 && i + 1 < argc) {
                id = argv[++i];
            } else if (strcmp(action, "rollback") == 0 && strcmp(argv[i], "--to-revision") == 0 && i + 1 < argc) {
                to_revision_str = argv[++i];
            }
        }

        if (!id) {
            char error_msg[128];
            snprintf(error_msg, sizeof(error_msg), "--id is required for 'rollout %s'.", action);
            tui_print_error(error_msg);
            return ph_ERROR_INVALID_ARGS;
        }

        char payload_buffer[1024];
        char* ptr = payload_buffer;
        const char* end = payload_buffer + sizeof(payload_buffer);

        ptr += snprintf(ptr, end - ptr, "{\"type\":\"%s\",\"id\":\"%s\"", action, id);

        if (to_revision_str) {
            // Basic validation: check if it's a number.
            for (const char* p = to_revision_str; *p; p++) {
                if (*p < '0' || *p > '9') {
                    tui_print_error("--to-revision must be a positive integer.");
                    return ph_ERROR_INVALID_ARGS;
                }
            }
            ptr += snprintf(ptr, end - ptr, ",\"toRevision\":%s", to_revision_str);
        }

        snprintf(ptr, end - ptr, "}");

        logger_log_fmt(LOG_LEVEL_DEBUG, "KubeHandler", "Calling 'run_release_orchestrator' with payload: %s", payload_buffer);
        unsigned char error_buffer[1024] = {0};
        int result = run_release_orchestrator(json_payload, error_buffer, sizeof(error_buffer));
        if (result != 0) {
            char error_title[128];
            snprintf(error_title, sizeof(error_title), "Rollout %s command failed.", action);
            tui_print_error(error_title);
            return ph_ERROR_EXEC_FAILED;
        }
        return ph_SUCCESS;

    } else {
        tui_print_error("Unknown action for 'rollout'. Use 'start', 'status', 'plan', 'promote', or 'rollback'.");
        return ph_ERROR_NOT_FOUND;
    }
}

/* BEGIN CHANGE: Implement cluster management commands with English localization. */
/**
 * @brief Handles the 'list-clusters' subcommand.
 *
 * Fetches the list of all available clusters from the configuration manager,
 * prints them to the console, and highlights the currently active cluster.
 */
static phStatus handle_list_clusters_command(int argc, const char** argv) {
    (void)argc; // Unused for now
    (void)argv; // Unused for now
    char** cluster_list = NULL;
    int count = 0;

    if (config_manager_get_clusters(&cluster_list, &count) != ph_SUCCESS) {
        tui_print_error("Failed to read cluster configuration.");
        return ph_ERROR_GENERAL;
    }

    if (count == 0) {
        tui_print_info("No clusters defined in the configuration.");
    } else {
        const char* current_cluster = config_manager_get_current_cluster();
        tui_print_info("Available clusters:");
        for (int i = 0; i < count; ++i) {
            if (current_cluster && strcmp(current_cluster, cluster_list[i]) == 0) {
                printf("  * %s (active)\n", cluster_list[i]);
            } else {
                printf("  - %s\n", cluster_list[i]);
            }
        }
    }

    config_manager_free_cluster_list(cluster_list, count);
    return ph_SUCCESS;
}

/**
 * @brief Handles the 'use-cluster' subcommand.
 *
 * Sets the specified cluster as the active context for subsequent commands.
 * It validates that a cluster name is provided and that it exists in the
 * configuration.
 */
static phStatus handle_use_cluster_command(int argc, const char** argv) {
    if (argc < 1) {
        tui_print_error("Cluster name is required. Usage: ph kube use-cluster <cluster-name>");
        return ph_ERROR_INVALID_ARGS;
    }
    const char* cluster_name = argv[0];

    if (config_manager_set_current_cluster(cluster_name) != ph_SUCCESS) {
        char error_msg[256];
        snprintf(error_msg, sizeof(error_msg), "Failed to set active cluster to '%s'. Does it exist in the configuration?", cluster_name);
        tui_print_error(error_msg);
        return ph_ERROR_GENERAL;
    }

    char success_msg[256];
    snprintf(success_msg, sizeof(success_msg), "Default cluster set to '%s'.", cluster_name);
    tui_print_success(success_msg);
    return ph_SUCCESS;
}

/**
 * @brief Handles the 'info' subcommand.
 *
 * Displays information about a specific cluster. If no cluster is specified,
 * it uses the currently active cluster context.
 */
static phStatus handle_info_command(int argc, const char** argv) {
    const char* cluster = NULL;
    if (argc > 0) {
        // User can specify a cluster: ph kube info my-cluster
        cluster = argv[0];
    } else {
        // If not, use the active cluster
        cluster = config_manager_get_current_cluster();
        if (cluster == NULL) {
            tui_print_error("No default cluster is set. Specify one or use 'ph kube use-cluster'.");
            return ph_ERROR_NOT_FOUND;
        }
    }

    char json_payload[256];
    snprintf(json_payload, sizeof(json_payload), "{\"cluster\":\"%s\"}", cluster);

    logger_log_fmt(LOG_LEVEL_DEBUG, "KubeHandler", "Calling 'run_k8s_info' with payload: %s", json_payload);

    int result = run_k8s_info(json_payload);

    return result == 0 ? ph_SUCCESS : ph_ERROR_EXEC_FAILED;
}
/* END CHANGE */

#define MAX_CLUSTERS 32
#define JSON_BUFFER_SIZE 8192

static phStatus handle_multi_command(int argc, const char** argv) {
    if (argc < 1) {
        tui_print_error("No action provided for 'multi'. Usage: ph kube multi <action> [options]");
        return ph_ERROR_INVALID_ARGS;
    }
    const char* action = argv[0];
    if (strcmp(action, "apply") != 0) {
        tui_print_error("Only 'apply' action is supported for 'multi' command.");
        return ph_ERROR_INVALID_ARGS;
    }

    const char* clusters_str = NULL, *path = NULL, *strategy = NULL, *app_name = NULL, *namespace = "default";
    for (int i = 1; i < argc; ++i) {
        if (strcmp(argv[i], "--clusters") == 0 && i + 1 < argc) clusters_str = argv[++i];
        else if (strcmp(argv[i], "--path") == 0 && i + 1 < argc) path = argv[++i];
        else if (strcmp(argv[i], "--strategy") == 0 && i + 1 < argc) strategy = argv[++i];
        else if (strcmp(argv[i], "--app-name") == 0 && i + 1 < argc) app_name = argv[++i];
        else if (strcmp(argv[i], "--namespace") == 0 && i + 1 < argc) namespace = argv[++i];
    }
    if (!clusters_str || !path || !app_name) {
        tui_print_error("--clusters, --path, and --app-name are required for multi apply.");
        return ph_ERROR_INVALID_ARGS;
    }

    char* clusters_copy = strdup(clusters_str);
    if (!clusters_copy) return ph_ERROR_MEMORY_ALLOC;
    const char* cluster_names[MAX_CLUSTERS];
    int cluster_count = 0;
    char* token = strtok(clusters_copy, ",");
    while (token != NULL && cluster_count < MAX_CLUSTERS) {
        cluster_names[cluster_count++] = token;
        token = strtok(NULL, ",");
    }

    char* manifest_content = read_file_content(path);
    if (!manifest_content) {
        free(clusters_copy);
        tui_print_error("Failed to read manifest file.");
        return ph_ERROR_IO;
    }
    char* escaped_manifest = json_escape(manifest_content);
    if (!escaped_manifest) {
        free(manifest_content);
        free(clusters_copy);
        return ph_ERROR_MEMORY_ALLOC;
    }

    char json_buffer[JSON_BUFFER_SIZE];
    char* ptr = json_buffer;
    const char* end = json_buffer + JSON_BUFFER_SIZE;
    ptr += snprintf(ptr, end - ptr, "{\"cluster_configs\":{");
    for (int i = 0; i < cluster_count; ++i) {
        ptr += snprintf(ptr, end - ptr, "\"%s\":\"/etc/ph/kubeconfigs/%s.yaml\"%s",
                        cluster_names[i], cluster_names[i], (i == cluster_count - 1) ? "" : ",");
    }
    ptr += snprintf(ptr, end - ptr, "},\"targets\":[");
    for (int i = 0; i < cluster_count; ++i) {
        ptr += snprintf(ptr, end - ptr, "{\"name\":\"%s\"}%s",
                        cluster_names[i], (i == cluster_count - 1) ? "" : ",");
    }
    ptr += snprintf(ptr, end - ptr, "],\"action\":{\"type\":\"apply\",\"manifests\":\"%s\",\"app_name\":\"%s\",\"namespace\":\"%s\",\"strategy\":{\"type\":\"%s\"}}}",
                    escaped_manifest, app_name, namespace, strategy ? strategy : "direct");


    logger_log_fmt(LOG_LEVEL_DEBUG, "KubeHandler", "Calling 'multi_cluster_orchestrator' with payload: %s", json_buffer);
    int result = run_multi_cluster_orchestrator(json_buffer);

    free(clusters_copy);
    free(manifest_content);
    free(escaped_manifest);
    return result == 0 ? ph_SUCCESS : ph_ERROR_EXEC_FAILED;
}

static phStatus handle_cluster_command(int argc, const char** argv) {
    if (argc < 1) {
        tui_print_error("No action provided for 'cluster'. Usage: ph kube cluster <action> [options]");
        return ph_ERROR_INVALID_ARGS;
    }

    const char* action = argv[0];
    if (strcmp(action, "policy") != 0) {
        tui_print_error("Only 'policy' action is supported for 'cluster' command.");
        return ph_ERROR_INVALID_ARGS;
    }

    // After 'policy', we expect the cluster name, then options.
    if (argc < 3) { // Expecting "policy", "cluster_name", "--policy-file", "path"
        tui_print_error("Usage: ph kube cluster policy <cluster-name> --policy-file <file-path>");
        return ph_ERROR_INVALID_ARGS;
    }

    const char* cluster_name = argv[1];
    const char* policy_file_path = NULL;

    for (int i = 2; i < argc; ++i) {
        if (strcmp(argv[i], "--policy-file") == 0 && i + 1 < argc) {
            policy_file_path = argv[++i];
        }
    }

    if (policy_file_path == NULL) {
        tui_print_error("--policy-file is a required argument.");
        return ph_ERROR_INVALID_ARGS;
    }

    // The Rust side will handle reading the file, so we just pass the path.
    char json_payload[2048];
    snprintf(json_payload, sizeof(json_payload),
             "{"
             "\"action\":\"set_policy\","
             "\"cluster_name\":\"%s\","
             "\"policy_file_path\":\"%s\""
             "}",
             cluster_name, policy_file_path);
    
    logger_log_fmt(LOG_LEVEL_DEBUG, "KubeHandler", "Calling 'run_multi_cluster_orchestrator' with payload: %s", json_payload);
    int result = run_multi_cluster_orchestrator(json_payload);

    return result == 0 ? ph_SUCCESS : ph_ERROR_EXEC_FAILED;
}

static phStatus handle_drift_command(int argc, const char** argv) {
    const char* cluster = NULL;
    const char* path = "."; // Default to current directory
    const char* since = NULL;
    const char* label = NULL;
    bool open_pr = false;
    bool auto_apply = false;

    for (int i = 0; i < argc; ++i) {
        if (strcmp(argv[i], "--cluster") == 0 && i + 1 < argc) cluster = argv[++i];
        else if (strcmp(argv[i], "--path") == 0 && i + 1 < argc) path = argv[++i];
        else if (strcmp(argv[i], "--since") == 0 && i + 1 < argc) since = argv[++i];
        else if (strcmp(argv[i], "--label") == 0 && i + 1 < argc) label = argv[++i];
        else if (strcmp(argv[i], "--open-pr") == 0) open_pr = true;
        else if (strcmp(argv[i], "--auto-apply") == 0) auto_apply = true;
    }

    if (!cluster) {
        cluster = config_manager_get_current_cluster();
        if (!cluster) {
            tui_print_error("No cluster specified and no default cluster is set. Use 'ph kube use-cluster' or provide --cluster.");
            return ph_ERROR_NOT_FOUND;
        }
    }
    
    if (open_pr && auto_apply) {
        tui_print_error("--open-pr and --auto-apply are mutually exclusive flags.");
        return ph_ERROR_INVALID_ARGS;
    }

    char json_payload[2048];
    char since_json_part[256];
    char label_json_part[256];

    if (since) {
        snprintf(since_json_part, sizeof(since_json_part), "\"%s\"", since);
    } else {
        snprintf(since_json_part, sizeof(since_json_part), "null");
    }

    if (label) {
        snprintf(label_json_part, sizeof(label_json_part), "\"%s\"", label);
    } else {
        snprintf(label_json_part, sizeof(label_json_part), "null");
    }

    snprintf(json_payload, sizeof(json_payload),
             "{"
             "\"action\":\"drift\","
             "\"cluster\":\"%s\","
             "\"path\":\"%s\","
             "\"since\":%s,"
             "\"label\":%s,"
             "\"open_pr\":%s,"
             "\"auto_apply\":%s"
             "}",
             cluster,
             path,
             since_json_part,
             label_json_part,
             open_pr ? "true" : "false",
             auto_apply ? "true" : "false");

    logger_log_fmt(LOG_LEVEL_DEBUG, "KubeHandler", "Calling 'run_drift_detector' with payload: %s", json_payload);
    
    char error_buffer[512] = {0};
    int result = run_drift_detector(json_payload, error_buffer, sizeof(error_buffer));

    if (result != 0) {
        tui_print_error(error_buffer);
        return ph_ERROR_EXEC_FAILED;
    }

    return ph_SUCCESS;
}