/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * loader.h - Interface for the dynamic module loader.
 *
 * This header defines the API for the module loading subsystem. This subsystem
 * is responsible for discovering, loading, and managing the lifecycle of all
 * external, dynamically-linked modules (.so, .dll). It acts as the central
 * registry for all functionality not implemented directly in the C core.
 *
 * The loader maintains an internal list of valid modules that have been
 * successfully loaded and initialized. It provides functions to find which
 * module is responsible for handling a specific command, which is essential
 * for the CLI dispatcher and the TUI.
 *
 * SPDX-License-Identifier: Apache-2.0 */

#ifndef LOADER_H
#define LOADER_H

#include "../../ipc/include/ph_core_api.h" // For phStatus and module function pointers

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @struct LoadedModule
 * @brief Represents a module that has been successfully loaded into memory.
 *
 * This structure holds all necessary information about a loaded module,
 * including its metadata, a handle to the dynamic library, and pointers to its
 * core functions as defined by the ph_core_api.h contract.
 */
typedef struct {
    void* handle;                   // Opaque handle to the loaded library (from dlopen/LoadLibrary)
    char* file_path;                // The full path to the module's file
    phModuleInfo info;           // A copy of the module's metadata
    PFN_module_init init_func;      // Pointer to the module's init function
    PFN_module_exec exec_func;      // Pointer to the module's exec function
    PFN_module_cleanup cleanup_func;// Pointer to the module's cleanup function
} LoadedModule;

/**
 * @brief Scans a directory, loads all valid modules, and initializes them.
 *
 * This is the main entry point for the loader. It iterates through files in the
 * specified directory, attempting to load any with the correct shared library
 * extension (.so/.dll). For each successfully loaded library, it resolves the
 * required API functions (`module_get_info`, `module_init`, etc.), calls the
 * module's init function, and stores it in an internal registry.
 *
 * @param directory_path The path to the directory containing the modules.
 * @return ph_SUCCESS on success, or an error code if a critical failure
 *         (like being unable to read the directory) occurs.
 */
phStatus modules_load(const char* directory_path);

/**
 * @brief Finds the module responsible for handling a given command.
 *
 * This function searches the internal registry of loaded modules to find one
 * that has registered the specified command in its metadata.
 *
 * @param command The command string to search for (e.g., "SND", "rls").
 * @return A read-only pointer to the `LoadedModule` struct for the handler,
 *         or NULL if no module handles the command.
 */
const LoadedModule* modules_find_handler(const char* command);

/**
 * @brief Retrieves a list of all successfully loaded modules.
 *
 * This is useful for UI components that need to display information about
 * what functionality is available.
 *
 * @param[out] count A pointer to an integer that will be filled with the number
 *                   of loaded modules.
 * @return A read-only, NULL-terminated array of pointers to `LoadedModule`
 *         structs. The memory is owned by the loader and is valid until
 *         `modules_cleanup` is called.
 */
const LoadedModule** modules_get_all(int* count);

/**
 * @brief Unloads all modules and frees associated resources.
 *
 * This function iterates through all loaded modules, calls their respective
 * `module_cleanup` function, and then unloads the shared library from memory.
 * It must be called at application shutdown to ensure a clean exit.
 */
void modules_cleanup(void);


#ifdef __cplusplus
} // extern "C"
#endif

#endif // LOADER_H