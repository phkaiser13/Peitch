/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * lua_bridge.h - Interface for the Lua scripting engine bridge.
 *
 * This header defines the public API for interacting with the embedded
 * Lua scripting engine. It provides functions to initialize the engine,
 * load and execute scripts, register custom commands, run lifecycle hooks,
 * and shut down the Lua virtual machine gracefully.
 *
 * SPDX-License-Identifier: Apache-2.0 */

#ifndef LUA_BRIDGE_H
#define LUA_BRIDGE_H

#include "../../ipc/include/ph_core_api.h" // For phStatus
#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @brief Initializes the Lua scripting engine.
 *
 * Creates the Lua state, loads standard libraries, exposes the 'ph'
 * C API, and loads all *.lua scripts from the 'plugins' directory.
 *
 * @return ph_SUCCESS on success, or an error code on failure.
 */
phStatus lua_bridge_init(void);

/**
 * @brief Executes a command previously registered by a Lua script.
 *
 * @param command_name The name of the command to execute.
 * @param argc The number of arguments.
 * @param argv The argument vector.
 * @return ph_SUCCESS if the Lua function returns a truthy value,
 * ph_ERROR_EXEC_FAILED otherwise.
 */
phStatus lua_bridge_execute_command(const char* command_name, int argc, const char** argv);

/**
 * @brief Runs all Lua functions registered for a specific lifecycle hook.
 *
 * @param hook_name The name of the hook to run (e.g., "pre-commit").
 * @param argc The number of arguments to pass to the hook functions.
 * @param argv The argument vector.
 * @return ph_SUCCESS if all hook functions execute successfully,
 * ph_ERROR_EXEC_FAILED if any of them fails.
 */
phStatus lua_bridge_run_hook(const char* hook_name, int argc, const char** argv);

/**
 * @brief Checks if a command is registered by the Lua bridge.
 *
 * @param command_name The command to check.
 * @return true if the command is a registered Lua command, false otherwise.
 */
bool lua_bridge_has_command(const char* command_name);

/**
 * @brief Gets the total number of commands registered via Lua.
 * @return The count of registered Lua commands.
 */
size_t lua_bridge_get_command_count(void);

/**
 * @brief Gets the description for a specific Lua-registered command.
 * @param command_name The name of the command.
 * @return A read-only string with the description, or NULL if not found.
 */
const char* lua_bridge_get_command_description(const char* command_name);

/**
 * @brief (Assumed needed by tui.c) Gets a list of all Lua-registered command names.
 * @return A NULL-terminated array of command name strings. The caller should
 * NOT modify this memory but should free it with a corresponding
 * lua_bridge_free_command_names function if it's dynamically allocated.
 */
const char** lua_bridge_get_all_command_names(void);

/**
 * @brief Libera a lista de nomes de comandos retornada por lua_bridge_get_all_command_names.
 * @param names_list A lista a ser liberada.
 */
void lua_bridge_free_command_names_list(const char** names_list);


/**
 * @brief Shuts down the Lua engine and frees all associated resources.
 *
 * Closes the Lua state and cleans up memory used by the command and hook registries.
 */
void lua_bridge_cleanup(void);

#ifdef __cplusplus
} // extern "C"
#endif

#endif // LUA_BRIDGE_H