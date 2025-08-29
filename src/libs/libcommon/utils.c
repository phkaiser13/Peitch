/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * utils.c - Implementation of the common C utilities library.
 *
 * This file provides the concrete implementations for the functions declared
 * in utils.h. Each function is written defensively, with extensive checks for
 * potential errors such as null pointers and memory allocation failures.
 *
 * The implementations prioritize safety, clarity, and efficiency, forming a
 * reliable foundation for the rest of the C-based components of the application.
 *
 * SPDX-License-Identifier: Apache-2.0 */

#include "utils.h"
#include "platform/platform.h" // For PATH_SEPARATOR
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/**
 * @see utils.h
 */
char* common_safe_strdup(const char* s) {
    if (!s) {
        return NULL;
    }
    size_t len = strlen(s) + 1;
    char* new_str = malloc(len);
    if (!new_str) {
        // Fail-fast strategy: If we can't allocate a simple string, the system
        // is likely in a critical state. Terminate immediately.
        fprintf(stderr, "FATAL: Memory allocation failed in common_safe_strdup. Exiting.\n");
        exit(EXIT_FAILURE);
    }
    memcpy(new_str, s, len);
    return new_str;
}

/**
 * @see utils.h
 */
char* common_path_join(const char* base, const char* leaf) {
    if (!base || !leaf) {
        return NULL;
    }

    size_t base_len = strlen(base);
    size_t leaf_len = strlen(leaf);

    // Handle trailing/leading separators to ensure only one exists.
    int base_has_sep = (base_len > 0 && base[base_len - 1] == PATH_SEPARATOR);
    int leaf_has_sep = (leaf_len > 0 && leaf[0] == PATH_SEPARATOR);

    // Calculate the exact length needed.
    // Base length + Leaf length + Separator (if needed) + Null terminator
    size_t total_len = base_len + leaf_len + 2;
    if (base_has_sep && leaf_has_sep) {
        total_len -= 1; // We'll remove one separator
    } else if (!base_has_sep && !leaf_has_sep) {
        // No change needed, space is for the new separator
    } else {
        total_len -= 1; // One exists, so no new one needed
    }

    char* result = malloc(total_len);
    if (!result) {
        return NULL; // Caller must handle this potential failure.
    }

    // Build the string safely.
    strcpy(result, base);

    if (base_has_sep && leaf_has_sep) {
        // Skip the separator on the leaf.
        strcat(result, leaf + 1);
    } else if (!base_has_sep && !leaf_has_sep) {
        // Add the separator.
        char sep_str[2] = {PATH_SEPARATOR, '\0'};
        strcat(result, sep_str);
        strcat(result, leaf);
    } else {
        // One of them has a separator, so just concatenate.
        strcat(result, leaf);
    }

    return result;
}

/**
 * @see utils.h
 */
char* common_read_file(const char* filepath, size_t* file_size) {
    if (!filepath) {
        return NULL;
    }

    // Open the file in binary read mode.
    FILE* f = fopen(filepath, "rb");
    if (!f) {
        return NULL;
    }

    // Seek to the end to determine the file size.
    fseek(f, 0, SEEK_END);
    long size = ftell(f);
    if (size < 0) {
        fclose(f);
        return NULL;
    }
    rewind(f); // Go back to the beginning.

    // Allocate a buffer of the perfect size + 1 for the null terminator.
    char* buffer = malloc(size + 1);
    if (!buffer) {
        fclose(f);
        return NULL;
    }

    // Read the entire file into the buffer in one go.
    size_t bytes_read = fread(buffer, 1, size, f);
    if (bytes_read < (size_t)size) {
        // If we couldn't read the whole file, it's an error.
        fclose(f);
        free(buffer);
        return NULL;
    }

    // The contract: always null-terminate the buffer.
    buffer[size] = '\0';

    if (file_size) {
        *file_size = (size_t)size;
    }

    fclose(f);
    return buffer;
}