/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* packages.h
* This header file defines the data structures and public interface for the
* package database. It specifies the 'Package' struct, which holds all
* metadata for a single installable tool. This design supports dynamic version
* resolution (e.g., "latest" or a specific version tag) by storing GitHub
* repository information and asset name patterns. The installer logic will use
* this metadata to construct the final download URL at runtime.
* SPDX-License-Identifier: Apache-2.0 */

#ifndef PHPKG_PACKAGES_H
#define PHPKG_PACKAGES_H

/**
 * @enum InstallMethod
 * @brief Defines the supported methods for installing a package.
 */
typedef enum {
    INSTALL_METHOD_DOWNLOAD_ZIP,
    INSTALL_METHOD_DOWNLOAD_TARGZ,
    INSTALL_METHOD_DOWNLOAD_BINARY,
    // A special type for packages that are too complex for simple binary drops
    // and should be installed via system package managers (e.g., apt, brew).
    INSTALL_METHOD_SYSTEM_PACKAGE 
} InstallMethod;

/**
 * @struct Package
 * @brief Represents a single installable package in the phpkg system.
 *
 * This structure is designed for dynamic URL construction. Instead of static URLs,
 * it stores the GitHub repository and patterns for asset names. The installer
 * will use these to query the GitHub API for a specific version (or the latest)
 * and build the correct download URL. The placeholder '{VERSION}' will be replaced
 * with the actual git tag (e.g., 'v1.2.3').
 */
typedef struct Package {
    const char* name;           // Unique identifier for the package (e.g., "git", "gh").
    const char* category;       // The category the package belongs to (e.g., "VCS", "DevOps").
    
    // For packages hosted on GitHub. Format: "owner/repo". If NULL, it's a non-GitHub package.
    const char* github_repo;

    // Asset name patterns. '{VERSION}' is replaced by the release tag.
    // Example: "gh_{VERSION}_linux_amd64.tar.gz"
    const char* asset_pattern_linux_x64;
    const char* asset_pattern_windows_x64;
    const char* asset_pattern_macos_x64;
    const char* asset_pattern_macos_arm64;

    // For non-GitHub packages, a full URL template. '{VERSION}' is still used.
    // Example: "https://dl.k8s.io/release/{VERSION}/bin/linux/amd64/kubectl"
    const char* direct_url_template_linux_x64;
    // ... add more for other platforms if needed for non-github packages

    // The relative path to the executable inside the archive.
    // Use the package name itself if it's at the root, or NULL for single binary downloads.
    const char* binary_path_in_archive;
    
    InstallMethod method;       // The installation method to be used.
} Package;


/**
 * @brief Finds a package by its unique name in the static package database.
 *
 * This function searches the internal list of available packages. The search
 * is case-sensitive.
 *
 * @param name The non-null, null-terminated string representing the package name.
 * @return A constant pointer to the found 'Package' struct. If no package
 *         with the given name is found, it returns NULL.
 */
const struct Package* find_package(const char* name);


#endif // PHPKG_PACKAGES_H