# Copyright (C) 2025 Pedro Henrique / phkaiser13
# SPDX-License-Identifier: Apache-2.0
#
# RPM spec template for ph.
# Workflow must replace k8s-prerls-0.0.3-beta and/or https://github.com/phkaiser13/peitch/archive/refs/tags/vk8s-prerls-0.0.3-beta.tar.gz and can optionally fill Sun, 24 Aug 2025 22:28:35 +0000.
# Keep comments in English and prefer using RPM macros (e.g. %{version}) inside the spec body.

Name:       ph
# Workflow should replace this placeholder with the release version (e.g. 1.2.3)
Version:    k8s-prerls-0.0.3-beta
Release:    1%{?dist}
Summary:    The Polyglot Assistant for Git & DevOps Workflows

License:    Apache-2.0
URL:        https://github.com/phkaiser13/peitch

# Source tarball URL. The workflow can either set this directly, or use the default pattern below.
# Default pattern (workflow can generate): https://github.com/phkaiser13/peitch/archive/refs/tags/vk8s-prerls-0.0.3-beta.tar.gz
Source0:    https://github.com/phkaiser13/peitch/archive/refs/tags/vk8s-prerls-0.0.3-beta.tar.gz

# Build-time dependencies
BuildRequires:  gcc-c++
BuildRequires:  cmake
BuildRequires:  pkgconfig(lua)
BuildRequires:  pkgconfig(libcurl)
BuildRequires:  nlohmann-json-devel
BuildRequires:  rust
BuildRequires:  cargo

# Runtime dependencies
Requires:       libcurl
Requires:       lua

%description
A modern, extensible, and high-performance command-line tool designed to
unify and streamline your development lifecycle.

%prep
# autosetup will unpack Source0 and chdir into the unpacked dir.
# We assume the tarball extracts to peitch-<version>
%autosetup -n peitch-%{version}

%build
# Use the bundled CMake macros provided by rpmbuild
%cmake -DCMAKE_BUILD_TYPE=Release
%cmake_build

%install
%cmake_install

%files
%license LICENSE
%{_bindir}/ph
%dir %{_datadir}/ph
%{_datadir}/ph/modules/
%{_datadir}/ph/plugins/

%changelog
# The workflow may add a changelog entry here. Example placeholder below:
* Sun, 24 Aug 2025 22:28:35 +0000 Pedro Henrique <you@example.com> - k8s-prerls-0.0.3-beta-1
- Initial RPM package.