/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * main.c - Main entry point for the ph application.
 *
 * This file contains the main function, which serves as the orchestrator for
 * the entire application lifecycle. Its primary responsibilities include:
 *
 * 1. Initializing core subsystems in the correct order: platform-specific
 * code, configuration management, logging, scripting, and the dynamic
 * module loader.
 * 2. Parsing command-line arguments to determine the execution mode. It handles
 * immediate flags like --help and --version before dispatching to the main
 * command handler.
 * 3. Launching either the interactive Text-based User Interface (TUI) or the
 * Command-Line Interface (CLI) handler based on the provided arguments.
 * 4. Ensuring a graceful shutdown by cleaning up all initialized subsystems
 * before the program terminates, regardless of the execution path.
 *
 * This file is intentionally kept high-level to provide a clear overview of
 * the application's control flow, delegating specialized logic to their
 * respective modules.
 *
 * SPDX-License-Identifier: Apache-2.0 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// Core module headers
#include "platform/platform.h"
#include "config/config_manager.h"
#include "ui/tui.h"
#include "cli/cli_parser.h"
#include "module_loader/loader.h"
#include "scripting/lua-h/lua_bridge.h"

// Shared library headers
#include "libs/liblogger/Logger.hpp"

// FFI declaration for the Rust-based telemetry initializer.
extern void setup_telemetry(void);

// Define application constants to avoid hardcoded strings
#define ph_VERSION "0.1.0-dev"
#define ph_CONFIG_FILE "ph.conf"
#define ph_LOG_FILE "ph_log.txt"
#define ph_MODULES_DIR "./modules"

// Forward declarations for static functions
static void print_version(void);
static void print_help(const char* app_name);
static int initialize_subsystems(void);
static void cleanup_subsystems(void);
static int process_arguments(int argc, char* argv[]);

/**
 * @brief The main entry point of the ph application.
 *
 * Orchestrates the application's lifecycle: initialization, execution,
 * and cleanup. It sets up all necessary subsystems, processes command-line
 * arguments to decide the execution mode (TUI or CLI), and ensures a
 * graceful shutdown.
 *
 * @param argc The number of command-line arguments.
 * @param argv An array of command-line argument strings.
 * @return EXIT_SUCCESS on successful execution, EXIT_FAILURE on critical error.
 */
int main(int argc, char* argv[]) {
    int exit_code = EXIT_SUCCESS;

    // 1. Initialize Core Subsystems
    if (initialize_subsystems() != 0) {
        // A critical subsystem failed to initialize. The logger might not be
        // available, so a printf fallback is used.
        fprintf(stderr, "A critical subsystem failed to initialize. Exiting.\n");
        exit_code = EXIT_FAILURE;
        goto cleanup; // Proceed to cleanup what might have been initialized.
    }

    // 2. Process Arguments and Determine Execution Mode
    exit_code = process_arguments(argc, argv);


    // 3. Graceful Shutdown
cleanup:
    logger_log(LOG_LEVEL_INFO, "MAIN", "ph application shutting down.");
    cleanup_subsystems();

    return exit_code;
}

/**
 * @brief Initializes all core application subsystems in the correct order.
 *
 * This function handles the setup of platform-specific features, logging,
 * configuration, scripting engine, and module loading.
 *
 * @return 0 on success, -1 on a critical failure.
 */
static int initialize_subsystems(void) {
    // Platform-specific setup (e.g., console encoding on Windows)
    platform_global_init();
    
    // Initialize OpenTelemetry tracing as early as possible.
    setup_telemetry();

    // Initialize the logging system first to make it available to other subsystems.
    logger_init(ph_LOG_FILE);
    logger_log(LOG_LEVEL_INFO, "MAIN", "ph application starting.");

    // Load configuration from file. This is not treated as a fatal error;
    // the application can run with default settings.
    if (config_load(ph_CONFIG_FILE) != 0) {
        logger_log(LOG_LEVEL_WARN, "MAIN", "Failed to load configuration. Using defaults.");
    }

    // Initialize the Lua scripting engine. This is critical for functionality.
    if (lua_bridge_init() != 0) {
        logger_log(LOG_LEVEL_FATAL, "MAIN", "Failed to initialize Lua scripting engine. Exiting.");
        return -1;
    }

    // Discover and load all external modules. This is critical.
    if (modules_load(ph_MODULES_DIR) != 0) {
        logger_log(LOG_LEVEL_FATAL, "MAIN", "Failed to load core modules. Exiting.");
        return -1;
    }
    logger_log(LOG_LEVEL_INFO, "MAIN", "All modules loaded successfully.");

    return 0; // Success
}

/**
 * @brief Processes command-line arguments and launches the appropriate mode.
 *
 * It checks for simple flags like --help and --version for immediate exit.
 * If no arguments are given, it starts the TUI. Otherwise, it dispatches
 * the command to the CLI handler.
 *
 * @param argc The number of command-line arguments.
 * @param argv An array of command-line argument strings.
 * @return EXIT_SUCCESS.
 */
static int process_arguments(int argc, char* argv[]) {
    if (argc < 2) {
        // No arguments provided: Enter interactive TUI mode
        logger_log(LOG_LEVEL_INFO, "MAIN", "No arguments detected. Starting interactive TUI mode.");
        tui_show_main_menu();
    } else {
        // Arguments provided: handle basic flags or dispatch to CLI
        if (strcmp(argv[1], "--help") == 0 || strcmp(argv[1], "-h") == 0) {
            print_help(argv[0]);
        } else if (strcmp(argv[1], "--version") == 0) {
            print_version();
        } else {
            logger_log(LOG_LEVEL_INFO, "MAIN", "Command detected. Dispatching to CLI handler.");
            cli_dispatch_command(argc, (const char**)argv);
        }
    }
    return EXIT_SUCCESS;
}

/**
 * @brief Cleans up and terminates all initialized subsystems.
 *
 * This function is called during graceful shutdown to release resources,
 * ensuring no memory leaks or dangling file handlers. The order of cleanup
 * is the reverse of initialization.
 */
static void cleanup_subsystems(void) {
    modules_cleanup();
    lua_bridge_cleanup();
    config_cleanup();
    logger_cleanup();
    platform_global_cleanup();
}


/**
 * @brief Prints the application's version information and exits.
 */
static void print_version(void) {
    printf("ph version %s\n", ph_VERSION);
    printf("Copyright (C) 2025 Pedro Henrique / phkaiser13\n");
    printf("License: Apache-2.0\n");
}

/**
 * @brief Prints the main help message with basic usage and exits.
 *
 * Detailed help for specific commands should be handled by the modules
 * themselves or the CLI parser.
 *
 * @param app_name The name of the executable (argv[0]).
 */
static void print_help(const char* app_name) {
    printf("Usage: %s [command] [options]\n\n", app_name);
    printf("A modern, polyglot Git helper to streamline your workflow.\n\n");
    printf("Run '%s' without arguments to enter the interactive menu.\n\n", app_name);
    printf("Core Commands:\n");
    printf("  --help, -h  Show this help message and exit.\n");
    printf("  --version   Show version information and exit.\n\n");
    printf("For a full list of commands, run the interactive TUI or consult the documentation.\n");
}