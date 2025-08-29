/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * node_bridge.h - Ultra-high-performance Node.js/V8 scripting engine bridge interface.
 *
 * This header defines the public API for interacting with the embedded
 * Node.js/V8 scripting engine with extreme performance optimizations. Features:
 * - Persistent V8 isolates with context pooling
 * - Pre-compiled JavaScript bytecode execution
 * - Zero-copy buffer operations between C and JavaScript
 * - Direct V8 API calls bypassing Node.js overhead
 * - Optimized object allocation and garbage collection control
 * - JIT compilation hints and inline caching
 * - Memory-mapped file operations for large scripts
 * - SIMD-accelerated data processing when available
 *
 * Performance targets:
 * - Sub-100μs command execution for cached operations
 * - Zero-allocation fast paths for common operations
 * - Minimal V8 heap pressure through object pooling
 * - Native-speed numeric operations via typed arrays
 *
 * SPDX-License-Identifier: Apache-2.0 */

#ifndef NODE_BRIDGE_H
#define NODE_BRIDGE_H

#include "../../ipc/include/ph_core_api.h" // For phStatus
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// === FORWARD DECLARATIONS FOR V8 TYPES ===
typedef struct v8_isolate v8_isolate_t;
typedef struct v8_context v8_context_t;
typedef struct v8_object v8_object_t;
typedef struct v8_function v8_function_t;
typedef struct v8_script v8_script_t;

/**
 * @brief Performance optimization flags for Node.js bridge initialization.
 */
typedef enum {
    NODE_OPT_NONE = 0,
    NODE_OPT_PRECOMPILE_SCRIPTS = 1 << 0,    // Pre-compile all scripts to V8 bytecode
    NODE_OPT_DISABLE_GC_IDLE = 1 << 1,       // Disable idle-time garbage collection
    NODE_OPT_OPTIMIZE_FOR_SIZE = 1 << 2,     // Optimize for memory usage over speed
    NODE_OPT_OPTIMIZE_FOR_SPEED = 1 << 3,    // Optimize for maximum execution speed
    NODE_OPT_ENABLE_JIT_HINTS = 1 << 4,      // Provide JIT compilation hints
    NODE_OPT_ZERO_COPY_BUFFERS = 1 << 5,     // Use zero-copy buffer operations
    NODE_OPT_PERSISTENT_CONTEXTS = 1 << 6,   // Keep contexts alive between calls
    NODE_OPT_INLINE_CACHING = 1 << 7,        // Enable aggressive inline caching
    NODE_OPT_NATIVE_MODULES = 1 << 8,        // Use native modules when available
    NODE_OPT_SIMD_ACCELERATION = 1 << 9,     // Enable SIMD operations
    NODE_OPT_MEMORY_MAPPING = 1 << 10,       // Use memory-mapped files for scripts
    NODE_OPT_TURBOFAN_ALWAYS = 1 << 11,      // Force TurboFan optimization always
    NODE_OPT_ALL = 0xFFFF                    // Enable all optimizations
} node_optimization_flags_t;

/**
 * @brief Node.js execution context with V8 isolate management.
 */
typedef struct {
    v8_isolate_t* isolate;           // V8 isolate instance
    v8_context_t* context;           // V8 execution context
    v8_object_t* global_object;      // Cached global object
    v8_object_t* ph_module;          // Cached ph module object
    void* persistent_data;           // Persistent data storage
    uint64_t creation_time;          // Context creation timestamp
    uint64_t last_used;              // Last usage timestamp
    uint32_t ref_count;              // Reference counter
    uint16_t optimization_level;     // Current optimization level
    bool is_optimized;               // Whether context is JIT-optimized
    bool has_native_modules;         // Whether native modules are loaded
} node_context_t;

/**
 * @brief Cached Node.js command entry for ultra-fast execution.
 */
typedef struct {
    char* command_name;              // Command identifier
    v8_script_t* compiled_script;    // Pre-compiled V8 script
    v8_function_t* function_handle;  // Direct function handle
    void* cached_source_code;        // Cached source for recompilation
    size_t source_size;              // Source code size
    char* description;               // Command description
    char* usage;                     // Usage string
    node_context_t* preferred_context; // Optimal context for this command
    uint64_t compilation_time;       // Time taken to compile
    uint64_t last_executed;          // Last execution timestamp
    uint32_t execution_count;        // Number of times executed
    uint32_t optimization_tier;      // V8 optimization tier (0-4)
    bool is_hot;                     // Whether function is hot (frequently called)
    bool is_native;                  // Whether backed by native code
} node_command_cache_t;

/**
 * @brief Advanced performance statistics and monitoring.
 */
typedef struct {
    // Execution statistics
    uint64_t total_commands_executed;
    uint64_t total_execution_time_ns;
    uint64_t min_execution_time_ns;
    uint64_t max_execution_time_ns;
    double avg_execution_time_ns;
    
    // Cache performance
    uint64_t cache_hits;
    uint64_t cache_misses;
    uint64_t script_compilations;
    uint64_t recompilations;
    
    // Memory statistics
    uint64_t heap_used_bytes;
    uint64_t heap_total_bytes;
    uint64_t external_memory_bytes;
    uint64_t peak_heap_usage;
    
    // Garbage collection
    uint64_t gc_count;
    uint64_t gc_time_total_ns;
    uint64_t gc_time_avg_ns;
    
    // V8 specific metrics
    uint32_t optimized_functions;
    uint32_t deoptimized_functions;
    uint32_t inline_cache_hits;
    uint32_t inline_cache_misses;
    
    // Performance counters
    uint64_t zero_copy_operations;
    uint64_t simd_operations;
    uint64_t native_calls;
    
    // Context management
    uint32_t contexts_created;
    uint32_t contexts_destroyed;
    uint32_t context_switches;
} node_perf_stats_t;

/**
 * @brief Buffer for zero-copy operations between C and JavaScript.
 */
typedef struct {
    void* data;                      // Raw data pointer
    size_t size;                     // Buffer size
    size_t capacity;                 // Buffer capacity
    uint32_t ref_count;              // Reference counter
    bool is_external;                // Whether buffer is externally allocated
    bool is_read_only;               // Whether buffer is read-only
    void (*finalizer)(void* data);   // Cleanup function
} node_zero_copy_buffer_t;

/**
 * @brief Hook registration for lifecycle events.
 */
typedef struct {
    char* hook_name;                 // Hook identifier
    v8_function_t** functions;       // Array of cached function handles
    size_t function_count;           // Number of registered functions
    size_t function_capacity;        // Capacity of function array
    uint64_t total_execution_time;   // Total time spent in this hook
    uint32_t execution_count;        // Number of times hook was called
} node_hook_registry_t;

/**
 * @brief TypeScript compilation cache entry.
 */
typedef struct {
    char* source_path;               // Original TypeScript file path
    char* compiled_js;               // Compiled JavaScript code
    size_t compiled_size;            // Size of compiled code
    uint64_t source_mtime;           // Source file modification time
    uint64_t compilation_time;       // When compilation happened
    v8_script_t* compiled_script;    // Pre-compiled V8 script
    bool needs_recompilation;        // Whether recompilation is needed
} ts_compilation_cache_t;

// === CORE API FUNCTIONS ===

/**
 * @brief Initializes the ultra-high-performance Node.js/V8 scripting engine.
 *
 * Creates optimized V8 isolates, sets up memory pools, configures JIT compilation,
 * and loads all *.js/*.ts scripts from the 'plugins' directory with aggressive
 * optimization and bytecode pre-compilation.
 *
 * @param opt_flags Optimization flags for maximum performance tuning
 * @return ph_SUCCESS on success, or an error code on failure
 */
phStatus node_bridge_init(node_optimization_flags_t opt_flags);

/**
 * @brief Creates a new high-performance Node.js execution context.
 *
 * @param context Pointer to context structure to initialize
 * @param isolate_flags V8 isolate creation flags
 * @return ph_SUCCESS on success, or an error code on failure
 */
phStatus node_bridge_create_context(node_context_t* context, uint32_t isolate_flags);

/**
 * @brief Destroys a Node.js execution context and performs aggressive cleanup.
 *
 * @param context Pointer to context to destroy
 */
void node_bridge_destroy_context(node_context_t* context);

/**
 * @brief Executes a JavaScript/TypeScript command with maximum optimization.
 *
 * Uses pre-compiled bytecode, persistent contexts, inline caching, and
 * zero-copy buffer operations for maximum performance.
 *
 * @param command_name The name of the command to execute
 * @param argc The number of arguments
 * @param argv The argument vector
 * @return ph_SUCCESS if execution succeeds, error code otherwise
 */
phStatus node_bridge_execute_command_optimized(const char* command_name, int argc, const char** argv);

/**
 * @brief Runs lifecycle hooks with batched execution and optimization.
 *
 * @param hook_name The name of the hook to run
 * @param argc The number of arguments
 * @param argv The argument vector
 * @return ph_SUCCESS if all hooks execute successfully
 */
phStatus node_bridge_run_hook_batch(const char* hook_name, int argc, const char** argv);

/**
 * @brief Pre-compiles JavaScript/TypeScript to V8 bytecode for instant execution.
 *
 * @param script_path Path to the script file
 * @param output_path Optional path to save compiled bytecode
 * @param context Target context for compilation
 * @return ph_SUCCESS on successful compilation
 */
phStatus node_bridge_precompile_script(const char* script_path, const char* output_path, node_context_t* context);

/**
 * @brief Compiles TypeScript to optimized JavaScript with caching.
 *
 * @param ts_source TypeScript source code
 * @param source_size Size of source code
 * @param output_js Pointer to store compiled JavaScript
 * @param output_size Pointer to store output size
 * @return ph_SUCCESS on successful compilation
 */
phStatus node_bridge_compile_typescript(const char* ts_source, size_t source_size, char** output_js, size_t* output_size);

/**
 * @brief Performs warmup operations for optimal performance.
 *
 * Pre-loads common modules, optimizes frequently used functions,
 * and prepares JIT compilation hints.
 *
 * @return ph_SUCCESS on successful warmup
 */
phStatus node_bridge_warmup(void);

/**
 * @brief Creates a zero-copy buffer for efficient data transfer.
 *
 * @param data Pointer to data
 * @param size Size of data
 * @param buffer Pointer to buffer structure to initialize
 * @return ph_SUCCESS if buffer is created successfully
 */
phStatus node_bridge_create_zero_copy_buffer(void* data, size_t size, node_zero_copy_buffer_t* buffer);

/**
 * @brief Releases a zero-copy buffer and its resources.
 *
 * @param buffer Buffer to release
 */
void node_bridge_release_zero_copy_buffer(node_zero_copy_buffer_t* buffer);

// === PERFORMANCE AND MONITORING ===

/**
 * @brief Checks if a command is registered and optimally cached.
 *
 * @param command_name The command to check
 * @return true if the command is registered and optimized
 */
bool node_bridge_has_command_cached(const char* command_name);

/**
 * @brief Gets comprehensive performance statistics.
 *
 * @param stats Pointer to structure to fill with statistics
 * @return ph_SUCCESS if statistics are retrieved successfully
 */
phStatus node_bridge_get_performance_stats(node_perf_stats_t* stats);

/**
 * @brief Optimizes the Node.js bridge based on runtime profiling data.
 *
 * Analyzes call patterns, recompiles hot functions with TurboFan,
 * optimizes memory layout, and adjusts garbage collection parameters.
 *
 * @return ph_SUCCESS if optimization completes successfully
 */
phStatus node_bridge_optimize_runtime(void);

/**
 * @brief Forces V8 garbage collection with specific strategy.
 *
 * @param gc_type Type of garbage collection (0=scavenge, 1=mark-compact, 2=incremental)
 * @return Number of bytes freed
 */
size_t node_bridge_force_gc(int gc_type);

/**
 * @brief Provides JIT compilation hints for a function.
 *
 * @param function_name Name of the function to optimize
 * @param hint_flags Optimization hint flags
 * @return ph_SUCCESS if hints are applied successfully
 */
phStatus node_bridge_provide_jit_hints(const char* function_name, uint32_t hint_flags);

/**
 * @brief Preloads and caches native modules for faster access.
 *
 * @param module_names Array of module names to preload
 * @param count Number of modules
 * @return ph_SUCCESS if modules are preloaded successfully
 */
phStatus node_bridge_preload_native_modules(const char** module_names, size_t count);

// === COMMAND MANAGEMENT ===

/**
 * @brief Gets the total number of cached Node.js commands.
 * @return The count of registered and cached commands
 */
size_t node_bridge_get_command_count(void);

/**
 * @brief Gets the description for a specific Node.js command.
 * @param command_name The name of the command
 * @return A read-only string with the description, or NULL if not found
 */
const char* node_bridge_get_command_description(const char* command_name);

/**
 * @brief Gets a list of all Node.js-registered command names.
 * @return A dynamically allocated array of command names. Must be freed with
 * node_bridge_free_command_names_list().
 */
const char** node_bridge_get_all_command_names(void);

/**
 * @brief Frees the command names list.
 * @param names_list The list to free
 */
void node_bridge_free_command_names_list(const char** names_list);

/**
 * @brief Gets detailed information about a cached command.
 * @param command_name The name of the command
 * @return Pointer to command cache entry, or NULL if not found
 */
const node_command_cache_t* node_bridge_get_command_info(const char* command_name);

// === ADVANCED FEATURES ===

/**
 * @brief Executes JavaScript code directly with optimization.
 *
 * @param source JavaScript source code
 * @param source_size Size of source code
 * @param context Context to execute in (NULL for default)
 * @param result Pointer to store execution result
 * @return ph_SUCCESS if execution succeeds
 */
phStatus node_bridge_eval_optimized(const char* source, size_t source_size, node_context_t* context, char** result);

/**
 * @brief Registers a C function to be callable from JavaScript.
 *
 * @param name Function name in JavaScript
 * @param callback C function pointer
 * @param arg_count Expected argument count
 * @return ph_SUCCESS if registration succeeds
 */
phStatus node_bridge_register_native_function(const char* name, void* callback, int arg_count);

/**
 * @brief Enables SIMD acceleration for specific operations.
 *
 * @param operation_mask Bitmask of operations to accelerate
 * @return ph_SUCCESS if SIMD is enabled successfully
 */
phStatus node_bridge_enable_simd(uint32_t operation_mask);

/**
 * @brief Creates a memory-mapped script for ultra-fast loading.
 *
 * @param script_path Path to script file
 * @param mmap_handle Output handle for memory mapping
 * @return ph_SUCCESS if memory mapping succeeds
 */
phStatus node_bridge_mmap_script(const char* script_path, void** mmap_handle);

/**
 * @brief Releases a memory-mapped script.
 *
 * @param mmap_handle Memory mapping handle
 * @param file_size Size of mapped file
 */
void node_bridge_unmap_script(void* mmap_handle, size_t file_size);

// === CLEANUP AND SHUTDOWN ===

/**
 * @brief Shuts down the Node.js engine with comprehensive cleanup.
 *
 * Performs comprehensive cleanup including:
 * - All execution contexts and isolates
 * - Compiled script caches and bytecode
 * - Memory pools and zero-copy buffers
 * - TypeScript compilation cache
 * - Native module handles
 * - V8 isolate disposal
 */
void node_bridge_cleanup(void);

/**
 * @brief Emergency shutdown for critical error situations.
 *
 * Immediately terminates all Node.js operations without cleanup.
 * Use only in critical error situations where normal cleanup might hang.
 */
void node_bridge_emergency_shutdown(void);

/**
 * @brief Validates the Node.js bridge state for debugging.
 *
 * @return ph_SUCCESS if bridge state is valid
 */
phStatus node_bridge_validate_state(void);

#ifdef __cplusplus
} // extern "C"
#endif

#endif // NODE_BRIDGE_H