/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* packages.c
* This file implements the package database for the phpkg module. It contains a
* static, read-only array of 'Package' structs, defining every tool that phpkg
* can install. This approach provides a self-contained, dependency-free
* "catalog" of software. The metadata includes GitHub repository info and asset
* name patterns, which allows the installer to dynamically fetch the latest or
* a specific version of a tool by constructing download URLs at runtime.
* SPDX-License-Identifier: Apache-2.0 */

#include "packages.h"
#include <string.h>
#include <stddef.h>

// The master list of all installable packages.
// This is the "database" of the package manager.
// The asset patterns use '{VERSION}' which will be replaced at runtime
// with the specific release tag (e.g., "v2.14.0").
static const Package g_packages[] = {
    // 1. Versionamento / VCS
    {
        .name = "git", .category = "VCS",
        .github_repo = NULL, // Git is a foundational dependency
        .method = INSTALL_METHOD_SYSTEM_PACKAGE
    },
    {
        .name = "git-lfs", .category = "VCS",
        .github_repo = "git-lfs/git-lfs",
        .asset_pattern_linux_x64 = "git-lfs-linux-amd64-{VERSION}.tar.gz",
        .asset_pattern_windows_x64 = "git-lfs-windows-amd64-{VERSION}.zip",
        .asset_pattern_macos_x64 = "git-lfs-darwin-amd64-{VERSION}.tar.gz",
        .asset_pattern_macos_arm64 = "git-lfs-darwin-arm64-{VERSION}.tar.gz",
        .binary_path_in_archive = "git-lfs",
        .method = INSTALL_METHOD_DOWNLOAD_TARGZ // .zip on windows will be handled
    },
    // SVN, Mercurial, etc., are often best installed via system package managers
    { .name = "svn", .category = "VCS", .method = INSTALL_METHOD_SYSTEM_PACKAGE },
    { .name = "hg", .category = "VCS", .method = INSTALL_METHOD_SYSTEM_PACKAGE },
    // Perforce, TFVC, CVS, ClearCase are complex/legacy and outside the scope of simple binary downloads
    { .name = "p4", .category = "VCS", .method = INSTALL_METHOD_SYSTEM_PACKAGE },
    { .name = "tfvc", .category = "VCS", .method = INSTALL_METHOD_SYSTEM_PACKAGE },
    { .name = "cvs", .category = "VCS", .method = INSTALL_METHOD_SYSTEM_PACKAGE },
    { .name = "clearcase", .category = "VCS", .method = INSTALL_METHOD_SYSTEM_PACKAGE },

    // 2. Integração com providers
    {
        .name = "gh", .category = "Providers",
        .github_repo = "cli/cli",
        .asset_pattern_linux_x64 = "gh_{VERSION}_linux_amd64.tar.gz",
        .asset_pattern_windows_x64 = "gh_{VERSION}_windows_amd64.zip",
        .asset_pattern_macos_x64 = "gh_{VERSION}_macOS_amd64.zip",
        .asset_pattern_macos_arm64 = "gh_{VERSION}_macOS_arm64.zip",
        .binary_path_in_archive = "bin/gh",
        .method = INSTALL_METHOD_DOWNLOAD_TARGZ
    },
    // GitLab CLI 'glab'
    {
        .name = "gl", .category = "Providers",
        .github_repo = "gitlabhq/cli",
        .asset_pattern_linux_x64 = "glab_{VERSION}_Linux_x86_64.tar.gz",
        .asset_pattern_windows_x64 = "glab_{VERSION}_Windows_x86_64.zip",
        .asset_pattern_macos_x64 = "glab_{VERSION}_macOS_x86_64.tar.gz",
        .asset_pattern_macos_arm64 = "glab_{VERSION}_macOS_arm64.tar.gz",
        .binary_path_in_archive = "bin/glab",
        .method = INSTALL_METHOD_DOWNLOAD_TARGZ
    },
    // Atlassian CLI for Bitbucket/Jira
    {
        .name = "bb", .category = "Providers", // Assuming 'atlassian-cli'
        .github_repo = "atlassian/atlassian-cli",
        .asset_pattern_linux_x64 = "atlassian-cli-{VERSION}-linux-x64.tar.gz",
        .asset_pattern_windows_x64 = "atlassian-cli-{VERSION}-windows-x64.zip",
        .binary_path_in_archive = "atlassian-cli-{VERSION}/bin/bitbucket", // Example path
        .method = INSTALL_METHOD_DOWNLOAD_TARGZ
    },
    // Azure CLI is a complex install
    { .name = "az", .category = "Providers", .method = INSTALL_METHOD_SYSTEM_PACKAGE },
    {
        .name = "jira-cli", .category = "Providers",
        .github_repo = "ankitpokhrel/jira-cli",
        .asset_pattern_linux_x64 = "jira_{VERSION}_linux_x86_64.tar.gz",
        .asset_pattern_windows_x64 = "jira_{VERSION}_windows_x86_64.zip",
        .asset_pattern_macos_x64 = "jira_{VERSION}_macOS_x86_64.tar.gz",
        .asset_pattern_macos_arm64 = "jira_{VERSION}_macOS_arm64.tar.gz",
        .binary_path_in_archive = "bin/jira",
        .method = INSTALL_METHOD_DOWNLOAD_TARGZ
    },

    // 3. DevOps / CI/CD
    // Docker is a daemon, not just a CLI
    { .name = "docker", .category = "DevOps", .method = INSTALL_METHOD_SYSTEM_PACKAGE },
    { .name = "docker-compose", .category = "DevOps", .method = INSTALL_METHOD_SYSTEM_PACKAGE },
    {
        .name = "kubectl", .category = "DevOps",
        .github_repo = NULL, // Non-GitHub release
        .direct_url_template_linux_x64 = "https://dl.k8s.io/release/{VERSION}/bin/linux/amd64/kubectl",
        // ... other platforms follow similar patterns
        .method = INSTALL_METHOD_DOWNLOAD_BINARY
    },
    {
        .name = "helm", .category = "DevOps",
        .github_repo = "helm/helm",
        .asset_pattern_linux_x64 = "helm-{VERSION}-linux-amd64.tar.gz",
        .asset_pattern_windows_x64 = "helm-{VERSION}-windows-amd64.zip",
        .asset_pattern_macos_x64 = "helm-{VERSION}-darwin-amd64.tar.gz",
        .asset_pattern_macos_arm64 = "helm-{VERSION}-darwin-arm64.tar.gz",
        .binary_path_in_archive = "linux-amd64/helm", // Path varies by archive
        .method = INSTALL_METHOD_DOWNLOAD_TARGZ
    },
    {
        .name = "terraform", .category = "DevOps",
        .github_repo = "hashicorp/terraform",
        .asset_pattern_linux_x64 = "terraform_{VERSION}_linux_amd64.zip",
        .asset_pattern_windows_x64 = "terraform_{VERSION}_windows_amd64.zip",
        .asset_pattern_macos_x64 = "terraform_{VERSION}_darwin_amd64.zip",
        .asset_pattern_macos_arm64 = "terraform_{VERSION}_darwin_arm64.zip",
        .binary_path_in_archive = "terraform",
        .method = INSTALL_METHOD_DOWNLOAD_ZIP
    },
    { .name = "ansible", .category = "DevOps", .method = INSTALL_METHOD_SYSTEM_PACKAGE }, // Python-based
    {
        .name = "packer", .category = "DevOps",
        .github_repo = "hashicorp/packer",
        .asset_pattern_linux_x64 = "packer_{VERSION}_linux_amd64.zip",
        .asset_pattern_windows_x64 = "packer_{VERSION}_windows_amd64.zip",
        .asset_pattern_macos_x64 = "packer_{VERSION}_darwin_amd64.zip",
        .asset_pattern_macos_arm64 = "packer_{VERSION}_darwin_arm64.zip",
        .binary_path_in_archive = "packer",
        .method = INSTALL_METHOD_DOWNLOAD_ZIP
    },
    {
        .name = "vault", .category = "DevOps",
        .github_repo = "hashicorp/vault",
        .asset_pattern_linux_x64 = "vault_{VERSION}_linux_amd64.zip",
        .asset_pattern_windows_x64 = "vault_{VERSION}_windows_amd64.zip",
        .asset_pattern_macos_x64 = "vault_{VERSION}_darwin_amd64.zip",
        .asset_pattern_macos_arm64 = "vault_{VERSION}_darwin_arm64.zip",
        .binary_path_in_archive = "vault",
        .method = INSTALL_METHOD_DOWNLOAD_ZIP
    },

    // 4. Ferramentas de análise / auditoria / logs
    {
        .name = "trivy", .category = "Analysis",
        .github_repo = "aquasecurity/trivy",
        .asset_pattern_linux_x64 = "trivy_{VERSION}_Linux-64bit.tar.gz",
        .asset_pattern_windows_x64 = "trivy_{VERSION}_Windows-64bit.zip",
        .asset_pattern_macos_x64 = "trivy_{VERSION}_macOS-64bit.tar.gz",
        .asset_pattern_macos_arm64 = "trivy_{VERSION}_macOS-ARM64.tar.gz",
        .binary_path_in_archive = "trivy",
        .method = INSTALL_METHOD_DOWNLOAD_TARGZ
    },
    {
        .name = "cloc", .category = "Analysis",
        .github_repo = "AlDanial/cloc",
        .asset_pattern_linux_x64 = "cloc-{VERSION}.tar.gz", // Source tarball, needs build
        .method = INSTALL_METHOD_SYSTEM_PACKAGE // Simpler to use system package
    },
    {
        .name = "gitleaks", .category = "Analysis",
        .github_repo = "gitleaks/gitleaks",
        .asset_pattern_linux_x64 = "gitleaks_{VERSION}_linux_x64.tar.gz",
        .asset_pattern_windows_x64 = "gitleaks_{VERSION}_windows_x64.zip",
        .asset_pattern_macos_x64 = "gitleaks_{VERSION}_darwin_x64.tar.gz",
        .asset_pattern_macos_arm64 = "gitleaks_{VERSION}_darwin_arm64.tar.gz",
        .binary_path_in_archive = "gitleaks",
        .method = INSTALL_METHOD_DOWNLOAD_TARGZ
    },

    // 8. Auxiliares / produtividade
    {
        .name = "jq", .category = "Productivity",
        .github_repo = "jqlang/jq",
        // JQ has unique naming, jq-linux64, not versioned in name
        .asset_pattern_linux_x64 = "jq-linux64",
        .asset_pattern_windows_x64 = "jq-win64.exe",
        .asset_pattern_macos_x64 = "jq-osx-amd64",
        .asset_pattern_macos_arm64 = "jq-osx-arm64",
        .binary_path_in_archive = NULL, // It's a direct binary download
        .method = INSTALL_METHOD_DOWNLOAD_BINARY
    },
    {
        .name = "yq", .category = "Productivity",
        .github_repo = "mikefarah/yq",
        .asset_pattern_linux_x64 = "yq_linux_amd64",
        .asset_pattern_windows_x64 = "yq_windows_amd64.exe",
        .asset_pattern_macos_x64 = "yq_darwin_amd64",
        .asset_pattern_macos_arm64 = "yq_darwin_arm64",
        .binary_path_in_archive = NULL,
        .method = INSTALL_METHOD_DOWNLOAD_BINARY
    },
    {
        .name = "fzf", .category = "Productivity",
        .github_repo = "junegunn/fzf",
        .asset_pattern_linux_x64 = "fzf-{VERSION}-linux_amd64.tar.gz",
        .asset_pattern_windows_x64 = "fzf-{VERSION}-windows_amd64.zip",
        .asset_pattern_macos_x64 = "fzf-{VERSION}-darwin_amd64.tar.gz",
        .asset_pattern_macos_arm64 = "fzf-{VERSION}-darwin_arm64.tar.gz",
        .binary_path_in_archive = "fzf",
        .method = INSTALL_METHOD_DOWNLOAD_TARGZ
    },
    {
        .name = "bat", .category = "Productivity",
        .github_repo = "sharkdp/bat",
        .asset_pattern_linux_x64 = "bat-{VERSION}-x86_64-unknown-linux-gnu.tar.gz",
        .asset_pattern_windows_x64 = "bat-{VERSION}-x86_64-pc-windows-msvc.zip",
        .asset_pattern_macos_x64 = "bat-{VERSION}-x86_64-apple-darwin.tar.gz",
        .binary_path_in_archive = "bat-{VERSION}-x86_64-unknown-linux-gnu/bat",
        .method = INSTALL_METHOD_DOWNLOAD_TARGZ
    },
    {
        .name = "ripgrep", .category = "Productivity",
        .github_repo = "BurntSushi/ripgrep",
        .asset_pattern_linux_x64 = "ripgrep-{VERSION}-x86_64-unknown-linux-musl.tar.gz",
        .asset_pattern_windows_x64 = "ripgrep-{VERSION}-x86_64-pc-windows-msvc.zip",
        .asset_pattern_macos_x64 = "ripgrep-{VERSION}-x86_64-apple-darwin.tar.gz",
        .binary_path_in_archive = "ripgrep-{VERSION}-x86_64-unknown-linux-musl/rg",
        .method = INSTALL_METHOD_DOWNLOAD_TARGZ
    },
    {
        .name = "delta", .category = "Productivity",
        .github_repo = "dandavison/delta",
        .asset_pattern_linux_x64 = "delta-{VERSION}-x86_64-unknown-linux-gnu.tar.gz",
        .asset_pattern_windows_x64 = "delta-{VERSION}-x86_64-pc-windows-msvc.zip",
        .asset_pattern_macos_x64 = "delta-{VERSION}-x86_64-apple-darwin.tar.gz",
        .binary_path_in_archive = "delta-{VERSION}-x86_64-unknown-linux-gnu/delta",
        .method = INSTALL_METHOD_DOWNLOAD_TARGZ
    },

    // 9. Backup / snapshot / storage
    {
        .name = "rclone", .category = "Backup",
        .github_repo = "rclone/rclone",
        .asset_pattern_linux_x64 = "rclone-{VERSION}-linux-amd64.zip",
        .asset_pattern_windows_x64 = "rclone-{VERSION}-windows-amd64.zip",
        .asset_pattern_macos_x64 = "rclone-{VERSION}-osx-amd64.zip",
        .asset_pattern_macos_arm64 = "rclone-{VERSION}-osx-arm64.zip",
        .binary_path_in_archive = "rclone-{VERSION}-linux-amd64/rclone",
        .method = INSTALL_METHOD_DOWNLOAD_ZIP
    },
    {
        .name = "restic", .category = "Backup",
        .github_repo = "restic/restic",
        .asset_pattern_linux_x64 = "restic_{VERSION}_linux_amd64.bz2", // Note: bz2
        .method = INSTALL_METHOD_SYSTEM_PACKAGE // Complex archive type for now
    },
    // NOTE: Many other packages from the list (svn2git, pre-commit, etc.) are often
    // script-based (Python, Ruby) and best installed with their language's package
    // manager (pip, gem). They are omitted here to focus on binary distribution.
};

/**
 * @brief Finds a package by its unique name in the static package database.
 *
 * Iterates through the global `g_packages` array to find a matching entry.
 * The comparison is case-sensitive.
 *
 * @param name The name of the package to find.
 * @return A constant pointer to the found Package struct, or NULL if not found.
 */
const struct Package* find_package(const char* name) {
    if (name == NULL) {
        return NULL;
    }

    size_t num_packages = sizeof(g_packages) / sizeof(g_packages[0]);
    for (size_t i = 0; i < num_packages; ++i) {
        if (strcmp(g_packages[i].name, name) == 0) {
            return &g_packages[i];
        }
    }

    return NULL;
}