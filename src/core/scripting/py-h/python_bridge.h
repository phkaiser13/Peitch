/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * python_bridge.h - High-performance Python scripting engine bridge interface.
 *
 * This header defines the public API for interacting with the embedded
 * Python scripting engine with extreme performance optimizations. Features:
 * - Persistent Python interpreter with module caching
 * - Pre-compiled bytecode execution
 * - Minimal Python object creation/destruction
 * - Direct C API calls bypassing Python overhead
 * - Memory pool allocation for frequent operations
 * - Lazy loading and JIT compilation strategies
 *
 * SPDX-License-Identifier: Apache-2.0 */

#ifndef PYTHON_BRIDGE_H
#define PYTHON_BRIDGE_H

#include "../../ipc/include/ph_core_api.h" // For phStatus
#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @brief Performance optimization flags for Python bridge initialization.
 */
typedef enum {
    PY_OPT_NONE = 0,
    PY_OPT_PRECOMPILE = 1 << 0,      // Pre-compile all scripts to bytecode
    PY_OPT_DISABLE_GC = 1 << 1,      // Disable Python garbage collection
    PY_OPT_NO_SITE = 1 << 2,         // Skip site.py loading for faster startup
    PY_OPT_POOL_OBJECTS = 1 << 3,    // Use object pooling for frequent allocations
    PY_OPT_FREEZE_MODULES = 1 << 4,  // Freeze imported modules in memory
    PY_OPT_FAST_CALLS = 1 << 5,      // Use vectorcall protocol when available
    PY_OPT_ALL = 0xFF                // Enable all optimizations
} py_optimization_flags_t;

/**
 * @brief Python execution context for performance isolation.
 */
typedef struct {
    void* interpreter;     // PyInterpreterState*
    void* thread_state;    // PyThreadState*
    void* globals_dict;    // PyObject* - cached globals dictionary
    void* ph_module;       // PyObject* - cached ph module
    bool is_active;        // Context activation state
    size_t ref_count;      // Reference counting for context reuse
} py_context_t;

/**
 * @brief Cached Python command entry for ultra-fast execution.
 */
typedef struct {
    char* command_name;
    void* compiled_code;   // PyCodeObject* - pre-compiled bytecode
    void* function_obj;    // PyObject* - cached function object
    char* description;
    char* usage;
    py_context_t* context; // Dedicated context for this command
    uint64_t last_used;    // Timestamp for LRU cache management
    uint32_t call_count;   // Statistics for optimization decisions
} py_command_cache_t;

/**
 * @brief Performance statistics for monitoring and optimization.
 */
typedef struct {
    uint64_t total_commands_executed;
    uint64_t total_execution_time_ns;
    uint64_t cache_hits;
    uint64_t cache_misses;
    uint64_t memory_allocations;
    uint64_t gc_collections;
    double avg_execution_time_ns;
    size_t peak_memory_usage;
} py_perf_stats_t;

/**
 * @brief Initializes the high-performance Python scripting engine.
 *
 * Creates an optimized Python interpreter with performance flags,
 * sets up memory pools, pre-loads modules, and loads all *.py scripts
 * from the 'plugins' directory with bytecode compilation.
 *
 * @param opt_flags Optimization flags to enable specific performance features
 * @return ph_SUCCESS on success, or an error code on failure
 */
phStatus python_bridge_init(py_optimization_flags_t opt_flags);

/**
 * @brief Creates a new high-performance Python execution context.
 *
 * @param context Pointer to context structure to initialize
 * @return ph_SUCCESS on success, or an error code on failure
 */
phStatus python_bridge_create_context(py_context_t* context);

/**
 * @brief Destroys a Python execution context and frees resources.
 *
 * @param context Pointer to context to destroy
 */
void python_bridge_destroy_context(py_context_t* context);

/**
 * @brief Executes a command with maximum performance optimization.
 *
 * Uses cached bytecode, persistent contexts, and minimal Python overhead.
 *
 * @param command_name The name of the command to execute
 * @param argc The number of arguments
 * @param argv The argument vector
 * @return ph_SUCCESS if execution succeeds, error code otherwise
 */
phStatus python_bridge_execute_command_fast(const char* command_name, int argc, const char** argv);

/**
 * @brief Runs lifecycle hooks with batched execution for performance.
 *
 * @param hook_name The name of the hook to run
 * @param argc The number of arguments
 * @param argv The argument vector
 * @return ph_SUCCESS if all hooks execute successfully
 */
phStatus python_bridge_run_hook_batch(const char* hook_name, int argc, const char** argv);

/**
 * @brief Pre-compiles a Python script to bytecode for faster execution.
 *
 * @param script_path Path to the Python script
 * @param output_path Path to save compiled bytecode (optional, NULL for memory only)
 * @return ph_SUCCESS on successful compilation
 */
phStatus python_bridge_precompile_script(const char* script_path, const char* output_path);

/**
 * @brief Warms up the Python bridge by pre-loading and caching common operations.
 *
 * @return ph_SUCCESS on successful warmup
 */
phStatus python_bridge_warmup(void);

/**
 * @brief Checks if a command is registered and cached for fast execution.
 *
 * @param command_name The command to check
 * @return true if the command is registered and cached
 */
bool python_bridge_has_command_cached(const char* command_name);

/**
 * @brief Gets performance statistics for monitoring and optimization.
 *
 * @param stats Pointer to structure to fill with statistics
 * @return ph_SUCCESS if statistics are retrieved successfully
 */
phStatus python_bridge_get_performance_stats(py_perf_stats_t* stats);

/**
 * @brief Optimizes the Python bridge based on usage patterns.
 *
 * Analyzes call patterns and optimizes caches, contexts, and memory usage.
 *
 * @return ph_SUCCESS if optimization completes successfully
 */
phStatus python_bridge_optimize(void);

/**
 * @brief Forces garbage collection and memory cleanup.
 *
 * @return Number of objects collected
 */
size_t python_bridge_force_gc(void);

/**
 * @brief Gets the total number of cached Python commands.
 * @return The count of registered and cached Python commands
 */
size_t python_bridge_get_command_count(void);

/**
 * @brief Gets the description for a specific Python command.
 * @param command_name The name of the command
 * @return A read-only string with the description, or NULL if not found
 */
const char* python_bridge_get_command_description(const char* command_name);

/**
 * @brief Gets a list of all Python-registered command names.
 * @return A dynamically allocated array of command names. Must be freed with
 * python_bridge_free_command_names_list().
 */
const char** python_bridge_get_all_command_names(void);

/**
 * @brief Frees the command names list returned by python_bridge_get_all_command_names.
 * @param names_list The list to free
 */
void python_bridge_free_command_names_list(const char** names_list);

/**
 * @brief Shuts down the Python engine with aggressive cleanup.
 *
 * Performs aggressive cleanup to free all resources, including:
 * - All execution contexts
 * - Compiled bytecode caches
 * - Memory pools
 * - Python interpreter instance
 */
void python_bridge_cleanup(void);

/**
 * @brief Emergency shutdown for critical situations.
 *
 * Immediately terminates Python operations without cleanup.
 * Use only in critical error situations.
 */
void python_bridge_emergency_shutdown(void);

#ifdef __cplusplus
} // extern "C"
#endif

#endif // PYTHON_BRIDGE_H