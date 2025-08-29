/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * utils.h - Interface for the common C utilities library.
 *
 * This header defines the public API for `libcommon`, a foundational library
 * providing safe, robust, and reusable utility functions for the entire C
 * codebase. Its purpose is to enforce the DRY (Don't Repeat Yourself)
 * principle and provide a single, trusted implementation for common tasks
 * like memory allocation and string manipulation.
 *
 * Every function is designed with safety as the primary concern, including
 * rigorous error checking and clear documentation regarding memory ownership.
 *
 * SPDX-License-Identifier: Apache-2.0 */

#ifndef LIBCOMMON_UTILS_H
#define LIBCOMMON_UTILS_H

#include <stddef.h> // For size_t

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @brief A safe string duplication function.
 *
 * This function behaves like the standard `strdup`, but with a critical
 * safety feature: it checks the return value of `malloc`. If memory allocation
 * fails, it prints a fatal error message to stderr and terminates the program.
 * This prevents the application from continuing in an unstable state.
 *
 * @param s The null-terminated string to duplicate.
 * @return A pointer to a newly allocated string.
 * @note The caller is responsible for freeing the returned pointer using `free()`.
 */
char* common_safe_strdup(const char* s);

/**
 * @brief Joins two path components using the correct platform-specific separator.
 *
 * This utility handles path joining intelligently, ensuring that exactly one
 * path separator exists between the two components, regardless of whether the
 * input components have trailing or leading separators.
 *
 * @param base The base path component (e.g., "/home/user/docs").
 * @param leaf The leaf path component (e.g., "file.txt").
 * @return A pointer to a newly allocated string containing the joined path.
 *         Returns NULL if memory allocation fails.
 * @note The caller is responsible for freeing the returned pointer using `free()`.
 */
char* common_path_join(const char* base, const char* leaf);

/**
 * @brief Reads the entire content of a file into a dynamically allocated string.
 *
 * This function opens, reads, and closes the specified file, returning its
 * entire content in a single buffer. The buffer is always null-terminated,
 * making it safe to use as a C string.
 *
 * @param filepath The path to the file to be read.
 * @param[out] file_size A pointer to a size_t variable where the size of the
 *                       file (in bytes, excluding the null terminator) will be
 *                       stored. Can be NULL if the size is not needed.
 * @return A pointer to a newly allocated buffer containing the file's content,
 *         or NULL if the file cannot be opened, read, or if memory allocation
 *         fails.
 * @note The caller is responsible for freeing the returned pointer using `free()`.
 */
char* common_read_file(const char* filepath, size_t* file_size);


#ifdef __cplusplus
} // extern "C"
#endif

#endif // LIBCOMMON_UTILS_H