# Copyright (C) 2025 Pedro Henrique / phkaiser13
# SPDX-License-Identifier: Apache-2.0
#
# Chocolatey installation script template for ph.
# Workflow must replace:
#   - k8s-prerls-0.0.3-beta           -> package version (e.g. 1.2.3)
#   -   -> sha256 checksum of the Windows installer .exe

$ErrorActionPreference = 'Stop'

# --- Package parameters (injected by CI/CD) ---
# Example: 1.2.3
$version = 'k8s-prerls-0.0.3-beta'

# installer file name and remote URL pattern
# Workflow should ensure the installer named like: ph-<version>-installer.exe exists in GitHub Releases
$installerFileName = "ph-$($version)-installer.exe"
$installerUrl = "https://github.com/phkaiser13/peitch/releases/download/v$($version)/$installerFileName"

# checksum injected by the workflow (sha256)
$checksum = ''
$checksumType = 'sha256'
$toolsDir = "$(Get-ToolsLocation)"

$packageArgs = @{
  packageName    = $env:ChocolateyPackageName
  fileType       = 'exe'
  url            = $installerUrl

  # --- Silent install args for a typical NSIS installer ---
  # Adjust silentArgs if your installer uses different flags.
  silentArgs     = "/S /D=`"$toolsDir`""

  validExitCodes = @(0)

  checksum       = $checksum
  checksumType   = $checksumType
}

# Download and run the installer (Chocolatey helper)
Install-ChocolateyPackage @packageArgs