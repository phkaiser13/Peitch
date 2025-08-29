/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* phpkg.c
* This file serves as the main entry point for the 'phpkg' module and acts as
* the Foreign Function Interface (FFI) bridge to the main application core.
* Its primary responsibilities are to register the module with the core,
* define the commands it handles (e.g., "pk"), parse command-line arguments
* passed from the core, and delegate the execution to the appropriate
* backend functions, such as the package installer.
* SPDX-License-Identifier: Apache-2.0 */

#include "installer.h" // Our main logic
#include <stdio.h>
#include <string.h>

// This would be the API header provided by the main application core.
// It defines the structures and function prototypes that modules must implement.
// For this standalone example, we'll define a minimal version of it.
// --- Start: Mock ph_core_api.h ---
typedef enum {
    ph_STATUS_SUCCESS = 0,
    ph_STATUS_ERROR = 1,
    ph_STATUS_INVALID_ARGS = 2
} ph_STATUS;

typedef struct {
    const char* command_name;
    const char* description;
} ModuleCommand;

typedef struct {
    const char* module_name;
    const char* description;
    const ModuleCommand* commands;
    int num_commands;
} ModuleInfo;
// --- End: Mock ph_core_api.h ---


// --- Module Metadata ---

// Define the commands this module provides.
static const ModuleCommand g_module_commands[] = {
    { "pk", "The Peitch package manager for installing tools and dependencies." }
};

// Define the main information structure for this module.
static const ModuleInfo g_module_info = {
    .module_name = "phpkg",
    .description = "A built-in package manager to download and install development tools.",
    .commands = g_module_commands,
    .num_commands = sizeof(g_module_commands) / sizeof(g_module_commands[0])
};


// --- FFI Implementation ---

/**
 * @brief Provides the core with metadata about this module.
 *
 * This function is called by the core during the module loading phase.
 * It returns a pointer to a static struct containing the module's name,
 * description, and the commands it handles.
 *
 * @return A constant pointer to the ModuleInfo struct.
 */
const ModuleInfo* module_get_info(void) {
    return &g_module_info;
}

/**
 * @brief Initializes the module.
 *
 * Called by the core after loading the module. Can be used for any
 * one-time setup. For phpkg, no specific initialization is needed.
 *
 * @return ph_STATUS_SUCCESS on success.
 */
ph_STATUS module_init(void) {
    // No specific initialization needed for this module.
    return ph_STATUS_SUCCESS;
}

/**
 * @brief Cleans up the module's resources.
 *
 * Called by the core before unloading the module.
 */
void module_cleanup(void) {
    // No specific cleanup needed for this module.
}

/**
 * @brief Executes a command handled by this module.
 *
 * This is the main entry point for command execution. It receives arguments
 * from the core, parses them, and calls the appropriate backend logic.
 *
 * Expected command structure:
 * argv[0]: "pk"
 * argv[1]: "install"
 * argv[2]: <package_name>
 * argv[3]: [--v<version> | (optional)]
 *
 * @param argc The number of arguments.
 * @param argv An array of argument strings.
 * @return A ph_STATUS code indicating the outcome.
 */
ph_STATUS module_exec(int argc, const char* argv[]) {
    // Basic argument validation
    if (argc < 3) {
        fprintf(stderr, "Error: Not enough arguments for 'pk' command.\n");
        fprintf(stderr, "Usage: ph pk install <package_name> [--v<version>]\n");
        return ph_STATUS_INVALID_ARGS;
    }

    // The first argument (argv[0]) is always the command name, "pk".
    // We check the subcommand, which should be "install".
    const char* subcommand = argv[1];
    if (strcmp(subcommand, "install") != 0) {
        fprintf(stderr, "Error: Unknown subcommand '%s' for 'pk'.\n", subcommand);
        fprintf(stderr, "Usage: ph pk install <package_name> [--v<version>]\n");
        return ph_STATUS_INVALID_ARGS;
    }

    const char* package_name = argv[2];
    const char* version_string = "latest"; // Default to latest

    // Check for an optional version argument
    if (argc > 3) {
        // Simple version parsing: expecting --v1.2.3 or similar
        if (strncmp(argv[3], "--v", 3) == 0) {
            version_string = argv[3] + 3; // Point past the "--v"
        } else {
            fprintf(stderr, "Error: Invalid version format. Expected '--v<version>'.\n");
            return ph_STATUS_INVALID_ARGS;
        }
    }

    // Delegate to the installer logic
    InstallStatus status = install_package(package_name, version_string);

    // Translate the detailed installer status to a generic core status
    switch (status) {
        case INSTALL_SUCCESS:
        case INSTALL_DELEGATED_TO_SYSTEM:
            return ph_STATUS_SUCCESS;
        case INSTALL_ERROR_PACKAGE_NOT_FOUND:
        case INSTALL_ERROR_UNSUPPORTED_PLATFORM:
        case INSTALL_ERROR_VERSION_RESOLUTION:
            return ph_STATUS_INVALID_ARGS;
        default:
            return ph_STATUS_ERROR;
    }
}