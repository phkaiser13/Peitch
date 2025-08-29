/*
* Copyright (C) 2025 Pedro Henrique / phkaiser13
*
* SPDX-License-Identifier: Apache-2.0
*/

// CHANGE SUMMARY:
// - The `resolve_version` function has been completely refactored.
// - The fragile `popen` call to `curl | grep | sed` has been removed.
// - It now uses the new C++-backed functions `download_to_string` and
//   `parse_github_latest_tag` for a robust, native implementation.
// - This new approach correctly handles HTTP requests and parses the resulting
//   JSON to reliably extract the version tag, improving stability and removing
//   dependencies on external shell tools.
// - Memory management for the strings returned from the C++ layer is handled correctly.

#include "installer.h"
#include "packages.h"
#include "downloader.hpp" // C++ interface

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h> // For mkdir
#include <unistd.h>   // For access()

// Platform-specific definitions
#if defined(_WIN32) || defined(_WIN64)
    #define OS_WINDOWS
    #include <windows.h> // For CreateDirectory
#elif defined(__APPLE__)
    #define OS_MACOS
#elif defined(__linux__)
    #define OS_LINUX
#endif

#if defined(__x86_64__) || defined(_M_X64)
    #define ARCH_X64
#elif defined(__aarch64__) || defined(_M_ARM64)
    #define ARCH_ARM64
#endif

#define TEMP_DIR_TEMPLATE "/tmp/phpkg_XXXXXX"
#define INSTALL_DIR_ROOT ".ph/bin"

// Forward declarations for internal helper functions
static char* resolve_version(const Package* package, const char* version_string);
static void get_platform_asset_patterns(const Package* package, const char** asset_pattern, const char** binary_path);
static void simple_str_replace(char* target, size_t target_size, const char* search, const char* replace);
static void download_progress_callback(long long total, long long downloaded, void* user_data);
static int create_directory_recursive(const char* path);
static int cleanup_temp_dir(const char* path);

// --- Main Installation Logic ---

InstallStatus install_package(const char* package_name, const char* version_string) {
    printf("==> phpkg: Attempting to install '%s' version '%s'\n", package_name, version_string);

    // 1. Find Package Metadata
    const Package* package = find_package(package_name);
    if (!package) {
        fprintf(stderr, "Error: Package '%s' not found in the catalog.\n", package_name);
        return INSTALL_ERROR_PACKAGE_NOT_FOUND;
    }

    // 2. Handle System-Delegated Packages
    if (package->method == INSTALL_METHOD_SYSTEM_PACKAGE) {
        printf("Info: Package '%s' should be installed using your system's package manager (e.g., apt, brew, yum).\n", package_name);
        printf("      phpkg cannot manage this installation directly.\n");
        return INSTALL_DELEGATED_TO_SYSTEM;
    }

    // 3. Resolve the target version
    char* target_version = resolve_version(package, version_string);
    if (!target_version) {
        fprintf(stderr, "Error: Could not resolve version for '%s'.\n", package_name);
        return INSTALL_ERROR_VERSION_RESOLUTION;
    }
    printf("==> Resolved version: %s\n", target_version);

    // 4. Get platform-specific asset info
    const char* asset_pattern = NULL;
    const char* binary_path_in_archive_pattern = NULL;
    get_platform_asset_patterns(package, &asset_pattern, &binary_path_in_archive_pattern);

    if (!asset_pattern) {
        fprintf(stderr, "Error: Package '%s' is not available for your platform/architecture.\n", package_name);
        free(target_version);
        return INSTALL_ERROR_UNSUPPORTED_PLATFORM;
    }

    // 5. Construct final asset name and download URL
    char asset_name[256];
    strncpy(asset_name, asset_pattern, sizeof(asset_name) - 1);
    asset_name[sizeof(asset_name) - 1] = '\0';
    simple_str_replace(asset_name, sizeof(asset_name), "{VERSION}", target_version);

    char download_url[512];
    if (package->github_repo) {
        snprintf(download_url, sizeof(download_url), "https://github.com/%s/releases/download/%s/%s",
                package->github_repo, target_version, asset_name);
    } else if (package->direct_url_template_linux_x64) {
        strncpy(download_url, package->direct_url_template_linux_x64, sizeof(download_url) - 1);
        download_url[sizeof(download_url) - 1] = '\0';
        simple_str_replace(download_url, sizeof(download_url), "{VERSION}", target_version);
    } else {
        fprintf(stderr, "Error: Package '%s' has an invalid configuration (no repo or URL template).\n", package_name);
        free(target_version);
        return INSTALL_ERROR_GENERIC;
    }
    
    printf("==> Download URL: %s\n", download_url);

    // 6. Prepare temporary directory
    char temp_dir[] = TEMP_DIR_TEMPLATE;
    if (!mkdtemp(temp_dir)) {
        perror("Error creating temporary directory");
        free(target_version);
        return INSTALL_ERROR_FILESYSTEM;
    }
    printf("==> Using temporary directory: %s\n", temp_dir);

    char downloaded_file_path[512];
    snprintf(downloaded_file_path, sizeof(downloaded_file_path), "%s/%s", temp_dir, asset_name);

    // 7. Download the file
    printf("==> Downloading...\n");
    DownloadCallbacks callbacks = { .on_progress = download_progress_callback, .user_data = NULL };
    DownloadResult result = download_file(download_url, downloaded_file_path, &callbacks);
    printf("\n"); // Newline after progress bar

    if (result.code != DOWNLOAD_SUCCESS) {
        fprintf(stderr, "Error: Download failed. Reason: %s\n", result.error_message);
        free(result.error_message);
        cleanup_temp_dir(temp_dir);
        free(target_version);
        return INSTALL_ERROR_DOWNLOAD;
    }
    printf("==> Download complete.\n");

    // 8. Extract the archive
    if (package->method == INSTALL_METHOD_DOWNLOAD_ZIP || package->method == INSTALL_METHOD_DOWNLOAD_TARGZ) {
        printf("==> Extracting archive...\n");
        char command[1024];
        if (package->method == INSTALL_METHOD_DOWNLOAD_ZIP) {
            snprintf(command, sizeof(command), "unzip -q -o \"%s\" -d \"%s\"", downloaded_file_path, temp_dir);
        } else { // TARGZ
            snprintf(command, sizeof(command), "tar -xzf \"%s\" -C \"%s\"", downloaded_file_path, temp_dir);
        }

        if (system(command) != 0) {
            fprintf(stderr, "Error: Failed to extract archive.\n");
            cleanup_temp_dir(temp_dir);
            free(target_version);
            return INSTALL_ERROR_EXTRACTION;
        }
        printf("==> Extraction complete.\n");
    }

    // 9. Locate and install the binary
    char source_binary_path[1024];
    if (binary_path_in_archive_pattern) {
        char final_binary_path[512];
        strncpy(final_binary_path, binary_path_in_archive_pattern, sizeof(final_binary_path) - 1);
        final_binary_path[sizeof(final_binary_path) - 1] = '\0';
        simple_str_replace(final_binary_path, sizeof(final_binary_path), "{VERSION}", target_version);
        snprintf(source_binary_path, sizeof(source_binary_path), "%s/%s", temp_dir, final_binary_path);
    } else {
        strncpy(source_binary_path, downloaded_file_path, sizeof(source_binary_path));
    }

    char install_dir[512];
    snprintf(install_dir, sizeof(install_dir), "%s/%s", getenv("HOME"), INSTALL_DIR_ROOT);
    
    if (create_directory_recursive(install_dir) != 0) {
        fprintf(stderr, "Error: Could not create installation directory: %s\n", install_dir);
        cleanup_temp_dir(temp_dir);
        free(target_version);
        return INSTALL_ERROR_FILESYSTEM;
    }

    char dest_binary_path[1024];
    snprintf(dest_binary_path, sizeof(dest_binary_path), "%s/%s", install_dir, package->name);

    printf("==> Installing binary to %s\n", dest_binary_path);
    if (rename(source_binary_path, dest_binary_path) != 0) {
        perror("Error moving binary to installation directory");
        cleanup_temp_dir(temp_dir);
        free(target_version);
        return INSTALL_ERROR_FILESYSTEM;
    }

    // 10. Set executable permissions
    #ifndef OS_WINDOWS
    char chmod_cmd[512];
    snprintf(chmod_cmd, sizeof(chmod_cmd), "chmod +x \"%s\"", dest_binary_path);
    if (system(chmod_cmd) != 0) {
        fprintf(stderr, "Warning: Failed to set executable permission on binary.\n");
    }
    #endif

    // 11. Cleanup
    cleanup_temp_dir(temp_dir);
    free(target_version);

    printf("\n✅ Successfully installed '%s' (%s).\n", package_name, version_string);
    printf("   Make sure '%s' is in your PATH.\n", install_dir);

    return INSTALL_SUCCESS;
}

// --- Helper Function Implementations ---

/**
 * @brief Resolves the version string. If "latest", fetches from GitHub API.
 * @return A dynamically allocated string with the version tag (e.g., "v1.2.3"), or NULL on failure.
 *         The caller is responsible for freeing this memory.
 */
static char* resolve_version(const Package* package, const char* version_string) {
    if (strcmp(version_string, "latest") != 0) {
        return strdup(version_string); // Use the specified version
    }

    if (!package->github_repo) {
        fprintf(stderr, "Error: Cannot resolve 'latest' for non-GitHub package '%s'.\n", package->name);
        return NULL;
    }

    printf("==> Resolving latest version for %s from GitHub...\n", package->github_repo);

    /* BEGIN CHANGE: Replace popen with robust HTTP/JSON parsing */
    char api_url[512];
    snprintf(api_url, sizeof(api_url), "https://api.github.com/repos/%s/releases/latest", package->github_repo);

    // Step 1: Download the JSON response to a string
    StringDownloadResult json_result = download_to_string(api_url);

    if (json_result.code != DOWNLOAD_SUCCESS) {
        fprintf(stderr, "Error: Failed to fetch latest release info from GitHub API.\n");
        if (json_result.error_message) {
            fprintf(stderr, "       Reason: %s\n", json_result.error_message);
            free(json_result.error_message); // Clean up error message
        }
        return NULL;
    }

    // Step 2: Parse the JSON string to get the tag name
    char* version_tag = parse_github_latest_tag(json_result.response_body);

    // Step 3: Clean up the downloaded JSON string
    free(json_result.response_body);

    if (!version_tag) {
        fprintf(stderr, "Error: Could not find 'tag_name' in GitHub API response. The release might be malformed or the API has changed.\n");
        return NULL;
    }

    return version_tag; // This is an allocated string, caller will free it.
    /* END CHANGE */
}

/**
 * @brief Selects the correct asset and binary path patterns for the current platform.
 */
static void get_platform_asset_patterns(const Package* package, const char** asset_pattern, const char** binary_path) {
    *asset_pattern = NULL;
    *binary_path = package->binary_path_in_archive;

    #if defined(OS_LINUX) && defined(ARCH_X64)
        *asset_pattern = package->asset_pattern_linux_x64;
    #elif defined(OS_WINDOWS) && defined(ARCH_X64)
        *asset_pattern = package->asset_pattern_windows_x64;
    #elif defined(OS_MACOS) && defined(ARCH_X64)
        *asset_pattern = package->asset_pattern_macos_x64;
    #elif defined(OS_MACOS) && defined(ARCH_ARM64)
        *asset_pattern = package->asset_pattern_macos_arm64;
    #endif
}

/**
 * @brief A simple in-place string replacement function.
 */
static void simple_str_replace(char* target, size_t target_size, const char* search, const char* replace) {
    char* substr = strstr(target, search);
    if (!substr) return;

    size_t search_len = strlen(search);
    size_t replace_len = strlen(replace);
    size_t tail_len = strlen(substr + search_len);

    if (replace_len > search_len && (strlen(target) + replace_len - search_len) >= target_size) {
        return;
    }

    memmove(substr + replace_len, substr + search_len, tail_len + 1);
    memcpy(substr, replace, replace_len);
}

/**
 * @brief Callback for the downloader to display a progress bar.
 */
static void download_progress_callback(long long total, long long downloaded, void* user_data) {
    (void)user_data;
    if (total <= 0) return;

    int percentage = (int)((downloaded * 100) / total);
    int bar_width = 50;
    int pos = bar_width * percentage / 100;

    printf("\r[");
    for (int i = 0; i < bar_width; ++i) {
        if (i < pos) printf("=");
        else if (i == pos) printf(">");
        else printf(" ");
    }
    printf("] %d%% (%lld/%lld MB)", percentage, downloaded / (1024*1024), total / (1024*1024));
    fflush(stdout);
}

/**
 * @brief Creates a directory and all its parents, like `mkdir -p`.
 */
static int create_directory_recursive(const char* path) {
    char tmp[512];
    char *p = NULL;
    size_t len;

    snprintf(tmp, sizeof(tmp),"%s",path);
    len = strlen(tmp);
    if(tmp[len - 1] == '/') tmp[len - 1] = 0;

    for(p = tmp + 1; *p; p++) {
        if(*p == '/') {
            *p = 0;
            #ifdef OS_WINDOWS
                CreateDirectory(tmp, NULL);
            #else
                mkdir(tmp, S_IRWXU);
            #endif
            *p = '/';
        }
    }
    #ifdef OS_WINDOWS
        return CreateDirectory(tmp, NULL) ? 0 : -1;
    #else
        return mkdir(tmp, S_IRWXU);
    #endif
}

/**
 * @brief Cleans up (recursively deletes) the temporary directory.
 */
static int cleanup_temp_dir(const char* path) {
    char command[1024];
    #ifdef OS_WINDOWS
        snprintf(command, sizeof(command), "rmdir /s /q \"%s\"", path);
    #else
        snprintf(command, sizeof(command), "rm -rf \"%s\"", path);
    #endif
    printf("==> Cleaning up temporary directory: %s\n", path);
    return system(command);
}