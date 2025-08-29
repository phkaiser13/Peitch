/*
* Copyright (C) 2025 Pedro Henrique / phkaiser13
*
* SPDX-License-Identifier: Apache-2.0
*/

// CHANGE SUMMARY:
// - Added `StringDownloadResult` struct to handle results from downloads that return
//   a string in memory instead of writing to a file.
// - Declared the new function `download_to_string` to fetch a URL's content as a string.
// - Declared the new function `parse_github_latest_tag` to extract the version tag from
//   a GitHub API JSON response.

#ifndef PHPKG_DOWNLOADER_HPP
#define PHPKG_DOWNLOADER_HPP

#ifdef __cplusplus
extern "C" {
#endif

#include <stddef.h> // For size_t

/**
 * @enum DownloadStatusCode
 * @brief Defines status codes for the download operation.
 */
typedef enum {
    DOWNLOAD_SUCCESS = 0,
    DOWNLOAD_ERROR_GENERIC = 1,
    DOWNLOAD_ERROR_HTTP = 2,        // Indicates an HTTP error (e.g., 404, 500)
    DOWNLOAD_ERROR_NETWORK = 3,     // Indicates a network-level error (e.g., DNS failure)
    DOWNLOAD_ERROR_FILESYSTEM = 4,  // Indicates an error writing to the destination file
    DOWNLOAD_ERROR_INVALID_URL = 5,
    DOWNLOAD_ERROR_JSON_PARSE = 6   // Indicates an error parsing JSON content
} DownloadStatusCode;

/**
 * @struct DownloadResult
 * @brief Holds the result of a file download operation.
 */
typedef struct {
    DownloadStatusCode code;
    // A dynamically allocated string with details on the error.
    // It is the responsibility of the caller to free this memory.
    // Will be NULL on success.
    char* error_message;
} DownloadResult;

/**
 * @typedef download_progress_callback_t
 * @brief A function pointer type for download progress callbacks.
 */
typedef void (*download_progress_callback_t)(long long total_bytes, long long downloaded_bytes, void* user_data);

/**
 * @struct DownloadCallbacks
 * @brief A structure to hold optional callback functions for the downloader.
 */
typedef struct {
    download_progress_callback_t on_progress;
    void* user_data; // Opaque pointer passed to the callback
} DownloadCallbacks;

/**
 * @brief Downloads a file from a given URL to a specified destination path.
 */
DownloadResult download_file(const char* url, const char* destination_path, const DownloadCallbacks* callbacks);

/* BEGIN CHANGE: Add interface for downloading to a string and parsing JSON. */
/**
 * @struct StringDownloadResult
 * @brief Holds the result of a download-to-string operation.
 */
typedef struct {
    DownloadStatusCode code;
    // On success, a dynamically allocated string with the response body. NULL on error.
    // The caller is responsible for freeing this memory.
    char* response_body;
    // On error, a dynamically allocated string with error details. NULL on success.
    // The caller is responsible for freeing this memory.
    char* error_message;
} StringDownloadResult;

/**
 * @brief Downloads the content of a URL into a dynamically allocated string.
 *
 * @param url The fully-qualified URL to download.
 * @return A StringDownloadResult. The caller MUST check the status code and
 *         free either 'response_body' on success or 'error_message' on failure.
 */
StringDownloadResult download_to_string(const char* url);

/**
 * @brief Parses a JSON string from the GitHub 'latest release' API to find the tag name.
 *
 * @param json_string A null-terminated string containing the JSON payload.
 * @return A dynamically allocated string containing the 'tag_name' value, or NULL
 *         if parsing fails or the key is not found. The caller is responsible for
 *         freeing this memory.
 */
char* parse_github_latest_tag(const char* json_string);
/* END CHANGE */

#ifdef __cplusplus
} // extern "C"
#endif

#endif // PHPKG_DOWNLOADER_HPP