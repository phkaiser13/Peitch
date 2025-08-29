/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* installer.h
* This header file defines the public interface for the package installation
* orchestrator. It provides a single entry point, 'install_package', which
* handles the entire installation workflow: finding package metadata, resolving
* the correct version and download URL, downloading the artifact, extracting it,
* and placing the binary in the final destination. The interface is designed
* to be clear and provide detailed status feedback through the InstallStatus enum.
* SPDX-License-Identifier: Apache-2.0 */

#ifndef PHPKG_INSTALLER_H
#define PHPKG_INSTALLER_H

/**
 * @enum InstallStatus
 * @brief Defines the possible outcomes of an installation attempt.
 *
 * This provides a detailed status, allowing the caller to give precise
 * feedback to the user.
 */
typedef enum {
    INSTALL_SUCCESS = 0,
    INSTALL_ERROR_GENERIC = 1,
    INSTALL_ERROR_PACKAGE_NOT_FOUND = 2,
    INSTALL_ERROR_UNSUPPORTED_PLATFORM = 3,
    INSTALL_ERROR_VERSION_RESOLUTION = 4, // Failed to get 'latest' or parse version
    INSTALL_ERROR_DOWNLOAD = 5,
    INSTALL_ERROR_FILESYSTEM = 6,         // Failed to create dirs, move files, etc.
    INSTALL_ERROR_EXTRACTION = 7,         // Failed to unzip/untar the package
    INSTALL_ERROR_PERMISSION = 8,         // e.g., cannot write to install directory
    INSTALL_DELEGATED_TO_SYSTEM = 9       // For packages requiring a system package manager
} InstallStatus;

/**
 * @brief Installs a package by its name and a specified version.
 *
 * This is the main orchestration function. It performs the following steps:
 * 1. Finds the package metadata.
 * 2. Determines the correct asset for the current OS and architecture.
 * 3. Resolves the target version (fetching 'latest' from GitHub API if requested).
 * 4. Constructs the final download URL.
 * 5. Creates a temporary directory for the download.
 * 6. Calls the downloader with progress callbacks.
 * 7. Extracts the archive (if necessary).
 * 8. Moves the binary to the final installation path (e.g., ~/.ph/bin).
 * 9. Sets executable permissions.
 * 10. Cleans up all temporary files.
 *
 * @param package_name The unique name of the package to install (e.g., "terraform").
 * @param version_string The desired version. Can be a specific tag (e.g., "v1.8.3")
 *                       or "latest" to automatically find the most recent release.
 * @return An InstallStatus code indicating the outcome of the operation.
 */
InstallStatus install_package(const char* package_name, const char* version_string);

#endif // PHPKG_INSTALLER_H