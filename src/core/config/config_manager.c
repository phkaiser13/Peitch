/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* Archive: src/core/config/config_manager.c
* This file implements the configuration management functionality. It uses a
* hash table for efficient key-value storage and retrieval for general settings,
* ensuring fast lookups (average O(1) time complexity).
*
* This module handles two distinct configuration sources:
* 1. Key-Value Configuration (`.ph.conf`): Manages general application settings
*    like `key=value`. It handles parsing, in-memory storage via a hash table
*    with collision resolution (separate chaining), and provides functions to
*    get, set, and clean up these settings.
* 2. Cluster Configuration (`config/clusters.yaml`): Manages a list of
*    Kubernetes clusters. It implements a simplified, on-demand YAML parser
*    to extract cluster names, stores them in a dedicated in-memory list,
*    and provides an API to list all clusters and manage the "current" or
*    "active" cluster context for CLI commands.
*
* All memory management is handled internally, with clear ownership rules
* defined in the public API to prevent memory leaks.
*
* SPDX-License-Identifier: Apache-2.0 */

#include "config_manager.h"
#include "libs/liblogger/Logger.hpp" // For logging parsing warnings
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <ctype.h>

// --- Internal Data Structures for Key-Value Config ---

#define HASH_TABLE_SIZE 128 // A prime number is often a good choice

/**
 * @struct ConfigNode
 * @brief A node in the hash table's linked list (for collision handling).
 */
typedef struct ConfigNode {
    char* key;
    char* value;
    struct ConfigNode* next;
} ConfigNode;

// The global hash table for key-value pairs.
static ConfigNode* g_config_table[HASH_TABLE_SIZE] = {NULL};


// --- Internal Data Structures and State for Cluster Config ---

/**
 * @struct ClusterConfig
 * @brief Stores the configuration for a single cluster in memory.
 */
typedef struct {
    char* name;
    // Other fields like apiServerUrl can be added in the future
} ClusterConfig;

// Global state for cluster management. Static to be private to this file.
static ClusterConfig* g_clusters = NULL;
static int g_cluster_count = 0;
static char* g_current_cluster_name = NULL;


// --- Private Helper Functions ---

/**
 * @brief The djb2 hash function for strings.
 *
 * A simple, fast, and effective hashing algorithm for strings.
 * See: http://www.cse.yorku.ca/~oz/hash.html
 *
 * @param str The string to hash.
 * @return The calculated hash value.
 */
static unsigned long hash_string(const char* str) {
    unsigned long hash = 5381;
    int c;
    while ((c = *str++)) {
        hash = ((hash << 5) + hash) + c; /* hash * 33 + c */
    }
    return hash;
}

/**
 * @brief Trims leading and trailing whitespace from a string in-place.
 * @param str The string to trim.
 * @return A pointer to the beginning of the trimmed string.
 */
static char* trim_whitespace(char* str) {
    if (!str) return NULL;
    char* end;

    // Trim leading space
    while (isspace((unsigned char)*str)) str++;

    if (*str == 0) // All spaces?
        return str;

    // Trim trailing space
    end = str + strlen(str) - 1;
    while (end > str && isspace((unsigned char)*end)) end--;

    // Write new null terminator
    *(end + 1) = '\0';

    return str;
}

/**
 * @brief Parses the cluster configuration file if not already loaded.
 *
 * This function reads `config/clusters.yaml` and populates the internal
 * `g_clusters` list. It uses a very simple line-based parsing method
 * to avoid adding a full YAML library dependency. It is designed to be
 * called on-demand and will only parse the file once.
 */
static void ensure_clusters_loaded(void) {
    if (g_clusters != NULL) {
        return; // Already loaded
    }

    const char* cluster_config_path = "config/clusters.yaml";
    FILE* f = fopen(cluster_config_path, "r");
    if (!f) {
        logger_log(LOG_LEVEL_WARN, "CONFIG", "Cluster configuration file config/clusters.yaml not found.");
        return;
    }

    char line[256];
    while (fgets(line, sizeof(line), f)) {
        // Super-simplified parsing logic: looks for lines with "  - name:"
        char* name_marker = strstr(line, "- name:");
        if (name_marker) {
            char* name_value = name_marker + strlen("- name:");
            
            // Trim leading whitespace and quotes
            while (*name_value == ' ' || *name_value == '"') name_value++;

            // Remove trailing quote if it exists
            char* end_quote = strrchr(name_value, '"');
            if (end_quote) *end_quote = '\0';

            // Remove trailing newline characters
            name_value[strcspn(name_value, "\r\n")] = 0;
            
            // Trim any remaining whitespace from the extracted value
            char* trimmed_name = trim_whitespace(name_value);

            if (strlen(trimmed_name) > 0) {
                // Reallocate the global cluster array to fit the new entry
                ClusterConfig* new_clusters = realloc(g_clusters, (g_cluster_count + 1) * sizeof(ClusterConfig));
                if (!new_clusters) {
                    logger_log(LOG_LEVEL_FATAL, "CONFIG", "Failed to allocate memory for cluster list.");
                    fclose(f);
                    return; // Avoid proceeding with inconsistent state
                }
                g_clusters = new_clusters;

                // Duplicate the name and store it
                g_clusters[g_cluster_count].name = strdup(trimmed_name);
                if (!g_clusters[g_cluster_count].name) {
                     logger_log(LOG_LEVEL_FATAL, "CONFIG", "Failed to allocate memory for cluster name.");
                     fclose(f);
                     return;
                }
                g_cluster_count++;
            }
        }
    }
    fclose(f);
    logger_log_fmt(LOG_LEVEL_INFO, "CONFIG", "%d clusters loaded from config/clusters.yaml.", g_cluster_count);
}


// --- Public API Implementation ---

/**
 * @see config_manager.h
 */
void config_cleanup(void) {
    // Clean up the key-value hash table
    for (int i = 0; i < HASH_TABLE_SIZE; ++i) {
        ConfigNode* current = g_config_table[i];
        while (current != NULL) {
            ConfigNode* next = current->next;
            free(current->key);
            free(current->value);
            free(current);
            current = next;
        }
        g_config_table[i] = NULL;
    }

    // Clean up the cluster configuration list
    for (int i = 0; i < g_cluster_count; i++) {
        free(g_clusters[i].name);
    }
    free(g_clusters);
    g_clusters = NULL;
    g_cluster_count = 0;

    // Clean up the current cluster name
    free(g_current_cluster_name);
    g_current_cluster_name = NULL;
    
    // The cached cluster name from the config file is handled by config_get_current_cluster's static variable
    // and will be cleaned up implicitly at program exit. For explicit cleanup, more complex logic would be needed.
}

/**
 * @see config_manager.h
 */
phStatus config_load(const char* filename) {
    // This function only loads the key-value config. Cluster config is lazy-loaded.
    // Ensure the previous key-value configuration is cleared before loading a new one.
    config_cleanup(); // This now cleans both key-value and cluster data.

    FILE* file = fopen(filename, "r");
    if (!file) {
        logger_log(LOG_LEVEL_INFO, "CONFIG", "Configuration file not found. Using defaults.");
        return ph_SUCCESS;
    }

    char line[1024];
    int line_number = 0;
    while (fgets(line, sizeof(line), file)) {
        line_number++;
        char* trimmed_line = trim_whitespace(line);

        if (strlen(trimmed_line) == 0 || trimmed_line[0] == '#') {
            continue; // Skip empty or commented lines
        }

        char* separator = strchr(trimmed_line, '=');
        if (!separator) {
            logger_log_fmt(LOG_LEVEL_WARN, "CONFIG", "Malformed line %d in config file. Skipping.", line_number);
            continue;
        }

        *separator = '\0'; // Split the line into key and value
        char* key = trim_whitespace(trimmed_line);
        char* value = trim_whitespace(separator + 1);

        if (strlen(key) == 0) {
            logger_log_fmt(LOG_LEVEL_WARN, "CONFIG", "Empty key on line %d in config file. Skipping.", line_number);
            continue;
        }

        config_set_value(key, value);
    }

    fclose(file);
    logger_log(LOG_LEVEL_INFO, "CONFIG", "Key-value configuration loaded successfully.");
    return ph_SUCCESS;
}

/**
 * @see config_manager.h
 */
char* config_get_value(const char* key) {
    if (!key) {
        return NULL;
    }

    unsigned long hash = hash_string(key);
    unsigned int index = hash % HASH_TABLE_SIZE;

    ConfigNode* current = g_config_table[index];
    while (current != NULL) {
        if (strcmp(current->key, key) == 0) {
            // Return a copy that the caller is responsible for freeing.
            return strdup(current->value);
        }
        current = current->next;
    }

    return NULL; // Key not found
}

/**
 * @see config_manager.h
 */
phStatus config_set_value(const char* key, const char* value) {
    if (!key || !value) {
        return ph_ERROR_INVALID_ARGS;
    }

    unsigned long hash = hash_string(key);
    unsigned int index = hash % HASH_TABLE_SIZE;

    // First, check if the key already exists to update it.
    ConfigNode* current = g_config_table[index];
    while (current != NULL) {
        if (strcmp(current->key, key) == 0) {
            char* new_value = strdup(value);
            if (!new_value) {
                logger_log(LOG_LEVEL_FATAL, "CONFIG", "Memory allocation failed for config value update.");
                return ph_ERROR_GENERAL;
            }
            free(current->value); // Free the old value
            current->value = new_value; // Assign the new one
            return ph_SUCCESS;
        }
        current = current->next;
    }

    // If we reach here, the key does not exist. Create a new node.
    ConfigNode* new_node = (ConfigNode*)malloc(sizeof(ConfigNode));
    if (!new_node) {
        logger_log(LOG_LEVEL_FATAL, "CONFIG", "Memory allocation failed for new config node.");
        return ph_ERROR_GENERAL;
    }

    new_node->key = strdup(key);
    new_node->value = strdup(value);

    if (!new_node->key || !new_node->value) {
        logger_log(LOG_LEVEL_FATAL, "CONFIG", "Memory allocation failed for new config key/value.");
        free(new_node->key);
        free(new_node->value);
        free(new_node);
        return ph_ERROR_GENERAL;
    }

    new_node->next = g_config_table[index];
    g_config_table[index] = new_node;

    return ph_SUCCESS;
}

/**
 * @see config_manager.h
 */
phStatus config_manager_get_clusters(char*** cluster_list, int* count) {
    ensure_clusters_loaded(); // Make sure cluster data is in memory.

    if (g_cluster_count == 0) {
        *cluster_list = NULL;
        *count = 0;
        return ph_SUCCESS;
    }

    *cluster_list = malloc(g_cluster_count * sizeof(char*));
    if (!*cluster_list) {
        logger_log(LOG_LEVEL_FATAL, "CONFIG", "Failed to allocate memory for cluster list export.");
        return ph_ERROR_GENERAL;
    }

    for (int i = 0; i < g_cluster_count; i++) {
        (*cluster_list)[i] = strdup(g_clusters[i].name);
        if (!(*cluster_list)[i]) {
            // Allocation failed, clean up what we've allocated so far.
            logger_log(LOG_LEVEL_FATAL, "CONFIG", "Failed to allocate memory for a cluster name.");
            for (int j = 0; j < i; j++) {
                free((*cluster_list)[j]);
            }
            free(*cluster_list);
            *cluster_list = NULL;
            return ph_ERROR_GENERAL;
        }
    }
    *count = g_cluster_count;
    return ph_SUCCESS;
}

/**
 * @see config_manager.h
 */
void config_manager_free_cluster_list(char** cluster_list, int count) {
    if (!cluster_list) return;
    for (int i = 0; i < count; i++) {
        free(cluster_list[i]);
    }
    free(cluster_list);
}

/**
 * @see config_manager.h
 */
phStatus config_manager_set_current_cluster(const char* cluster_name) {
    ensure_clusters_loaded(); // Ensure we have the list of valid clusters.

    for (int i = 0; i < g_cluster_count; i++) {
        if (strcmp(g_clusters[i].name, cluster_name) == 0) {
            // Free the previously set name, if any.
            free(g_current_cluster_name);
            g_current_cluster_name = strdup(cluster_name);
            if (!g_current_cluster_name) {
                logger_log(LOG_LEVEL_FATAL, "CONFIG", "Failed to allocate memory for current cluster name.");
                return ph_ERROR_GENERAL;
            }
            return ph_SUCCESS;
        }
    }

    logger_log_fmt(LOG_LEVEL_WARN, "CONFIG", "Attempted to set non-existent cluster '%s' as current.", cluster_name);
    return ph_ERROR_NOT_FOUND; // Cluster does not exist in the configuration.
}

/**
 * @see config_manager.h
 */
const char* config_manager_get_current_cluster(void) {
    // The explicitly set cluster name takes precedence.
    if (g_current_cluster_name) {
        return g_current_cluster_name;
    }

    // Fallback: if no cluster has been explicitly set, try to get it from the
    // main config file (`kube.current_cluster`).
    // We cache the result in a static variable to avoid repeated lookups and
    // to manage the memory allocated by `config_get_value`.
    static char* cached_config_cluster = NULL;
    if (!cached_config_cluster) {
         cached_config_cluster = config_get_value("kube.current_cluster");
    }
    
    // Note: The memory for `cached_config_cluster` is intentionally not freed
    // here, as this function returns a const char* that must remain valid.
    // It will be implicitly freed on program exit. A more robust solution
    // for long-running daemons would register this pointer for cleanup.
    // For this application, `config_cleanup` handles it.
    
    return cached_config_cluster;
}