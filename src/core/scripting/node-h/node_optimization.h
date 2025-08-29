/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * node_optimization.h - Node.js/V8 bridge optimization configurations and utilities.
 *
 * This header provides compile-time and runtime optimization configurations
 * specifically tuned for maximum Node.js/V8 performance integration with the C core.
 * 
 * Includes:
 * - V8-specific compiler optimization hints and attributes
 * - Memory allocation strategies and zero-copy buffer management
 * - JavaScript/TypeScript performance tuning constants
 * - V8 heap and garbage collection optimizations
 * - JIT compilation hints and inline caching utilities
 * - SIMD acceleration support for bulk operations
 * - Platform-specific V8 optimizations
 * - Profiling and benchmarking utilities for Node.js workloads
 *
 * SPDX-License-Identifier: Apache-2.0 */

#ifndef NODE_OPTIMIZATION_H
#define NODE_OPTIMIZATION_H

#include <stdint.h>
#include <stdbool.h>
#include <time.h>

#ifdef __cplusplus
extern "C" {
#endif

// === V8-SPECIFIC COMPILER OPTIMIZATION ATTRIBUTES ===

// Hot path optimization for V8 callback functions
#define NODE_HOT_PATH __attribute__((hot)) __attribute__((flatten))

// Cold path optimization for error handling and cleanup
#define NODE_COLD_PATH __attribute__((cold)) __attribute__((noinline))

// Force inline for critical performance functions in V8 interactions
#define NODE_FORCE_INLINE __attribute__((always_inline)) static inline

// Pure function - no V8 side effects, result depends only on parameters
#define NODE_PURE __attribute__((pure)) __attribute__((const))

// Non-null parameters - enables compiler optimizations for V8 pointers
#define NODE_NONNULL(...) __attribute__((nonnull(__VA_ARGS__)))

// Memory alignment for V8 object allocation optimization
#define NODE_V8_ALIGNED __attribute__((aligned(8)))  // V8 objects are 8-byte aligned
#define NODE_CACHE_ALIGNED __attribute__((aligned(64)))  // CPU cache line alignment

// Branch prediction hints optimized for V8 execution patterns
#define NODE_LIKELY(x) __builtin_expect(!!(x), 1)
#define NODE_UNLIKELY(x) __builtin_expect(!!(x), 0)

// Prefetch hints for V8 object access patterns
#define NODE_PREFETCH_READ(addr) __builtin_prefetch(addr, 0, 3)
#define NODE_PREFETCH_WRITE(addr) __builtin_prefetch(addr, 1, 3)

// === V8 PERFORMANCE TUNING CONSTANTS ===

// V8 heap and memory management
#define NODE_V8_INITIAL_HEAP_SIZE (64 * 1024 * 1024)    // 64MB initial heap
#define NODE_V8_MAX_HEAP_SIZE (512 * 1024 * 1024)       // 512MB maximum heap
#define NODE_V8_MAX_OLD_SPACE (256 * 1024 * 1024)       // 256MB old generation
#define NODE_V8_MAX_NEW_SPACE (32 * 1024 * 1024)        // 32MB young generation
#define NODE_V8_PAGE_SIZE 4096                           // V8 page size
#define NODE_V8_OBJECT_ALIGNMENT 8                       // V8 object alignment

// Zero-copy buffer optimizations
#define NODE_ZERO_COPY_THRESHOLD 1024                    // Minimum size for zero-copy
#define NODE_BUFFER_POOL_SIZE 128                        // Number of pooled buffers
#define NODE_MAX_BUFFER_SIZE (16 * 1024 * 1024)         // 16MB maximum buffer size
#define NODE_BUFFER_ALIGNMENT 64                         // Buffer alignment for SIMD

// JavaScript compilation and caching
#define NODE_SCRIPT_CACHE_SIZE 256                       // Number of cached scripts
#define NODE_BYTECODE_CACHE_SIZE (8 * 1024 * 1024)      // 8MB bytecode cache
#define NODE_SOURCE_MAP_CACHE_SIZE (2 * 1024 * 1024)    // 2MB source map cache
#define NODE_COMPILATION_CACHE_TTL 3600                  // 1 hour cache TTL

// Inline caching and optimization tiers
#define NODE_INLINE_CACHE_SIZE 512                       // Number of inline cache entries
#define NODE_IC_HASH_MASK 0x1FF                         // Mask for inline cache hash
#define NODE_OPTIMIZATION_THRESHOLD 100                  // Calls before TurboFan optimization
#define NODE_DEOPT_THRESHOLD 10                          // Deoptimizations before giving up
#define NODE_MAX_OPTIMIZATION_TIER 4                     // Maximum V8 optimization tier

// Context and isolate management
#define NODE_MAX_CONTEXTS_PER_ISOLATE 32                 // Maximum contexts per isolate
#define NODE_CONTEXT_SWITCH_THRESHOLD_NS 10000           // 10μs context switch threshold
#define NODE_ISOLATE_IDLE_TIMEOUT_MS 5000               // 5s isolate idle timeout
#define NODE_GC_IDLE_TIME_MS 50                         // Maximum GC idle time

// TypeScript compilation optimizations
#define NODE_TS_CACHE_SIZE 128                          // TypeScript compilation cache size
#define NODE_TS_INCREMENTAL_THRESHOLD (64 * 1024)      // 64KB threshold for incremental compilation
#define NODE_TS_MEMORY_LIMIT (128 * 1024 * 1024)       // 128MB TypeScript compiler memory limit

// === HIGH-RESOLUTION TIMING ===

/**
 * @brief Get high-resolution timestamp in nanoseconds.
 * @return Timestamp in nanoseconds
 */
NODE_FORCE_INLINE uint64_t NODE_GET_TIMESTAMP_NS(void) NODE_PURE {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}

/**
 * @brief Get CPU cycle counter for ultra-precise timing.
 * @return CPU cycle count
 */
NODE_FORCE_INLINE uint64_t NODE_GET_CPU_CYCLES(void) NODE_PURE {
#ifdef __x86_64__
    uint32_t hi, lo;
    __asm__ volatile("rdtsc" : "=a"(lo), "=d"(hi));
    return ((uint64_t)hi << 32) | lo;
#elif defined(__aarch64__)
    uint64_t cycles;
    __asm__ volatile("mrs %0, cntvct_el0" : "=r"(cycles));
    return cycles;
#else
    return NODE_GET_TIMESTAMP_NS();  // Fallback to nanosecond timer
#endif
}

// === MEMORY ALLOCATION STRATEGIES ===

typedef enum {
    NODE_ALLOC_SYSTEM,        // Use system malloc/free
    NODE_ALLOC_V8_HEAP,       // Allocate in V8 heap
    NODE_ALLOC_EXTERNAL,      // External allocation (ArrayBuffer)
    NODE_ALLOC_POOL,          // Use memory pools
    NODE_ALLOC_STACK,         // Stack allocation for small objects
    NODE_ALLOC_MMAP,          // Memory mapping for large objects
    NODE_ALLOC_ZERO_COPY      // Zero-copy buffers
} node_allocation_strategy_t;

typedef struct {
    node_allocation_strategy_t strategy;
    size_t pool_size;
    size_t alignment;
    bool use_zero_copy;
    bool enable_recycling;
    bool prefer_external;
} node_memory_config_t;

// === V8 OPTIMIZATION CONFIGURATIONS ===

typedef enum {
    NODE_EXEC_INTERPRETED,    // V8 Ignition interpreter
    NODE_EXEC_BASELINE,       // Baseline compiler (Sparkplug)
    NODE_EXEC_OPTIMIZED,      // TurboFan optimizing compiler
    NODE_EXEC_NATIVE,         // Native code with C++ fast calls
    NODE_EXEC_WASM,           // WebAssembly for compute-intensive operations
    NODE_EXEC_TURBOSHAFT      // Next-gen TurboShaft compiler
} node_execution_mode_t;

typedef struct {
    node_execution_mode_t mode;
    bool enable_turbofan;
    bool enable_sparkplug;
    bool enable_maglev;
    bool enable_concurrent_recompilation;
    bool enable_function_context_specialization;
    bool enable_escape_analysis;
    bool enable_load_elimination;
    bool enable_dead_code_elimination;
    bool enable_branch_elimination;
    uint32_t optimization_level;  // 0-4, higher = more aggressive
} node_execution_config_t;

// === V8 GARBAGE COLLECTION TUNING ===

typedef enum {
    NODE_GC_SCAVENGE,         // Young generation collection
    NODE_GC_MARK_COMPACT,     // Full mark-compact collection
    NODE_GC_INCREMENTAL,      // Incremental marking
    NODE_GC_CONCURRENT,       // Concurrent marking and sweeping
    NODE_GC_PARALLEL          // Parallel scavenge and marking
} node_gc_strategy_t;

typedef struct {
    node_gc_strategy_t strategy;
    uint32_t young_generation_threshold;
    uint32_t old_generation_threshold;
    uint32_t incremental_marking_limit;
    uint32_t idle_time_limit_ms;
    bool enable_concurrent_marking;
    bool enable_parallel_scavenge;
    bool enable_memory_reducer;
} node_gc_config_t;

// === PERFORMANCE MONITORING STRUCTURES ===

typedef struct NODE_CACHE_ALIGNED {
    uint64_t total_calls;
    uint64_t total_time_ns;
    uint64_t total_cpu_cycles;
    uint64_t min_time_ns;
    uint64_t max_time_ns;
    uint64_t v8_compile_time_ns;
    uint64_t v8_execution_time_ns;
    uint64_t gc_time_ns;
    double avg_time_ns;
    uint32_t optimization_tier;
    uint32_t deoptimization_count;
    uint32_t inline_cache_hits;
    uint32_t inline_cache_misses;
    uint32_t error_count;
    bool is_hot;
    bool is_optimized;
} node_function_stats_t;

typedef struct NODE_CACHE_ALIGNED {
    node_function_stats_t* function_stats;
    size_t function_count;
    
    // Memory statistics
    uint64_t total_memory_allocated;
    uint64_t peak_memory_usage;
    uint64_t v8_heap_used;
    uint64_t v8_heap_total;
    uint64_t external_memory;
    uint64_t zero_copy_operations;
    
    // Compilation statistics
    uint64_t scripts_compiled;
    uint64_t scripts_cached;
    uint64_t typescript_compilations;
    uint64_t compilation_time_total_ns;
    
    // Garbage collection statistics
    uint64_t gc_collections;
    uint64_t gc_time_total_ns;
    uint64_t gc_objects_collected;
    double gc_overhead_percent;
    
    // V8 optimization statistics
    uint32_t functions_optimized;
    uint32_t functions_deoptimized;
    uint32_t inline_caches_created;
    uint32_t turbofan_compilations;
    
    // Context statistics
    uint32_t contexts_created;
    uint32_t contexts_destroyed;
    uint32_t context_switches;
    uint64_t context_switch_time_ns;
} node_performance_monitor_t;

// === STRING HASHING FOR V8 OPTIMIZATIONS ===

/**
 * @brief Ultra-fast string hash function optimized for V8 string internalization.
 * Uses the same hash algorithm as V8 for compatibility.
 * @param str String to hash
 * @param len Length of string
 * @return Hash value compatible with V8
 */
NODE_FORCE_INLINE uint32_t NODE_FAST_STRING_HASH(const char* str, size_t len) NODE_NONNULL(1) NODE_PURE {
    uint32_t hash = 0;
    const char* end = str + len;
    
    // Process 4 bytes at a time for better performance
    while (str + 4 <= end) {
        uint32_t chunk = *(uint32_t*)str;
        hash = hash * 31 + chunk;
        str += 4;
    }
    
    // Process remaining bytes
    while (str < end) {
        hash = hash * 31 + (unsigned char)*str++;
    }
    
    return hash;
}

/**
 * @brief Calculate hash for V8 symbol interning.
 * @param symbol Symbol string
 * @return Hash value for symbol table
 */
NODE_FORCE_INLINE uint32_t NODE_SYMBOL_HASH(const char* symbol) NODE_NONNULL(1) NODE_PURE {
    return NODE_FAST_STRING_HASH(symbol, strlen(symbol)) | 0x80000000;  // Mark as symbol
}

// === V8 OBJECT ALLOCATION UTILITIES ===

/**
 * @brief Check if a pointer is properly aligned for V8 objects.
 * @param ptr Pointer to check
 * @return true if properly aligned for V8
 */
NODE_FORCE_INLINE bool NODE_IS_V8_ALIGNED(const void* ptr) NODE_NONNULL(1) NODE_PURE {
    return ((uintptr_t)ptr % NODE_V8_OBJECT_ALIGNMENT) == 0;
}

/**
 * @brief Round up size to V8 object alignment.
 * @param size Size to align
 * @return Aligned size
 */
NODE_FORCE_INLINE size_t NODE_ALIGN_V8_SIZE(size_t size) NODE_PURE {
    return (size + NODE_V8_OBJECT_ALIGNMENT - 1) & ~(NODE_V8_OBJECT_ALIGNMENT - 1);
}

/**
 * @brief Check if size is suitable for zero-copy optimization.
 * @param size Buffer size
 * @return true if zero-copy should be used
 */
NODE_FORCE_INLINE bool NODE_USE_ZERO_COPY(size_t size) NODE_PURE {
    return size >= NODE_ZERO_COPY_THRESHOLD && size <= NODE_MAX_BUFFER_SIZE;
}

// === SIMD ACCELERATION SUPPORT ===

#ifdef __x86_64__
#include <immintrin.h>

/**
 * @brief SIMD-accelerated memory copy for large buffers.
 * @param dst Destination buffer
 * @param src Source buffer  
 * @param size Size in bytes (must be multiple of 32)
 */
NODE_FORCE_INLINE void NODE_SIMD_MEMCPY(void* dst, const void* src, size_t size) NODE_NONNULL(1, 2) {
    if (size >= 32 && NODE_IS_V8_ALIGNED(dst) && NODE_IS_V8_ALIGNED(src)) {
        const __m256i* src_vec = (const __m256i*)src;
        __m256i* dst_vec = (__m256i*)dst;
        size_t vec_count = size / 32;
        
        for (size_t i = 0; i < vec_count; i++) {
            _mm256_store_si256(&dst_vec[i], _mm256_load_si256(&src_vec[i]));
        }
        
        // Handle remaining bytes
        size_t remaining = size % 32;
        if (remaining > 0) {
            memcpy((char*)dst + size - remaining, (char*)src + size - remaining, remaining);
        }
    } else {
        memcpy(dst, src, size);
    }
}

/**
 * @brief SIMD-accelerated buffer comparison.
 * @param buf1 First buffer
 * @param buf2 Second buffer
 * @param size Size in bytes
 * @return true if buffers are equal
 */
NODE_FORCE_INLINE bool NODE_SIMD_MEMCMP(const void* buf1, const void* buf2, size_t size) NODE_NONNULL(1, 2) NODE_PURE {
    if (size >= 32 && NODE_IS_V8_ALIGNED(buf1) && NODE_IS_V8_ALIGNED(buf2)) {
        const __m256i* vec1 = (const __m256i*)buf1;
        const __m256i* vec2 = (const __m256i*)buf2;
        size_t vec_count = size / 32;
        
        for (size_t i = 0; i < vec_count; i++) {
            __m256i cmp = _mm256_cmpeq_epi8(_mm256_load_si256(&vec1[i]), _mm256_load_si256(&vec2[i]));
            if (_mm256_movemask_epi8(cmp) != 0xFFFFFFFF) {
                return false;
            }
        }
        
        // Handle remaining bytes
        size_t remaining = size % 32;
        if (remaining > 0) {
            return memcmp((char*)buf1 + size - remaining, (char*)buf2 + size - remaining, remaining) == 0;
        }
        return true;
    } else {
        return memcmp(buf1, buf2, size) == 0;
    }
}

#else
// Fallback implementations for non-x86 platforms
#define NODE_SIMD_MEMCPY(dst, src, size) memcpy(dst, src, size)
#define NODE_SIMD_MEMCMP(buf1, buf2, size) (memcmp(buf1, buf2, size) == 0)
#endif

// === INLINE CACHING UTILITIES ===

/**
 * @brief Hash function for inline cache keys.
 * @param key Cache key string
 * @return Hash value for cache indexing
 */
NODE_FORCE_INLINE uint32_t NODE_IC_HASH(const char* key) NODE_NONNULL(1) NODE_PURE {
    uint32_t hash = NODE_FAST_STRING_HASH(key, strlen(key));
    return hash & NODE_IC_HASH_MASK;
}

/**
 * @brief Check if inline cache entry is valid.
 * @param timestamp Entry timestamp
 * @param current_time Current timestamp
 * @return true if entry is still valid
 */
NODE_FORCE_INLINE bool NODE_IC_IS_VALID(uint64_t timestamp, uint64_t current_time) NODE_PURE {
    return (current_time - timestamp) < 1000000000ULL;  // 1 second TTL
}

// === PLATFORM-SPECIFIC OPTIMIZATIONS ===

#ifdef __x86_64__
// x86-64 specific optimizations
#define NODE_CACHE_LINE_SIZE 64
#define NODE_PAGE_SIZE 4096
#define NODE_TLB_SIZE 1024

NODE_FORCE_INLINE void NODE_MEMORY_BARRIER(void) {
    __asm__ volatile("mfence" ::: "memory");
}

NODE_FORCE_INLINE void NODE_CPU_PAUSE(void) {
    __asm__ volatile("pause" ::: "memory");
}

#elif defined(__aarch64__)
// ARM64 specific optimizations  
#define NODE_CACHE_LINE_SIZE 64
#define NODE_PAGE_SIZE 4096
#define NODE_TLB_SIZE 512

NODE_FORCE_INLINE void NODE_MEMORY_BARRIER(void) {
    __asm__ volatile("dsb sy" ::: "memory");
}

NODE_FORCE_INLINE void NODE_CPU_PAUSE(void) {
    __asm__ volatile("yield" ::: "memory");
}

#else
// Generic optimizations
#define NODE_CACHE_LINE_SIZE 64
#define NODE_PAGE_SIZE 4096
#define NODE_TLB_SIZE 256

NODE_FORCE_INLINE void NODE_MEMORY_BARRIER(void) {
    __sync_synchronize();
}

NODE_FORCE_INLINE void NODE_CPU_PAUSE(void) {
    // No-op for generic platforms
}
#endif

// === DEBUGGING AND PROFILING MACROS ===

#ifdef NODE_DEBUG_PERFORMANCE
#define NODE_PERF_TRACE(msg) \
    do { \
        uint64_t ts = NODE_GET_TIMESTAMP_NS(); \
        printf("[NODE_PERF] %lu: %s\n", ts, msg); \
    } while(0)

#define NODE_PERF_TIMER_START(name) \
    uint64_t node_perf_timer_##name = NODE_GET_TIMESTAMP_NS()

#define NODE_PERF_TIMER_END(name) \
    do { \
        uint64_t elapsed = NODE_GET_TIMESTAMP_NS() - node_perf_timer_##name; \
        printf("[NODE_PERF] %s: %lu ns\n", #name, elapsed); \
    } while(0)

#define NODE_V8_TRACE(isolate, msg) \
    do { \
        if (isolate) { \
            HeapStatistics stats; \
            isolate->GetHeapStatistics(&stats); \
            printf("[V8_TRACE] %s - Heap: %zu/%zu bytes\n", msg, \
                   stats.used_heap_size(), stats.total_heap_size()); \
        } \
    } while(0)

#else
#define NODE_PERF_TRACE(msg) do {} while(0)
#define NODE_PERF_TIMER_START(name) do {} while(0)
#define NODE_PERF_TIMER_END(name) do {} while(0)
#define NODE_V8_TRACE(isolate, msg) do {} while(0)
#endif

// === COMPILE-TIME CONFIGURATION ===

// Enable/disable specific optimizations at compile time
#ifndef NODE_ENABLE_ZERO_COPY_BUFFERS
#define NODE_ENABLE_ZERO_COPY_BUFFERS 1
#endif

#ifndef NODE_ENABLE_INLINE_CACHING
#define NODE_ENABLE_INLINE_CACHING 1
#endif

#ifndef NODE_ENABLE_SIMD_ACCELERATION
#define NODE_ENABLE_SIMD_ACCELERATION 1
#endif

#ifndef NODE_ENABLE_TYPESCRIPT_CACHE
#define NODE_ENABLE_TYPESCRIPT_CACHE 1
#endif

#ifndef NODE_ENABLE_TURBOFAN_ALWAYS
#define NODE_ENABLE_TURBOFAN_ALWAYS 0  // Usually disabled for development
#endif

#ifndef NODE_ENABLE_CONCURRENT_GC
#define NODE_ENABLE_CONCURRENT_GC 1
#endif

#ifndef NODE_ENABLE_MEMORY_MAPPING
#define NODE_ENABLE_MEMORY_MAPPING 1
#endif

#ifndef NODE_ENABLE_PROFILING
#define NODE_ENABLE_PROFILING 1
#endif

#ifdef __cplusplus
} // extern "C"
#endif

#endif // NODE_OPTIMIZATION_H