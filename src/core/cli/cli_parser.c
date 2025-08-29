/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* cli_parser.c - Implementation of the CLI Parser and Command Dispatcher.
*
* This file implements the core logic for routing command-line interface (CLI)
* commands to their respective handlers. It acts as a powerful, multi-stage
* dispatcher that decouples command invocation from execution, supporting
* specialized command groups, dynamically registered Lua scripts, and native
* compiled modules.
*
* The `cli_dispatch_command` function is the single point of entry. Its logic is:
* 1. First, check for special "command groups" (e.g., 'kube', 'local', 'runners').
*    If a match is found, dispatch is delegated to a dedicated sub-handler
*    responsible for that group's subcommands. This allows for structured and
*    modular command suites (e.g., `ph runners list`).
* 2. If the command is not part of a group, query the Lua bridge to see if it is
*    a user-defined script. If found, execution is delegated to the Lua engine.
* 3. If not found in the Lua bridge, it falls back to querying the native
*    module loader for a compiled C handler.
* 4. If no handler is found in any system, an "unknown command" error is reported.
*
* This design makes the CLI extremely extensible, allowing new functionality to be
* added via compiled modules, simple Lua scripts, or entire command suites
* without modifying the core dispatcher.
*
* SPDX-License-Identifier: Apache-2.0 */

#include "cli_parser.h"
#include "module_loader/loader.h"
#include "tui/tui.h"
#include "libs/liblogger/Logger.hpp"
#include "scripting/lua-h/lua_bridge.h"
#include "commands/kube_handler.h"
#include "commands/local_handler.h"
#include "commands/runners_handler.h"
#include "commands/secrets_handler.h"
#include "commands/policy_handler.h"
#include "commands/health_handler.h"
#include "commands/preview_handler.h"
#include <stdio.h>
#include <string.h>

/**
 * @see cli_parser.h
 */
phStatus cli_dispatch_command(int argc, const char** argv) {
    // Basic validation: we need at least the application name and a command.
    if (argc < 2 || argv[1] == NULL || strlen(argv[1]) == 0) {
        tui_print_error("No command provided. Use --help for usage information.");
        logger_log(LOG_LEVEL_WARN, "CLI", "Dispatch called with no command.");
        return ph_ERROR_INVALID_ARGS;
    }

    const char* command = argv[1];
    logger_log_fmt(LOG_LEVEL_INFO, "CLI", "Attempting to dispatch command: '%s'", command);

    // --- STAGE 1: Check for special command groups ---
    // This is the first layer of dispatch, handling command suites.
    if (strcmp(command, "kube") == 0) {
        logger_log(LOG_LEVEL_INFO, "CLI", "Command group 'kube' identified. Delegating to kube_handler.");
        if (argc < 3) {
            tui_print_error("The 'kube' command requires a subcommand.");
            return ph_ERROR_INVALID_ARGS;
        }
        return handle_kube_command(argc - 2, &argv[2]);

    } else if (strcmp(command, "local") == 0) {
        logger_log(LOG_LEVEL_INFO, "CLI", "Command group 'local' identified. Delegating to local_handler.");
        if (argc < 3) {
            tui_print_error("The 'local' command requires a subcommand.");
            return ph_ERROR_INVALID_ARGS;
        }
        return handle_local_command(argc - 2, &argv[2]);

    } else if (strcmp(command, "runners") == 0) {
        logger_log(LOG_LEVEL_INFO, "CLI", "Command group 'runners' identified. Delegating to runners_handler.");
        if (argc < 3) {
            tui_print_error("The 'runners' command requires a subcommand.");
            return ph_ERROR_INVALID_ARGS;
        }
        return handle_runners_command(argc - 2, &argv[2]);

    } else if (strcmp(command, "secrets") == 0) {
        logger_log(LOG_LEVEL_INFO, "CLI", "Command group 'secrets' identified. Delegating to secrets_handler.");
        if (argc < 3) {
            tui_print_error("The 'secrets' command requires a subcommand.");
            return ph_ERROR_INVALID_ARGS;
        }
        return handle_secrets_command(argc - 2, &argv[2]);

    } else if (strcmp(command, "policy") == 0) {
        logger_log(LOG_LEVEL_INFO, "CLI", "Command group 'policy' identified. Delegating to policy_handler.");
        if (argc < 3) {
            tui_print_error("The 'policy' command requires a subcommand.");
            return ph_ERROR_INVALID_ARGS;
        }
        return handle_policy_command(argc - 2, &argv[2]);

    } else if (strcmp(command, "health") == 0) {
        logger_log(LOG_LEVEL_INFO, "CLI", "Command group 'health' identified. Delegating to health_handler.");
        if (argc < 3) {
            tui_print_error("The 'health' command requires a subcommand.");
            return ph_ERROR_INVALID_ARGS;
        }
        return handle_health_command(argc - 2, &argv[2]);

    } else if (strcmp(command, "autoheal") == 0) {
        logger_log(LOG_LEVEL_INFO, "CLI", "Command group 'autoheal' identified. Delegating to health_handler.");
        if (argc < 3) {
            tui_print_error("The 'autoheal' command requires a subcommand.");
            return ph_ERROR_INVALID_ARGS;
        }
        // The same handler processes both 'health' and 'autoheal' commands.
        return handle_health_command(argc - 2, &argv[2]);

    } else if (strcmp(command, "preview") == 0) {
        logger_log(LOG_LEVEL_INFO, "CLI", "Command group 'preview' identified. Delegating to preview_handler.");
        if (argc < 3) {
            tui_print_error("The 'preview' command requires a subcommand.");
            return ph_ERROR_INVALID_ARGS;
        }
        return handle_preview_command(argc - 2, &argv[2]);
    }

    // If the command is not a special group, proceed to the next stages.

    // --- STAGE 2: Check the Lua Bridge for a registered command ---
    if (lua_bridge_has_command(command)) {
        logger_log_fmt(LOG_LEVEL_INFO, "CLI", "Command '%s' is a registered Lua command. Dispatching to bridge.", command);
        phStatus status = lua_bridge_execute_command(command, argc - 1, &argv[1]);
        if (status != ph_SUCCESS) {
            logger_log_fmt(LOG_LEVEL_ERROR, "CLI", "Execution of Lua command '%s' failed with status code %d.", command, status);
            tui_print_error("The scripted command failed to execute successfully.");
        } else {
            logger_log_fmt(LOG_LEVEL_INFO, "CLI", "Lua command '%s' executed successfully.", command);
        }   
        return status;
    }

    // --- STAGE 3: Fallback to native C modules ---
    logger_log_fmt(LOG_LEVEL_DEBUG, "CLI", "Command '%s' not found in groups or Lua bridge. Checking native modules.", command);
    const LoadedModule* handler_module = modules_find_handler(command);

    if (handler_module) {
        logger_log_fmt(LOG_LEVEL_INFO, "CLI", "Found native handler for '%s' in module '%s'. Executing...", command, handler_module->info.name);
        phStatus status = handler_module->exec_func(argc - 1, &argv[1]);
        if (status != ph_SUCCESS) {
            logger_log_fmt(LOG_LEVEL_ERROR, "CLI", "Execution of native command '%s' failed with status code %d.", command, status);
            tui_print_error("The command failed to execute successfully.");
        } else {
            logger_log_fmt(LOG_LEVEL_INFO, "CLI", "Native command '%s' executed successfully.", command);
        }
        return status;
    }

    // --- STAGE 4: Command not found in any system ---
    char error_msg[128];
    snprintf(error_msg, sizeof(error_msg), "Unknown command: '%s'", command);
    tui_print_error(error_msg);

    logger_log_fmt(LOG_LEVEL_WARN, "CLI", "No handler found for command: '%s'", command);
    return ph_ERROR_NOT_FOUND;
}