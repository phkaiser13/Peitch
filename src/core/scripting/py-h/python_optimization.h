/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * python_optimization.h - Python bridge optimization configurations and utilities.
 *
 * This header provides compile-time and runtime optimization configurations
 * specifically tuned for maximum Python performance integration with the C core.
 * 
 * Includes:
 * - Compiler optimization hints and attributes
 * - Memory allocation strategies and pool configurations
 * - Python-specific performance tuning constants
 * - Platform-specific optimizations
 * - Profiling and benchmarking utilities
 *
 * SPDX-License-Identifier: Apache-2.0 */

#ifndef PYTHON_OPTIMIZATION_H
#define PYTHON_OPTIMIZATION_H

#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

// === COMPILER OPTIMIZATION ATTRIBUTES ===

// Hot path optimization - tells compiler this function is called frequently
#define PY_HOT_PATH __attribute__((hot))

// Cold path optimization - tells compiler this function is rarely called
#define PY_COLD_PATH __attribute__((cold))

// Force inline for critical performance functions
#define PY_FORCE_INLINE __attribute__((always_inline)) static inline

// Pure function - no side effects, result depends only on parameters
#define PY_PURE __attribute__((pure))

// Const function - no side effects, result is constant
#define PY_CONST __attribute__((const))

// Non-null parameters - enables compiler optimizations
#define PY_NONNULL(...) __attribute__((nonnull(__VA_ARGS__)))

// Memory alignment for cache line optimization
#define PY_CACHE_ALIGNED __attribute__((aligned(64)))

// Branch prediction hints
#define PY_LIKELY(x) __builtin_expect(!!(x), 1)
#define PY_UNLIKELY(x) __builtin_expect(!!(x), 0)

// === PERFORMANCE TUNING CONSTANTS ===

// Memory pool configurations
#define PY_SMALL_OBJECT_POOL_SIZE (64 * 1024)      // 64KB for small objects
#define PY_LARGE_OBJECT_POOL_SIZE (1024 * 1024)    // 1MB for large objects
#define PY_STRING_POOL_SIZE (256 * 1024)           // 256KB for string interning
#define PY_BYTECODE_CACHE_SIZE (2 * 1024 * 1024)   // 2MB for compiled bytecode

// Cache sizes optimized for L1/L2/L3 cache hierarchy
#define PY_L1_CACHE_SIZE 32768      // 32KB - typical L1 cache size
#define PY_L2_CACHE_SIZE 262144     // 256KB - typical L2 cache size
#define PY_L3_CACHE_SIZE 8388608    // 8MB - typical L3 cache size

// Python interpreter optimization limits
#define PY_MAX_CACHED_CONTEXTS 16
#define PY_MAX_CACHED_MODULES 64
#define PY_MAX_CACHED_FUNCTIONS 256
#define PY_MAX_INTERNED_STRINGS 512
#define PY_MAX_PRECOMPILED_SCRIPTS 128

// Performance monitoring thresholds
#define PY_SLOW_COMMAND_THRESHOLD_NS 1000000    // 1ms
#define PY_VERY_SLOW_COMMAND_THRESHOLD_NS 10000000  // 10ms
#define PY_GC_FORCE_THRESHOLD 1000              // Force GC after 1000 operations

// String optimization constants
#define PY_SHORT_STRING_THRESHOLD 64
#define PY_INTERN_STRING_THRESHOLD 32
#define PY_STATIC_BUFFER_SIZE 256

// === MEMORY ALLOCATION STRATEGIES ===

typedef enum {
    PY_ALLOC_SYSTEM,        // Use system malloc/free
    PY_ALLOC_POOL,          // Use memory pools
    PY_ALLOC_ARENA,         // Use arena allocation
    PY_ALLOC_STACK,         // Use stack allocation for small objects
    PY_ALLOC_MMAP           // Use memory mapping for large objects
} py_allocation_strategy_t;

typedef struct {
    py_allocation_strategy_t strategy;
    size_t pool_size;
    size_t alignment;
    bool use_zero_copy;
    bool enable_recycling;
} py_memory_config_t;

// === PYTHON-SPECIFIC OPTIMIZATIONS ===

typedef enum {
    PY_EXEC_NORMAL,         // Standard Python execution
    PY_EXEC_BYTECODE,       // Pre-compiled bytecode execution
    PY_EXEC_JIT,           // Just-in-time compilation
    PY_EXEC_NATIVE,        // Native code generation
    PY_EXEC_VECTORIZED     // Vectorized operations
} py_execution_mode_t;

typedef struct {
    py_execution_mode_t mode;
    bool enable_inlining;
    bool enable_loop_unrolling;
    bool enable_constant_folding;
    bool enable_dead_code_elimination;
    bool enable_tail_call_optimization;
} py_execution_config_t;

// === PERFORMANCE MONITORING STRUCTURES ===

typedef struct PY_CACHE_ALIGNED {
    uint64_t total_calls;
    uint64_t total_time_ns;
    uint64_t min_time_ns;
    uint64_t max_time_ns;
    uint64_t cache_hits;
    uint64_t cache_misses;
    double avg_time_ns;
    uint32_t error_count;
} py_function_stats_t;

typedef struct PY_CACHE_ALIGNED {
    py_function_stats_t* function_stats;
    size_t function_count;
    uint64_t total_memory_allocated;
    uint64_t peak_memory_usage;
    uint64_t gc_collections;
    uint64_t gc_time_ns;
    double cpu_usage_percent;
} py_performance_monitor_t;

// === OPTIMIZATION UTILITY FUNCTIONS ===

/**
 * @brief Initialize performance monitoring system.
 * @return true on success, false on failure
 */
bool py_perf_monitor_init(py_performance_monitor_t* monitor) PY_NONNULL(1);

/**
 * @brief Record function call statistics.
 * @param monitor Performance monitor instance
 * @param function_name Name of the function
 * @param execution_time_ns Execution time in nanoseconds
 * @param success Whether the function succeeded
 */
void py_perf_record_call(py_performance_monitor_t* monitor, 
                        const char* function_name,
                        uint64_t execution_time_ns,
                        bool success) PY_NONNULL(1, 2) PY_HOT_PATH;

/**
 * @brief Get high-resolution timestamp in nanoseconds.
 * @return Timestamp in nanoseconds
 */
PY_FORCE_INLINE uint64_t py_get_timestamp_ns(void) PY_PURE {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}

/**
 * @brief Optimize memory layout for cache performance.
 * @param data Pointer to data structure
 * @param size Size of the data structure
 */
void py_optimize_cache_layout(void* data, size_t size) PY_NONNULL(1);

/**
 * @brief Prefetch data into CPU cache.
 * @param addr Address to prefetch
 * @param rw 0 for read, 1 for write
 * @param locality Temporal locality hint (0-3)
 */
PY_FORCE_INLINE void py_prefetch(const void* addr, int rw, int locality) PY_NONNULL(1) {
    __builtin_prefetch(addr, rw, locality);
}

/**
 * @brief Fast string hash function optimized for Python strings.
 * @param str String to hash
 * @param len Length of string
 * @return Hash value
 */
PY_FORCE_INLINE uint32_t py_fast_string_hash(const char* str, size_t len) PY_NONNULL(1) PY_PURE {
    uint32_t hash = 5381;
    for (size_t i = 0; i < len; i++) {
        hash = ((hash << 5) + hash) + (unsigned char)str[i];
    }
    return hash;
}

/**
 * @brief Check if a pointer is properly aligned for the target architecture.
 * @param ptr Pointer to check
 * @param alignment Required alignment
 * @return true if properly aligned
 */
PY_FORCE_INLINE bool py_is_aligned(const void* ptr, size_t alignment) PY_NONNULL(1) PY_CONST {
    return ((uintptr_t)ptr % alignment) == 0;
}

// === PLATFORM-SPECIFIC OPTIMIZATIONS ===

#ifdef __x86_64__
// Use SIMD instructions for bulk operations on x86-64
#define PY_USE_SIMD 1
#define PY_CACHE_LINE_SIZE 64
#define PY_PAGE_SIZE 4096

PY_FORCE_INLINE void py_memory_barrier(void) {
    __asm__ volatile("mfence" ::: "memory");
}

#elif defined(__aarch64__)
// ARM64 optimizations
#define PY_USE_NEON 1
#define PY_CACHE_LINE_SIZE 64
#define PY_PAGE_SIZE 4096

PY_FORCE_INLINE void py_memory_barrier(void) {
    __asm__ volatile("dsb sy" ::: "memory");
}

#else
// Generic optimizations
#define PY_CACHE_LINE_SIZE 64
#define PY_PAGE_SIZE 4096

PY_FORCE_INLINE void py_memory_barrier(void) {
    __sync_synchronize();
}
#endif

// === DEBUGGING AND PROFILING MACROS ===

#ifdef PY_DEBUG_PERFORMANCE
#define PY_PERF_TRACE(msg) \
    do { \
        uint64_t ts = py_get_timestamp_ns(); \
        printf("[PERF] %lu: %s\n", ts, msg); \
    } while(0)

#define PY_PERF_TIMER_START(name) \
    uint64_t perf_timer_##name = py_get_timestamp_ns()

#define PY_PERF_TIMER_END(name) \
    do { \
        uint64_t elapsed = py_get_timestamp_ns() - perf_timer_##name; \
        printf("[PERF] %s: %lu ns\n", #name, elapsed); \
    } while(0)
#else
#define PY_PERF_TRACE(msg) do {} while(0)
#define PY_PERF_TIMER_START(name) do {} while(0)
#define PY_PERF_TIMER_END(name) do {} while(0)
#endif

// === COMPILE-TIME CONFIGURATION ===

// Enable/disable specific optimizations at compile time
#ifndef PY_ENABLE_MEMORY_POOLS
#define PY_ENABLE_MEMORY_POOLS 1
#endif

#ifndef PY_ENABLE_BYTECODE_CACHE
#define PY_ENABLE_BYTECODE_CACHE 1
#endif

#ifndef PY_ENABLE_STRING_INTERNING
#define PY_ENABLE_STRING_INTERNING 1
#endif

#ifndef PY_ENABLE_JIT_COMPILATION
#define PY_ENABLE_JIT_COMPILATION 0  // Experimental
#endif

#ifndef PY_ENABLE_VECTORIZATION
#define PY_ENABLE_VECTORIZATION 1
#endif

#ifndef PY_ENABLE_PROFILING
#define PY_ENABLE_PROFILING 1
#endif

#ifdef __cplusplus
} // extern "C"
#endif

#endif // PYTHON_OPTIMIZATION_H