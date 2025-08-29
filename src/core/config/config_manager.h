/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* Archive: src/core/config/config_manager.h
* This header file defines the public API for the configuration manager module.
* This module is responsible for parsing and providing access to application settings.
*
* It handles two types of configurations:
* 1. A simple key-value store from a file (e.g., `.ph.conf`), where each line
*    is a `key=value` pair. This is used for general application settings.
* 2. A list of Kubernetes clusters from a YAML file (`config/clusters.yaml`).
*    This allows the system to manage multiple cluster contexts, including
*    listing them and setting an "active" cluster for operations.
*
* The manager abstracts all file I/O and parsing logic, providing a clean and
* centralized interface for the rest of the application to retrieve and manage
* configuration data, preventing this logic from being scattered throughout
* the codebase.
*
* SPDX-License-Identifier: Apache-2.0 */

#ifndef CONFIG_MANAGER_H
#define CONFIG_MANAGER_H

#include "../../ipc/include/ph_core_api.h" // For phStatus enum

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @brief Loads configuration settings from a specified file into memory.
 *
 * This function reads the given file line by line, parsing `key=value` pairs.
 * It ignores empty lines and lines starting with '#' (comments). Any existing
 * configuration in memory is cleared before loading the new file. If the file
 * cannot be opened, it returns an error, but the application can proceed with
 * default values.
 *
 * @param filename The path to the configuration file.
 * @return ph_SUCCESS if the file was loaded successfully or if it doesn't
 *         exist (which is not a fatal error). Returns an error code like
 *         ph_ERROR_GENERAL on file read errors.
 */
phStatus config_load(const char* filename);

/**
 * @brief Retrieves a configuration value for a given key.
 *
 * This function performs a lookup in the in-memory configuration store.
 * It returns a NEWLY ALLOCATED string containing the value. The caller
 * OWNS this memory and MUST free it using `free()` when it is no longer needed.
 * This design prevents memory corruption when the value is passed to other
 * modules or scripting engines that might try to manage its lifecycle.
 *
 * @param key The null-terminated string key to look up.
 * @return A pointer to a newly allocated value string, or NULL if the key is
 *         not found or if memory allocation fails.
 */
char* config_get_value(const char* key);

/**
 * @brief Sets or updates a configuration value in memory.
 *
 * This function adds a new key-value pair to the configuration or updates the
 * value of an existing key. The key and value strings are copied internally,
 * so the caller does not need to keep the original strings valid after this
 * function returns. This function does not persist the change to a file.
 *
 * @param key The null-terminated string key to set. Cannot be NULL.
 * @param value The null-terminated string value to associate with the key. Cannot be NULL.
 * @return ph_SUCCESS on success, or an error code (e.g., ph_ERROR_GENERAL)
 *         if memory allocation fails.
 */
phStatus config_set_value(const char* key, const char* value);

/**
 * @brief Frees all resources used by the configuration manager.
 *
 * This function should be called once at application shutdown to deallocate
 * all memory used for storing the configuration keys and values, preventing
 * memory leaks.
 */
void config_cleanup(void);

/**
 * @brief Gets a list with the names of all configured clusters.
 *
 * This function allocates memory for a list of strings. The caller is
 * responsible for freeing this memory using config_manager_free_cluster_list.
 *
 * @param[out] cluster_list A pointer to the array of strings (char**) that will be allocated.
 * @param[out] count A pointer to an integer that will receive the number of clusters.
 * @return ph_SUCCESS on success, or an error code.
 */
phStatus config_manager_get_clusters(char*** cluster_list, int* count);

/**
 * @brief Frees the memory allocated by config_manager_get_clusters.
 *
 * @param cluster_list The list of cluster names to free.
 * @param count The number of elements in the list.
 */
void config_manager_free_cluster_list(char** cluster_list, int count);

/**
 * @brief Sets the active Kubernetes cluster for subsequent commands.
 *
 * This function stores the name of the active cluster in an internal static variable.
 *
 * @param cluster_name The name of the cluster to be set as active.
 * @return ph_SUCCESS if the cluster exists in the configuration, otherwise an error.
 */
phStatus config_manager_set_current_cluster(const char* cluster_name);

/**
 * @brief Gets the name of the currently active Kubernetes cluster.
 *
 * @return A constant string with the name of the active cluster, or NULL if no
 * cluster has been set. The string is managed internally and should not be freed.
 */
const char* config_manager_get_current_cluster(void);

#ifdef __cplusplus
} // extern "C"
#endif

#endif // CONFIG_MANAGER_H