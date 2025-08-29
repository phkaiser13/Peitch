/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * ph_core_api.h - Core API contract for ph modules.
 *
 * This header file defines the essential interface that every dynamically loaded
 * module must implement. It acts as a strict contract between the C-based core
 * application and external modules written in any language that can export C
 * symbols (C, C++, Rust, Go, etc.). The design promotes loose coupling,
 * allowing modules to be developed, compiled, and updated independently of
 * the core application.
 *
 * The lifecycle of a module is as follows:
 * 1. The core application dynamically loads the module's shared library.
 * 2. It retrieves the address of the `module_get_info` function to learn about
 *    the module's capabilities (name, version, supported commands).
 * 3. It calls `module_init`, passing a context object with pointers to core
 *    functions. This context includes a simple logger and a new, buffer-safe
 *    formatted logger (`log_fmt`), allowing modules to perform setup and logging
 *    securely and efficiently.
 * 4. When a user command matches one supported by the module, the core calls
 *    `module_exec` with the relevant arguments.
 * 5. Before the application exits, it calls `module_cleanup` for graceful
 *    shutdown and resource deallocation within the module.
 *
 * SPDX-License-Identifier: Apache-2.0 */

#ifndef ph_CORE_API_H
#define ph_CORE_API_H

#include <stdint.h> // For fixed-width integer types like int32_t
#include <stddef.h> // For size_t

/* Use C linkage for all symbols in this header. This is crucial for ensuring
 * that function names are not mangled by C++ compilers and are directly
 * linkable from Rust, Go, and other languages. */
#ifdef __cplusplus
extern "C" {
#endif

/**
 * @enum phStatus
 * @brief Defines standard status codes returned by module functions.
 *        This provides a consistent error handling mechanism across the
 *        entire application.
 */
typedef enum {
    ph_SUCCESS = 0,          /**< Operation completed successfully. */
    ph_ERROR_GENERAL = -1,   /**< A generic, unspecified error occurred. */
    ph_ERROR_INVALID_ARGS = -2,/**< Invalid arguments were passed to the function. */
    ph_ERROR_NOT_FOUND = -3, /**< A required resource (file, config) was not found. */
    ph_ERROR_INIT_FAILED = -4,/**< Module initialization failed. */
    ph_ERROR_EXEC_FAILED = -5 /**< Command execution failed. */
} phStatus;

/**
 * @enum phLogLevel
 * @brief Defines the severity levels for the logging system.
 *        Modules use these levels when calling the logger provided by the core.
 */
typedef enum {
    LOG_LEVEL_DEBUG,   /**< Detailed information for debugging. */
    LOG_LEVEL_INFO,    /**< General informational messages. */
    LOG_LEVEL_WARN,    /**< Warnings about potential issues. */
    LOG_LEVEL_ERROR,   /**< Errors that occurred but are recoverable. */
    LOG_LEVEL_FATAL    /**< Critical errors causing the application to terminate. */
} phLogLevel;


/**
 * @struct phModuleInfo
 * @brief A structure to hold metadata about a module.
 *        This is returned by `module_get_info` and used by the core to
 *        register the module and its commands.
 */
typedef struct {
    const char* name;         /**< The unique name of the module (e.g., "git_ops"). */
    const char* version;      /**< The module's version string (e.g., "1.0.0"). */
    const char* description;  /**< A brief description of the module's purpose. */
    const char** commands;    /**< A NULL-terminated array of command strings this module handles. */
} phModuleInfo;


/**
 * @struct phCoreContext
 * @brief A context object passed from the core to the modules during init.
 *        It provides access to core functionalities (callbacks) without
 *        exposing internal core data structures. This is a form of
 *        dependency injection.
 */
typedef struct {
    /**
     * @brief A function pointer to the core's simple logging system.
     * @param level The severity level of the message.
     * @param module_name The name of the module logging the message.
     * @param message The pre-formatted log message.
     */
    void (*log)(phLogLevel level, const char* module_name, const char* message);

    /**
     * @brief A function pointer to the core's safe formatted logging system.
     *
     * This function behaves like printf. It dynamically allocates memory for the
     * final message, making it immune to buffer overflow vulnerabilities.
     * Modules should prefer this over the simple `log` function when message
     * content has a variable or unpredictable size.
     *
     * @param level The severity level of the message.
     * @param module_name The name of the module logging the message.
     * @param format The printf-style format string.
     * @param ... Variable arguments for the format string.
     */
    void (*log_fmt)(phLogLevel level, const char* module_name, const char* format, ...);

    /**
     * @brief A function pointer to the core's configuration manager.
     * @param key The configuration key to retrieve.
     * @return The value as a string, or NULL if not found. The module
     *         should NOT free this string.
     */
    char* (*get_config_value)(const char* key);

    /**
     * @brief A function pointer to the core's UI rendering engine.
     * @param text The text to display to the user.
     */
    void (*print_ui)(const char* text);

} phCoreContext;


// --- REQUIRED MODULE EXPORTED FUNCTIONS ---

/**
 * @brief Retrieves metadata about the module.
 *
 * This function must be exported by every module. It is the first function
 * called by the core after loading the module to understand what it does.
 * The returned pointer and its contents must remain valid for the lifetime
 * of the application.
 *
 * @return A pointer to a static phModuleInfo struct.
 */
typedef const phModuleInfo* (*PFN_module_get_info)(void);

/**
 * @brief Initializes the module.
 *
 * This function is called once by the core after loading the module. The module
 * should perform any necessary setup, such as allocating memory or establishing
 * connections. It receives a context with callbacks to core functions.
 *
 * @param context A pointer to the core context object.
 * @return ph_SUCCESS on success, or an error code on failure.
 */
typedef phStatus (*PFN_module_init)(const phCoreContext* context);

/**
 * @brief Executes a command handled by the module.
 *
 * This function is called by the core when a user enters a command that this
 * module has registered.
 *
 * @param argc The number of arguments in the argv array. argv[0] is the command itself.
 * @param argv An array of argument strings.
 * @return ph_SUCCESS on success, or an error code on failure.
 */
typedef phStatus (*PFN_module_exec)(int argc, const char** argv);

/**
 * @brief Cleans up and de-initializes the module.
 *
 * This function is called once by the core just before the application exits.
 * The module must free all allocated resources to prevent memory leaks.
 */
typedef void (*PFN_module_cleanup)(void);


#ifdef __cplusplus
} // extern "C"
#endif

#endif // ph_CORE_API_H