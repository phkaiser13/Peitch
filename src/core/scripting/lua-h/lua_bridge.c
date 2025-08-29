/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * lua_bridge.c - Enhanced implementation of the Lua scripting engine bridge.
 *
 * This file implements an advanced bridge between the C core and the Lua scripting
 * engine. It manages the lifecycle of the Lua VM, exposes a comprehensive set of
 * safe C functions to the Lua environment, and enables dynamic command registration.
 * This enhanced version provides full access to configuration management, allows
 * plugins to register custom commands with complex logic, and implements a robust
 * hook system for extensibility.
 *
 * The core of the bridge is a global table named `ph` that is injected
 * into the Lua state. This table contains functions that scripts can call,
 * including configuration access, command registration, and advanced Git operations.
 * This provides maximum extensibility while maintaining security and performance.
 *
 * SPDX-License-Identifier: Apache-2.0 */

#include "lua_bridge.h"
#include "libs/liblogger/Logger.hpp"
#include "platform/platform.h"
#include "cli/cli_parser.h"
#include "core/config/config_manager.h"
#include <string.h>
#include <stdlib.h>

// Lua headers
#include <lua.h>
#include <lauxlib.h>
#include <lualib.h>

// Platform-specific includes for directory traversal
#ifdef PLATFORM_WINDOWS
#include <windows.h>
#else
#include <dirent.h>
#endif

// --- Module-level static variables ---

// The single, global Lua state for the entire application.
static lua_State* g_lua_state = NULL;

// Dynamic command registry for Lua-registered commands
typedef struct {
    char* command_name;
    char* lua_function_name;
    char* description;
    char* usage;
} lua_command_entry_t;

static lua_command_entry_t* g_lua_commands = NULL;
static size_t g_lua_command_count = 0;
static size_t g_lua_command_capacity = 0;

// Hook registry for event-driven plugin execution
typedef struct {
    char* hook_name;
    char** function_names;
    size_t function_count;
    size_t function_capacity;
} lua_hook_registry_t;

static lua_hook_registry_t* g_hook_registry = NULL;
static size_t g_hook_count = 0;
static size_t g_hook_capacity = 0;

// --- Internal Helper Functions ---

/**
 * @brief Safely grows a dynamic array with exponential growth strategy.
 *
 * @param ptr Pointer to the array pointer
 * @param current_size Current number of elements
 * @param current_capacity Current capacity of the array
 * @param element_size Size of each element
 * @param min_capacity Minimum required capacity
 * @return true on success, false on allocation failure
 */
static bool grow_array(void** ptr, size_t current_size, size_t* current_capacity, 
                      size_t element_size, size_t min_capacity) {
    if (*current_capacity >= min_capacity) return true;
    
    size_t new_capacity = *current_capacity == 0 ? 8 : *current_capacity;
    while (new_capacity < min_capacity) {
        new_capacity *= 2;
    }
    
    void* new_ptr = realloc(*ptr, new_capacity * element_size);
    if (!new_ptr) {
        logger_log(LOG_LEVEL_ERROR, "LUA_BRIDGE", "Memory allocation failed during array growth");
        return false;
    }
    
    *ptr = new_ptr;
    *current_capacity = new_capacity;
    return true;
}

/**
 * @brief Finds a Lua command by name in the registry.
 *
 * @param command_name The command name to search for
 * @return Pointer to the command entry, or NULL if not found
 */
static lua_command_entry_t* find_lua_command(const char* command_name) {
    for (size_t i = 0; i < g_lua_command_count; i++) {
        if (strcmp(g_lua_commands[i].command_name, command_name) == 0) {
            return &g_lua_commands[i];
        }
    }
    return NULL;
}

/**
 * @brief Finds or creates a hook registry entry.
 *
 * @param hook_name The name of the hook
 * @return Pointer to the hook registry entry, or NULL on failure
 */
static lua_hook_registry_t* find_or_create_hook(const char* hook_name) {
    // First, try to find existing hook
    for (size_t i = 0; i < g_hook_count; i++) {
        if (strcmp(g_hook_registry[i].hook_name, hook_name) == 0) {
            return &g_hook_registry[i];
        }
    }
    
    // Create new hook entry
    if (!grow_array((void**)&g_hook_registry, g_hook_count, &g_hook_capacity, 
                   sizeof(lua_hook_registry_t), g_hook_count + 1)) {
        return NULL;
    }
    
    lua_hook_registry_t* hook = &g_hook_registry[g_hook_count++];
    hook->hook_name = strdup(hook_name);
    hook->function_names = NULL;
    hook->function_count = 0;
    hook->function_capacity = 0;
    
    return hook;
}

// --- Enhanced C Functions Exposed to Lua ---

/**
 * @brief Enhanced Lua binding for the core logging function.
 *
 * Exposes `logger_log` to Lua scripts with additional formatting support.
 * Lua usage: `ph.log("INFO", "My message from Lua", optional_context)`
 *
 * @param L The Lua state.
 * @return The number of return values pushed onto the stack (0).
 */
static int l_ph_log(lua_State* L) {
    int n_args = lua_gettop(L);
    if (n_args < 2 || n_args > 3) {
        return luaL_error(L, "ph.log expects 2-3 arguments: level (string), message (string), [context (string)]");
    }

    const char* level_str = luaL_checkstring(L, 1);
    const char* message = luaL_checkstring(L, 2);
    const char* context = n_args >= 3 ? luaL_checkstring(L, 3) : "LUA_PLUGIN";

    phLogLevel level = LOG_LEVEL_INFO; // Default
    if (strcmp(level_str, "DEBUG") == 0) level = LOG_LEVEL_DEBUG;
    else if (strcmp(level_str, "WARN") == 0) level = LOG_LEVEL_WARN;
    else if (strcmp(level_str, "ERROR") == 0) level = LOG_LEVEL_ERROR;
    else if (strcmp(level_str, "FATAL") == 0) level = LOG_LEVEL_FATAL;

    logger_log(level, context, message);
    return 0; // No return values
}

/**
 * @brief Enhanced Lua binding for the core command dispatcher.
 *
 * Exposes `cli_dispatch_command` to Lua scripts with argument support.
 * Lua usage: `ph.run_command("status", {"--porcelain", "-v"})`
 *
 * @param L The Lua state.
 * @return The number of return values pushed onto the stack (1 - success boolean).
 */
static int l_ph_run_command(lua_State* L) {
    int n_args = lua_gettop(L);
    if (n_args < 1 || n_args > 2) {
        return luaL_error(L, "ph.run_command expects 1-2 arguments: command (string), [args (table)]");
    }
    
    const char* command_str = luaL_checkstring(L, 1);
    
    // Build argc/argv from Lua arguments
    const char** argv = NULL;
    int argc = 2; // "ph", command
    
    if (n_args == 2) {
        luaL_checktype(L, 2, LUA_TTABLE);
        
        // Count arguments in table
        size_t arg_count = lua_rawlen(L, 2);
        argc = (int)(2 + arg_count);
        
        argv = malloc(sizeof(char*) * (argc + 1));
        if (!argv) {
            lua_pushboolean(L, 0);
            return 1;
        }
        
        argv[0] = "ph";
        argv[1] = command_str;
        
        // Extract arguments from table
        for (size_t i = 0; i < arg_count; i++) {
            lua_rawgeti(L, 2, (int)(i + 1));
            argv[2 + i] = lua_tostring(L, -1);
            lua_pop(L, 1);
        }
        argv[argc] = NULL;
    } else {
        argv = malloc(sizeof(char*) * 3);
        if (!argv) {
            lua_pushboolean(L, 0);
            return 1;
        }
        argv[0] = "ph";
        argv[1] = command_str;
        argv[2] = NULL;
    }
    
    phStatus result = cli_dispatch_command(argc, argv);
    free(argv);
    
    lua_pushboolean(L, result == ph_SUCCESS ? 1 : 0);
    return 1;
}

/**
 * @brief Lua binding for configuration value retrieval.
 *
 * Exposes `config_get_value` to Lua scripts.
 * Lua usage: `local value = ph.config_get("user.name")`
 *
 * @param L The Lua state.
 * @return The number of return values pushed onto the stack (1 - value or nil).
 */
static int l_ph_config_get(lua_State* L) {
    const char* key = luaL_checkstring(L, 1);
    
    char* value = config_get_value(key);
    if (value) {
        lua_pushstring(L, value);
        free(value);
    } else {
        lua_pushnil(L);
    }
    
    return 1;
}

/**
 * @brief Lua binding for configuration value setting.
 *
 * Exposes `config_set_value` to Lua scripts.
 * Lua usage: `ph.config_set("user.name", "John Doe")`
 *
 * @param L The Lua state.
 * @return The number of return values pushed onto the stack (1 - success boolean).
 */
static int l_ph_config_set(lua_State* L) {
    const char* key = luaL_checkstring(L, 1);
    const char* value = luaL_checkstring(L, 2);
    
    phStatus result = config_set_value(key, value);
    lua_pushboolean(L, result == ph_SUCCESS ? 1 : 0);
    
    return 1;
}

/**
 * @brief Lua binding for dynamic command registration.
 *
 * Allows Lua scripts to register new commands with the CLI parser.
 * Lua usage: `ph.register_command("mycommand", "my_lua_function", "Description", "Usage")`
 *
 * @param L The Lua state.
 * @return The number of return values pushed onto the stack (1 - success boolean).
 */
static int l_ph_register_command(lua_State* L) {
    int n_args = lua_gettop(L);
    if (n_args < 2 || n_args > 4) {
        return luaL_error(L, "ph.register_command expects 2-4 arguments: command (string), function (string), [description (string)], [usage (string)]");
    }
    
    const char* command_name = luaL_checkstring(L, 1);
    const char* lua_function = luaL_checkstring(L, 2);
    const char* description = n_args >= 3 ? luaL_checkstring(L, 3) : "User-defined command";
    const char* usage = n_args >= 4 ? luaL_checkstring(L, 4) : command_name;
    
    // Check if command already exists
    if (find_lua_command(command_name)) {
        logger_log_fmt(LOG_LEVEL_WARN, "LUA_BRIDGE", "Command '%s' already registered, ignoring duplicate", command_name);
        lua_pushboolean(L, 0);
        return 1;
    }
    
    // Verify the Lua function exists
    lua_getglobal(L, lua_function);
    if (!lua_isfunction(L, -1)) {
        lua_pop(L, 1);
        logger_log_fmt(LOG_LEVEL_ERROR, "LUA_BRIDGE", "Lua function '%s' not found for command '%s'", lua_function, command_name);
        lua_pushboolean(L, 0);
        return 1;
    }
    lua_pop(L, 1);
    
    // Grow command array if needed
    if (!grow_array((void**)&g_lua_commands, g_lua_command_count, &g_lua_command_capacity, 
                   sizeof(lua_command_entry_t), g_lua_command_count + 1)) {
        lua_pushboolean(L, 0);
        return 1;
    }
    
    // Register the command
    lua_command_entry_t* entry = &g_lua_commands[g_lua_command_count++];
    entry->command_name = strdup(command_name);
    entry->lua_function_name = strdup(lua_function);
    entry->description = strdup(description);
    entry->usage = strdup(usage);
    
    logger_log_fmt(LOG_LEVEL_INFO, "LUA_BRIDGE", "Registered Lua command '%s' -> '%s'", command_name, lua_function);
    lua_pushboolean(L, 1);
    return 1;
}

/**
 * @brief Lua binding for hook registration.
 *
 * Allows Lua scripts to register functions to be called on specific hooks.
 * Lua usage: `ph.register_hook("pre-commit", "my_pre_commit_function")`
 *
 * @param L The Lua state.
 * @return The number of return values pushed onto the stack (1 - success boolean).
 */
static int l_ph_register_hook(lua_State* L) {
    const char* hook_name = luaL_checkstring(L, 1);
    const char* function_name = luaL_checkstring(L, 2);
    
    // Verify the Lua function exists
    lua_getglobal(L, function_name);
    if (!lua_isfunction(L, -1)) {
        lua_pop(L, 1);
        logger_log_fmt(LOG_LEVEL_ERROR, "LUA_BRIDGE", "Lua function '%s' not found for hook '%s'", function_name, hook_name);
        lua_pushboolean(L, 0);
        return 1;
    }
    lua_pop(L, 1);
    
    // Find or create hook registry
    lua_hook_registry_t* hook = find_or_create_hook(hook_name);
    if (!hook) {
        lua_pushboolean(L, 0);
        return 1;
    }
    
    // Grow function array if needed
    if (!grow_array((void**)&hook->function_names, hook->function_count, &hook->function_capacity,
                   sizeof(char*), hook->function_count + 1)) {
        lua_pushboolean(L, 0);
        return 1;
    }
    
    // Register the function
    hook->function_names[hook->function_count++] = strdup(function_name);
    
    logger_log_fmt(LOG_LEVEL_DEBUG, "LUA_BRIDGE", "Registered function '%s' for hook '%s'", function_name, hook_name);
    lua_pushboolean(L, 1);
    return 1;
}

/**
 * @brief Lua binding for file existence checking.
 *
 * Lua usage: `local exists = ph.file_exists("/path/to/file")`
 *
 * @param L The Lua state.
 * @return The number of return values pushed onto the stack (1 - boolean).
 */
static int l_ph_file_exists(lua_State* L) {
    const char* path = luaL_checkstring(L, 1);
    
    FILE* f = fopen(path, "r");
    if (f) {
        fclose(f);
        lua_pushboolean(L, 1);
    } else {
        lua_pushboolean(L, 0);
    }
    
    return 1;
}

/**
 * @brief Lua binding for environment variable access.
 *
 * Lua usage: `local value = ph.getenv("HOME")`
 *
 * @param L The Lua state.
 * @return The number of return values pushed onto the stack (1 - value or nil).
 */
static int l_ph_getenv(lua_State* L) {
    const char* name = luaL_checkstring(L, 1);
    
    const char* value = getenv(name);
    if (value) {
        lua_pushstring(L, value);
    } else {
        lua_pushnil(L);
    }
    
    return 1;
}

// Enhanced function registry with comprehensive API
static const struct luaL_Reg ph_lib[] = {
    // Core functionality
    {"log", l_ph_log},
    {"run_command", l_ph_run_command},
    
    // Configuration management
    {"config_get", l_ph_config_get},
    {"config_set", l_ph_config_set},
    
    // Dynamic registration
    {"register_command", l_ph_register_command},
    {"register_hook", l_ph_register_hook},
    
    // Utility functions
    {"file_exists", l_ph_file_exists},
    {"getenv", l_ph_getenv},
    
    {NULL, NULL} // Sentinel
};

// --- Public API Implementation ---

/**
 * @see lua_bridge.h
 */
phStatus lua_bridge_init(void) {
    if (g_lua_state) {
        logger_log(LOG_LEVEL_WARN, "LUA_BRIDGE", "Lua bridge already initialized.");
        return ph_SUCCESS;
    }

    // 1. Create Lua state and load standard libraries
    g_lua_state = luaL_newstate();
    if (!g_lua_state) {
        logger_log(LOG_LEVEL_FATAL, "LUA_BRIDGE", "Failed to create Lua state.");
        return ph_ERROR_INIT_FAILED;
    }
    luaL_openlibs(g_lua_state);

    // 2. Create the `ph` library table and register our enhanced C functions
    luaL_newlib(g_lua_state, ph_lib);
    lua_setglobal(g_lua_state, "ph");

    // 3. Add version information to the ph table
    lua_getglobal(g_lua_state, "ph");
    lua_pushstring(g_lua_state, "2.0.0");
    lua_setfield(g_lua_state, -2, "version");
    lua_pop(g_lua_state, 1);

    // 4. Scan and load all scripts from the "plugins" directory
    const char* plugin_dir = "plugins";

#ifdef PLATFORM_WINDOWS
    char search_path[MAX_PATH];
    snprintf(search_path, sizeof(search_path), "%s\\*.lua", plugin_dir);
    WIN32_FIND_DATA fd;
    HANDLE hFind = FindFirstFile(search_path, &fd);
    if (hFind != INVALID_HANDLE_VALUE) {
        do {
            char full_path[MAX_PATH];
            snprintf(full_path, sizeof(full_path), "%s\\%s", plugin_dir, fd.cFileName);
            if (luaL_dofile(g_lua_state, full_path) != LUA_OK) {
                logger_log_fmt(LOG_LEVEL_ERROR, "LUA_BRIDGE", "Failed to load plugin '%s': %s", 
                              full_path, lua_tostring(g_lua_state, -1));
                lua_pop(g_lua_state, 1); // Pop error message from stack
            } else {
                logger_log_fmt(LOG_LEVEL_INFO, "LUA_BRIDGE", "Loaded plugin: %s", fd.cFileName);
            }
        } while (FindNextFile(hFind, &fd) != 0);
        FindClose(hFind);
    }
#else // POSIX
    DIR* d = opendir(plugin_dir);
    if (d) {
        struct dirent* dir;
        while ((dir = readdir(d)) != NULL) {
            if (strstr(dir->d_name, ".lua")) {
                char full_path[1024];
                snprintf(full_path, sizeof(full_path), "%s/%s", plugin_dir, dir->d_name);
                if (luaL_dofile(g_lua_state, full_path) != LUA_OK) {
                    logger_log_fmt(LOG_LEVEL_ERROR, "LUA_BRIDGE", "Failed to load plugin '%s': %s", 
                                  full_path, lua_tostring(g_lua_state, -1));
                    lua_pop(g_lua_state, 1); // Pop error message
                } else {
                    logger_log_fmt(LOG_LEVEL_INFO, "LUA_BRIDGE", "Loaded plugin: %s", dir->d_name);
                }
            }
        }
        closedir(d);
    }
#endif

    logger_log_fmt(LOG_LEVEL_INFO, "LUA_BRIDGE", "Lua scripting engine initialized with %zu registered commands", 
                  g_lua_command_count);
    return ph_SUCCESS;
}

/**
 * @see lua_bridge.h
 */
phStatus lua_bridge_execute_command(const char* command_name, int argc, const char** argv) {
    if (!g_lua_state) return ph_ERROR_GENERAL;
    
    lua_command_entry_t* cmd = find_lua_command(command_name);
    if (!cmd) return ph_ERROR_NOT_FOUND;
    
    // Get the Lua function
    lua_getglobal(g_lua_state, cmd->lua_function_name);
    if (!lua_isfunction(g_lua_state, -1)) {
        lua_pop(g_lua_state, 1);
        logger_log_fmt(LOG_LEVEL_ERROR, "LUA_BRIDGE", "Command function '%s' is no longer valid", 
                      cmd->lua_function_name);
        return ph_ERROR_EXEC_FAILED;
    }
    
    // Push arguments onto the stack
    for (int i = 0; i < argc; ++i) {
        lua_pushstring(g_lua_state, argv[i]);
    }
    
    // Call the function with argc arguments and expect 1 return value (status)
    if (lua_pcall(g_lua_state, argc, 1, 0) != LUA_OK) {
        logger_log_fmt(LOG_LEVEL_ERROR, "LUA_BRIDGE", "Error executing command '%s': %s", 
                      command_name, lua_tostring(g_lua_state, -1));
        lua_pop(g_lua_state, 1); // Pop error message
        return ph_ERROR_EXEC_FAILED;
    }
    
    // Get return value (expected to be boolean or number indicating success)
    int success = 1;
    if (lua_isboolean(g_lua_state, -1)) {
        success = lua_toboolean(g_lua_state, -1);
    } else if (lua_isnumber(g_lua_state, -1)) {
        success = (lua_tointeger(g_lua_state, -1) == 0); // 0 = success in Unix convention
    }
    lua_pop(g_lua_state, 1); // Pop return value
    
    return success ? ph_SUCCESS : ph_ERROR_EXEC_FAILED;
}

/**
 * @see lua_bridge.h
 */
phStatus lua_bridge_run_hook(const char* hook_name, int argc, const char** argv) {
    if (!g_lua_state) return ph_ERROR_GENERAL;
    
    // Find the hook registry
    lua_hook_registry_t* hook = NULL;
    for (size_t i = 0; i < g_hook_count; i++) {
        if (strcmp(g_hook_registry[i].hook_name, hook_name) == 0) {
            hook = &g_hook_registry[i];
            break;
        }
    }
    
    if (!hook || hook->function_count == 0) {
        return ph_ERROR_NOT_FOUND; // No functions registered for this hook
    }
    
    // Execute all functions registered for this hook
    phStatus overall_result = ph_SUCCESS;
    for (size_t i = 0; i < hook->function_count; i++) {
        lua_getglobal(g_lua_state, hook->function_names[i]);
        
        if (!lua_isfunction(g_lua_state, -1)) {
            lua_pop(g_lua_state, 1);
            logger_log_fmt(LOG_LEVEL_WARN, "LUA_BRIDGE", "Hook function '%s' is no longer valid", 
                          hook->function_names[i]);
            continue;
        }
        
        // Push arguments onto the stack
        for (int j = 0; j < argc; ++j) {
            lua_pushstring(g_lua_state, argv[j]);
        }
        
        // Call the function
        if (lua_pcall(g_lua_state, argc, 0, 0) != LUA_OK) {
            logger_log_fmt(LOG_LEVEL_ERROR, "LUA_BRIDGE", "Error running hook '%s' function '%s': %s", 
                          hook_name, hook->function_names[i], lua_tostring(g_lua_state, -1));
            lua_pop(g_lua_state, 1); // Pop error message
            overall_result = ph_ERROR_EXEC_FAILED;
        }
    }
    
    return overall_result;
}

/**
 * @see lua_bridge.h
 */
bool lua_bridge_has_command(const char* command_name) {
    return find_lua_command(command_name) != NULL;
}

/**
 * @see lua_bridge.h
 */
size_t lua_bridge_get_command_count(void) {
    return g_lua_command_count;
}

/**
 * @see lua_bridge.h
 */
const char* lua_bridge_get_command_description(const char* command_name) {
    lua_command_entry_t* cmd = find_lua_command(command_name);
    return cmd ? cmd->description : NULL;
}

/**
 * @see lua_bridge.h
 */
const char** lua_bridge_get_all_command_names(void) {
    if (g_lua_command_count == 0) {
        return NULL;
    }

    // Aloca memória para um array de ponteiros de string
    const char** names = malloc(sizeof(char*) * g_lua_command_count);
    if (!names) {
        logger_log(LOG_LEVEL_ERROR, "LUA_BRIDGE", "Failed to allocate memory for command names list.");
        return NULL;
    }

    // Preenche o array com os ponteiros para os nomes dos comandos
    for (size_t i = 0; i < g_lua_command_count; i++) {
        names[i] = g_lua_commands[i].command_name;
    }

    return names;
}

/**
 * @brief Libera a memória alocada por lua_bridge_get_all_command_names.
 */
void lua_bridge_free_command_names_list(const char** names_list) {
    if (names_list) {
        // Libera apenas o array de ponteiros, não as strings em si.
        free((void*)names_list);
    }
}



/**
 * @see lua_bridge.h
 */
void lua_bridge_cleanup(void) {
    if (g_lua_state) {
        lua_close(g_lua_state);
        g_lua_state = NULL;
    }
    
    // Clean up command registry
    for (size_t i = 0; i < g_lua_command_count; i++) {
        free(g_lua_commands[i].command_name);
        free(g_lua_commands[i].lua_function_name);
        free(g_lua_commands[i].description);
        free(g_lua_commands[i].usage);
    }
    free(g_lua_commands);
    g_lua_commands = NULL;
    g_lua_command_count = 0;
    g_lua_command_capacity = 0;
    
    // Clean up hook registry
    for (size_t i = 0; i < g_hook_count; i++) {
        free(g_hook_registry[i].hook_name);
        for (size_t j = 0; j < g_hook_registry[i].function_count; j++) {
            free(g_hook_registry[i].function_names[j]);
        }
        free(g_hook_registry[i].function_names);
    }
    free(g_hook_registry);
    g_hook_registry = NULL;
    g_hook_count = 0;
    g_hook_capacity = 0;
    
    logger_log(LOG_LEVEL_INFO, "LUA_BRIDGE", "Enhanced Lua bridge cleaned up.");
}