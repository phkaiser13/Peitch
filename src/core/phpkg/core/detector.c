/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * detector.c - Intelligent package manager detector for phpkg.
 * 
 * This module is responsible for detecting all available package managers
 * on the system, determining their availability, versions, and calculating
 * priorities based on OS, architecture, and package availability.
 * It provides the foundation for phpkg's meta-package management capabilities
 * by identifying which tools are available for package installation.
 * 
 * SPDX-License-Identifier: Apache-2.0 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdbool.h>
#include <sys/stat.h>

#ifdef _WIN32
    #include <windows.h>
    #define PATH_SEPARATOR ";"
    #define DIR_SEPARATOR "\\"
#else
    #include <unistd.h>
    #define PATH_SEPARATOR ":"
    #define DIR_SEPARATOR "/"
#endif

typedef enum {
    PM_HOMEBREW,
    PM_VCPKG,
    PM_CHOCOLATEY,
    PM_APT,
    PM_SNAP,
    PM_WINGET,
    PM_PACMAN,
    PM_DNF,
    PM_SCOOP,
    PM_MACPORTS,
    PM_NIX,
    PM_UNKNOWN
} PackageManagerType;

typedef enum {
    OS_WINDOWS,
    OS_MACOS,
    OS_LINUX,
    OS_BSD,
    OS_UNKNOWN
} OSType;

typedef enum {
    ARCH_X86,
    ARCH_X64,
    ARCH_ARM,
    ARCH_ARM64,
    ARCH_UNKNOWN
} Architecture;

typedef struct {
    PackageManagerType type;
    char name[64];
    char version[32];
    char path[512];
    char command[64];
    bool is_native;      // instalado via phpkg
    bool is_available;   // está funcionando
    int priority;        // 0-100, maior = preferido
    time_t detected_at;
} PackageManager;

typedef struct {
    PackageManager* managers;
    int count;
    int capacity;
    int preferred_index;
    OSType os;
    Architecture arch;
} DetectedManagers;

// Estrutura de detecção para cada gerenciador
typedef struct {
    PackageManagerType type;
    const char* name;
    const char* executable;
    const char* test_command;
    const char* version_command;
    const char* common_paths[8];
    OSType supported_os[4];
} DetectorConfig;

static const DetectorConfig DETECTORS[] = {
    {
        .type = PM_HOMEBREW,
        .name = "homebrew",
        .executable = "brew",
        .test_command = "brew --version",
        .version_command = "brew --version | head -1",
        .common_paths = {"/usr/local/bin", "/opt/homebrew/bin", "/home/linuxbrew/.linuxbrew/bin", NULL},
        .supported_os = {OS_MACOS, OS_LINUX, OS_UNKNOWN}
    },
    {
        .type = PM_VCPKG,
        .name = "vcpkg",
        .executable = "vcpkg",
        .test_command = "vcpkg version",
        .version_command = "vcpkg version | head -1",
        .common_paths = {"C:\\vcpkg", "C:\\tools\\vcpkg", "/usr/local/vcpkg", NULL},
        .supported_os = {OS_WINDOWS, OS_LINUX, OS_MACOS, OS_UNKNOWN}
    },
    {
        .type = PM_CHOCOLATEY,
        .name = "chocolatey",
        .executable = "choco",
        .test_command = "choco --version",
        .version_command = "choco --version",
        .common_paths = {"C:\\ProgramData\\chocolatey\\bin", NULL},
        .supported_os = {OS_WINDOWS, OS_UNKNOWN}
    },
    {
        .type = PM_APT,
        .name = "apt",
        .executable = "apt",
        .test_command = "apt --version",
        .version_command = "apt --version | head -1",
        .common_paths = {"/usr/bin", "/bin", NULL},
        .supported_os = {OS_LINUX, OS_UNKNOWN}
    },
    {
        .type = PM_SNAP,
        .name = "snap",
        .executable = "snap",
        .test_command = "snap version",
        .version_command = "snap version | grep snap",
        .common_paths = {"/usr/bin", "/snap/bin", NULL},
        .supported_os = {OS_LINUX, OS_UNKNOWN}
    },
    {
        .type = PM_WINGET,
        .name = "winget",
        .executable = "winget",
        .test_command = "winget --version",
        .version_command = "winget --version",
        .common_paths = {"C:\\Users\\%USERNAME%\\AppData\\Local\\Microsoft\\WindowsApps", NULL},
        .supported_os = {OS_WINDOWS, OS_UNKNOWN}
    },
    {
        .type = PM_SCOOP,
        .name = "scoop",
        .executable = "scoop",
        .test_command = "scoop --version",
        .version_command = "scoop --version",
        .common_paths = {"C:\\Users\\%USERNAME%\\scoop\\shims", NULL},
        .supported_os = {OS_WINDOWS, OS_UNKNOWN}
    },
    {
        .type = PM_PACMAN,
        .name = "pacman",
        .executable = "pacman",
        .test_command = "pacman --version",
        .version_command = "pacman --version | head -1",
        .common_paths = {"/usr/bin", NULL},
        .supported_os = {OS_LINUX, OS_UNKNOWN}
    },
    {
        .type = PM_NIX,
        .name = "nix",
        .executable = "nix",
        .test_command = "nix --version",
        .version_command = "nix --version",
        .common_paths = {"/nix/var/nix/profiles/default/bin", "/usr/bin", NULL},
        .supported_os = {OS_LINUX, OS_MACOS, OS_UNKNOWN}
    }
};

// Detecta o OS atual
static OSType detect_os() {
    #ifdef _WIN32
        return OS_WINDOWS;
    #elif __APPLE__
        return OS_MACOS;
    #elif __linux__
        return OS_LINUX;
    #elif __FreeBSD__ || __OpenBSD__ || __NetBSD__
        return OS_BSD;
    #else
        return OS_UNKNOWN;
    #endif
}

// Detecta a arquitetura
static Architecture detect_architecture() {
    #if defined(__x86_64__) || defined(_M_X64)
        return ARCH_X64;
    #elif defined(__i386__) || defined(_M_IX86)
        return ARCH_X86;
    #elif defined(__aarch64__) || defined(_M_ARM64)
        return ARCH_ARM64;
    #elif defined(__arm__) || defined(_M_ARM)
        return ARCH_ARM;
    #else
        return ARCH_UNKNOWN;
    #endif
}

// Verifica se um arquivo existe
static bool file_exists(const char* path) {
    struct stat buffer;
    return (stat(path, &buffer) == 0);
}

// Expande variáveis de ambiente no path
static char* expand_path(const char* path) {
    static char expanded[1024];
    
    #ifdef _WIN32
        ExpandEnvironmentStringsA(path, expanded, sizeof(expanded));
    #else
        // Simple expansion for Unix - expandir $HOME, etc
        if (strstr(path, "$HOME")) {
            char* home = getenv("HOME");
            if (home) {
                snprintf(expanded, sizeof(expanded), "%s%s", home, path + 5);
                return expanded;
            }
        }
        strncpy(expanded, path, sizeof(expanded) - 1);
    #endif
    
    return expanded;
}

// Testa se um comando está disponível
static bool test_command(const char* command) {
    char buffer[256];
    
    #ifdef _WIN32
        snprintf(buffer, sizeof(buffer), "%s >nul 2>&1", command);
    #else
        snprintf(buffer, sizeof(buffer), "command -v %s >/dev/null 2>&1", command);
    #endif
    
    return (system(buffer) == 0);
}

// Obtém a versão de um gerenciador
static void get_version(const char* version_cmd, char* output, size_t size) {
    FILE* fp = popen(version_cmd, "r");
    if (fp == NULL) {
        strncpy(output, "unknown", size);
        return;
    }
    
    if (fgets(output, size, fp) == NULL) {
        strncpy(output, "unknown", size);
    }
    
    // Remove newline
    output[strcspn(output, "\n")] = 0;
    pclose(fp);
}

// Busca um executável no PATH
static char* find_in_path(const char* executable) {
    static char fullpath[1024];
    char* path_env = getenv("PATH");
    
    if (!path_env) return NULL;
    
    char* path_copy = strdup(path_env);
    char* token = strtok(path_copy, PATH_SEPARATOR);
    
    while (token) {
        snprintf(fullpath, sizeof(fullpath), "%s%s%s", token, DIR_SEPARATOR, executable);
        
        #ifdef _WIN32
            // No Windows, tenta com .exe também
            char with_exe[1024];
            snprintf(with_exe, sizeof(with_exe), "%s.exe", fullpath);
            if (file_exists(with_exe)) {
                free(path_copy);
                return fullpath;
            }
        #endif
        
        if (file_exists(fullpath)) {
            free(path_copy);
            return fullpath;
        }
        
        token = strtok(NULL, PATH_SEPARATOR);
    }
    
    free(path_copy);
    return NULL;
}

// Detecta um gerenciador específico
static PackageManager* detect_single_manager(const DetectorConfig* config, OSType current_os) {
    // Verifica se o OS é suportado
    bool os_supported = false;
    for (int i = 0; i < 4 && config->supported_os[i] != OS_UNKNOWN; i++) {
        if (config->supported_os[i] == current_os) {
            os_supported = true;
            break;
        }
    }
    
    if (!os_supported) return NULL;
    
    PackageManager* pm = calloc(1, sizeof(PackageManager));
    pm->type = config->type;
    strncpy(pm->name, config->name, sizeof(pm->name) - 1);
    strncpy(pm->command, config->executable, sizeof(pm->command) - 1);
    
    // Primeiro tenta encontrar no PATH
    char* path = find_in_path(config->executable);
    if (path) {
        strncpy(pm->path, path, sizeof(pm->path) - 1);
        pm->is_available = test_command(config->test_command);
        
        if (pm->is_available) {
            get_version(config->version_command, pm->version, sizeof(pm->version));
            pm->detected_at = time(NULL);
            return pm;
        }
    }
    
    // Tenta paths comuns
    for (int i = 0; i < 8 && config->common_paths[i]; i++) {
        char* expanded = expand_path(config->common_paths[i]);
        snprintf(pm->path, sizeof(pm->path), "%s%s%s", 
                 expanded, DIR_SEPARATOR, config->executable);
        
        if (file_exists(pm->path)) {
            pm->is_available = test_command(config->test_command);
            
            if (pm->is_available) {
                get_version(config->version_command, pm->version, sizeof(pm->version));
                pm->detected_at = time(NULL);
                return pm;
            }
        }
    }
    
    free(pm);
    return NULL;
}

// Calcula prioridade baseado em OS e arquitetura
static int calculate_priority(PackageManagerType type, OSType os, Architecture arch) {
    int priority = 50; // Base
    
    // Ajusta por OS
    switch (os) {
        case OS_WINDOWS:
            if (type == PM_VCPKG) priority = 90;
            else if (type == PM_CHOCOLATEY) priority = 85;
            else if (type == PM_WINGET) priority = 80;
            else if (type == PM_SCOOP) priority = 75;
            break;
            
        case OS_MACOS:
            if (type == PM_HOMEBREW) priority = 95;
            else if (type == PM_MACPORTS) priority = 70;
            else if (type == PM_NIX) priority = 60;
            break;
            
        case OS_LINUX:
            if (type == PM_APT) priority = 85;
            else if (type == PM_SNAP) priority = 75;
            else if (type == PM_PACMAN) priority = 80;
            else if (type == PM_DNF) priority = 80;
            else if (type == PM_HOMEBREW) priority = 70;
            else if (type == PM_NIX) priority = 65;
            break;
    }
    
    // Ajusta por arquitetura (vcpkg é melhor para desenvolvimento cross-platform)
    if (arch == ARCH_X64 && type == PM_VCPKG) {
        priority += 5;
    }
    
    return priority;
}

// Função principal de detecção
DetectedManagers* detect_all_managers() {
    DetectedManagers* dm = calloc(1, sizeof(DetectedManagers));
    dm->capacity = 10;
    dm->managers = calloc(dm->capacity, sizeof(PackageManager));
    dm->count = 0;
    dm->os = detect_os();
    dm->arch = detect_architecture();
    dm->preferred_index = -1;
    
    int max_priority = -1;
    
    // Detecta todos os gerenciadores
    for (size_t i = 0; i < sizeof(DETECTORS) / sizeof(DETECTORS[0]); i++) {
        PackageManager* pm = detect_single_manager(&DETECTORS[i], dm->os);
        
        if (pm) {
            // Calcula prioridade
            pm->priority = calculate_priority(pm->type, dm->os, dm->arch);
            
            // Adiciona à lista
            if (dm->count >= dm->capacity) {
                dm->capacity *= 2;
                dm->managers = realloc(dm->managers, dm->capacity * sizeof(PackageManager));
            }
            
            memcpy(&dm->managers[dm->count], pm, sizeof(PackageManager));
            
            // Atualiza preferido
            if (pm->priority > max_priority) {
                max_priority = pm->priority;
                dm->preferred_index = dm->count;
            }
            
            dm->count++;
            free(pm);
        }
    }
    
    return dm;
}

// Libera memória
void free_detected_managers(DetectedManagers* dm) {
    if (dm) {
        free(dm->managers);
        free(dm);
    }
}

// Busca um gerenciador específico
PackageManager* find_manager_by_type(DetectedManagers* dm, PackageManagerType type) {
    for (int i = 0; i < dm->count; i++) {
        if (dm->managers[i].type == type) {
            return &dm->managers[i];
        }
    }
    return NULL;
}

// Busca gerenciador por nome (suporta aliases)
PackageManager* find_manager_by_name(DetectedManagers* dm, const char* name) {
    for (int i = 0; i < dm->count; i++) {
        if (strcasecmp(dm->managers[i].name, name) == 0) {
            return &dm->managers[i];
        }
        
        // Aliases comuns
        if (strcasecmp(name, "hb") == 0 && dm->managers[i].type == PM_HOMEBREW) {
            return &dm->managers[i];
        }
        if (strcasecmp(name, "vc") == 0 && dm->managers[i].type == PM_VCPKG) {
            return &dm->managers[i];
        }
        if (strcasecmp(name, "choco") == 0 && dm->managers[i].type == PM_CHOCOLATEY) {
            return &dm->managers[i];
        }
    }
    return NULL;
}

// Obtém o gerenciador preferido
PackageManager* get_preferred_manager(DetectedManagers* dm) {
    if (dm->preferred_index >= 0 && dm->preferred_index < dm->count) {
        return &dm->managers[dm->preferred_index];
    }
    return NULL;
}

// Função de debug - imprime gerenciadores detectados
void print_detected_managers(DetectedManagers* dm) {
    printf("=== Detected Package Managers ===\n");
    printf("OS: %d, Architecture: %d\n", dm->os, dm->arch);
    printf("Found %d package managers:\n\n", dm->count);
    
    for (int i = 0; i < dm->count; i++) {
        PackageManager* pm = &dm->managers[i];
        printf("[%d] %s%s\n", i + 1, pm->name, 
               (i == dm->preferred_index) ? " (preferred)" : "");
        printf("    Version: %s\n", pm->version);
        printf("    Path: %s\n", pm->path);
        printf("    Priority: %d\n", pm->priority);
        printf("    Available: %s\n", pm->is_available ? "yes" : "no");
        printf("\n");
    }
}

// Exemplo de uso
#ifdef DETECTOR_TEST
int main() {
    printf("Starting package manager detection...\n\n");
    
    DetectedManagers* dm = detect_all_managers();
    print_detected_managers(dm);
    
    PackageManager* preferred = get_preferred_manager(dm);
    if (preferred) {
        printf("Recommended package manager: %s\n", preferred->name);
    }
    
    free_detected_managers(dm);
    return 0;
}
#endif