/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * resolver.c - Ultra-intelligent package name resolver for phpkg.
 * 
 * This module implements revolutionary package resolution algorithms that can
 * intelligently map package names across different package managers. It uses
 * fuzzy matching, contextual analysis, and self-learning mechanisms to resolve
 * package names even when they differ across platforms. The resolver maintains
 * a database of package aliases and learns from successful resolutions to
 * improve accuracy over time.
 * 
 * SPDX-License-Identifier: Apache-2.0 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdbool.h>
#include <ctype.h>
#include <math.h>
#include <time.h>

#ifdef _WIN32
    #include <windows.h>
    #define strcasecmp _stricmp
#else
    #include <unistd.h>
    #include <strings.h>
#endif

/* Package mapping structures */
typedef struct {
    char canonical_name[128];      /* Standard name */
    char description[256];         /* Package description */
    char* homebrew_name;
    char* vcpkg_name;
    char* choco_name;
    char* apt_name;
    char* snap_name;
    char* winget_name;
    char* pacman_name;
    char* nix_name;
    char* scoop_name;
    char** tags;                  /* Keywords for matching */
    int tags_count;
    float confidence;              /* Confidence score 0-1 */
    time_t last_used;
} PackageAlias;

typedef struct {
    PackageAlias* aliases;
    int count;
    int capacity;
    time_t last_update;
    char db_path[512];
} PackageDatabase;

typedef struct {
    char query[128];
    char resolved_name[128];
    char manager_name[64];
    float confidence;
    time_t timestamp;
} LearnedMapping;

typedef struct {
    LearnedMapping* mappings;
    int count;
    int capacity;
} LearningCache;

/* Search result structure */
typedef struct {
    char package_name[128];
    char manager_name[64];
    float match_score;
    bool is_exact;
    bool is_available;
} SearchResult;

typedef struct {
    SearchResult* results;
    int count;
    int capacity;
} SearchResults;

/* Global databases */
static PackageDatabase* g_package_db = NULL;
static LearningCache* g_learning_cache = NULL;

/* Common package mappings - hardcoded for essential packages */
static const PackageAlias COMMON_PACKAGES[] = {
    {
        .canonical_name = "curl",
        .description = "Command line tool for transferring data with URLs",
        .homebrew_name = "curl",
        .vcpkg_name = "curl",
        .choco_name = "curl",
        .apt_name = "curl",
        .snap_name = "curl",
        .winget_name = "cURL.cURL",
        .pacman_name = "curl",
        .nix_name = "curl",
        .scoop_name = "curl",
        .confidence = 1.0
    },
    {
        .canonical_name = "git",
        .description = "Distributed version control system",
        .homebrew_name = "git",
        .vcpkg_name = "git",
        .choco_name = "git",
        .apt_name = "git",
        .snap_name = "git",
        .winget_name = "Git.Git",
        .pacman_name = "git",
        .nix_name = "git",
        .scoop_name = "git",
        .confidence = 1.0
    },
    {
        .canonical_name = "cmake",
        .description = "Cross-platform build system generator",
        .homebrew_name = "cmake",
        .vcpkg_name = "cmake",
        .choco_name = "cmake",
        .apt_name = "cmake",
        .snap_name = "cmake",
        .winget_name = "Kitware.CMake",
        .pacman_name = "cmake",
        .nix_name = "cmake",
        .scoop_name = "cmake",
        .confidence = 1.0
    },
    {
        .canonical_name = "boost",
        .description = "C++ libraries collection",
        .homebrew_name = "boost",
        .vcpkg_name = "boost",
        .choco_name = "boost-msvc-14.1",
        .apt_name = "libboost-all-dev",
        .snap_name = NULL,
        .winget_name = NULL,
        .pacman_name = "boost",
        .nix_name = "boost",
        .scoop_name = "boost",
        .confidence = 1.0
    },
    {
        .canonical_name = "openssl",
        .description = "Cryptography and SSL/TLS toolkit",
        .homebrew_name = "openssl",
        .vcpkg_name = "openssl",
        .choco_name = "openssl",
        .apt_name = "openssl",
        .snap_name = NULL,
        .winget_name = "ShiningLight.OpenSSL",
        .pacman_name = "openssl",
        .nix_name = "openssl",
        .scoop_name = "openssl",
        .confidence = 1.0
    },
    {
        .canonical_name = "json",
        .description = "JSON for Modern C++ (nlohmann)",
        .homebrew_name = "nlohmann-json",
        .vcpkg_name = "nlohmann-json",
        .choco_name = NULL,
        .apt_name = "nlohmann-json3-dev",
        .snap_name = NULL,
        .winget_name = NULL,
        .pacman_name = "nlohmann-json",
        .nix_name = "nlohmann_json",
        .scoop_name = NULL,
        .confidence = 0.9
    },
    {
        .canonical_name = "zlib",
        .description = "Compression library",
        .homebrew_name = "zlib",
        .vcpkg_name = "zlib",
        .choco_name = "zlib",
        .apt_name = "zlib1g-dev",
        .snap_name = NULL,
        .winget_name = NULL,
        .pacman_name = "zlib",
        .nix_name = "zlib",
        .scoop_name = "zlib",
        .confidence = 1.0
    }
};

/* Levenshtein distance for fuzzy matching */
static int levenshtein_distance(const char* s1, const char* s2) {
    int len1 = strlen(s1);
    int len2 = strlen(s2);
    
    int* matrix = calloc((len1 + 1) * (len2 + 1), sizeof(int));
    
    for (int i = 0; i <= len1; i++) {
        matrix[i * (len2 + 1)] = i;
    }
    
    for (int j = 0; j <= len2; j++) {
        matrix[j] = j;
    }
    
    for (int i = 1; i <= len1; i++) {
        for (int j = 1; j <= len2; j++) {
            int cost = (tolower(s1[i-1]) == tolower(s2[j-1])) ? 0 : 1;
            
            int deletion = matrix[(i-1) * (len2 + 1) + j] + 1;
            int insertion = matrix[i * (len2 + 1) + (j-1)] + 1;
            int substitution = matrix[(i-1) * (len2 + 1) + (j-1)] + cost;
            
            matrix[i * (len2 + 1) + j] = fmin(fmin(deletion, insertion), substitution);
        }
    }
    
    int distance = matrix[len1 * (len2 + 1) + len2];
    free(matrix);
    return distance;
}

/* Calculate similarity ratio (0-1) */
static float similarity_ratio(const char* s1, const char* s2) {
    int distance = levenshtein_distance(s1, s2);
    int max_len = fmax(strlen(s1), strlen(s2));
    
    if (max_len == 0) return 1.0;
    return 1.0 - ((float)distance / max_len);
}

/* Advanced pattern matching with context awareness */
static float advanced_match_score(const char* query, const PackageAlias* pkg) {
    float name_score = 0.0;
    float desc_score = 0.0;
    float tag_score = 0.0;
    
    /* Direct name matching */
    name_score = similarity_ratio(query, pkg->canonical_name);
    
    /* Check all package manager specific names */
    const char* names[] = {
        pkg->homebrew_name, pkg->vcpkg_name, pkg->choco_name,
        pkg->apt_name, pkg->snap_name, pkg->winget_name,
        pkg->pacman_name, pkg->nix_name, pkg->scoop_name
    };
    
    for (int i = 0; i < 9; i++) {
        if (names[i]) {
            float score = similarity_ratio(query, names[i]);
            if (score > name_score) name_score = score;
        }
    }
    
    /* Description matching (lower weight) */
    if (pkg->description[0]) {
        char lower_desc[256];
        char lower_query[128];
        
        /* Convert to lowercase for comparison */
        for (int i = 0; pkg->description[i] && i < 255; i++) {
            lower_desc[i] = tolower(pkg->description[i]);
        }
        for (int i = 0; query[i] && i < 127; i++) {
            lower_query[i] = tolower(query[i]);
        }
        
        if (strstr(lower_desc, lower_query)) {
            desc_score = 0.5;
        }
    }
    
    /* Tag matching */
    if (pkg->tags && pkg->tags_count > 0) {
        for (int i = 0; i < pkg->tags_count; i++) {
            float tag_sim = similarity_ratio(query, pkg->tags[i]);
            if (tag_sim > tag_score) tag_score = tag_sim;
        }
    }
    
    /* Weighted combination */
    return (name_score * 0.7) + (desc_score * 0.2) + (tag_score * 0.1);
}

/* Initialize package database */
PackageDatabase* init_package_database() {
    PackageDatabase* db = calloc(1, sizeof(PackageDatabase));
    db->capacity = 100;
    db->aliases = calloc(db->capacity, sizeof(PackageAlias));
    db->count = 0;
    db->last_update = time(NULL);
    
    /* Load common packages */
    int common_count = sizeof(COMMON_PACKAGES) / sizeof(COMMON_PACKAGES[0]);
    for (int i = 0; i < common_count; i++) {
        memcpy(&db->aliases[db->count++], &COMMON_PACKAGES[i], sizeof(PackageAlias));
    }
    
    /* Set database path */
    #ifdef _WIN32
        snprintf(db->db_path, sizeof(db->db_path), 
                 "%s\\phpkg\\packages.db", getenv("APPDATA"));
    #else
        snprintf(db->db_path, sizeof(db->db_path), 
                 "%s/.config/phpkg/packages.db", getenv("HOME"));
    #endif
    
    return db;
}

/* Initialize learning cache */
LearningCache* init_learning_cache() {
    LearningCache* cache = calloc(1, sizeof(LearningCache));
    cache->capacity = 50;
    cache->mappings = calloc(cache->capacity, sizeof(LearnedMapping));
    cache->count = 0;
    return cache;
}

/* Get package name for specific manager */
const char* get_manager_specific_name(const PackageAlias* alias, const char* manager) {
    if (strcasecmp(manager, "homebrew") == 0 || strcasecmp(manager, "brew") == 0) {
        return alias->homebrew_name;
    } else if (strcasecmp(manager, "vcpkg") == 0) {
        return alias->vcpkg_name;
    } else if (strcasecmp(manager, "chocolatey") == 0 || strcasecmp(manager, "choco") == 0) {
        return alias->choco_name;
    } else if (strcasecmp(manager, "apt") == 0 || strcasecmp(manager, "apt-get") == 0) {
        return alias->apt_name;
    } else if (strcasecmp(manager, "snap") == 0) {
        return alias->snap_name;
    } else if (strcasecmp(manager, "winget") == 0) {
        return alias->winget_name;
    } else if (strcasecmp(manager, "pacman") == 0) {
        return alias->pacman_name;
    } else if (strcasecmp(manager, "nix") == 0) {
        return alias->nix_name;
    } else if (strcasecmp(manager, "scoop") == 0) {
        return alias->scoop_name;
    }
    return NULL;
}

/* Main resolution function */
SearchResults* resolve_package(const char* query, const char* preferred_manager) {
    if (!g_package_db) {
        g_package_db = init_package_database();
    }
    if (!g_learning_cache) {
        g_learning_cache = init_learning_cache();
    }
    
    SearchResults* results = calloc(1, sizeof(SearchResults));
    results->capacity = 10;
    results->results = calloc(results->capacity, sizeof(SearchResult));
    results->count = 0;
    
    /* First, check learned mappings */
    for (int i = 0; i < g_learning_cache->count; i++) {
        LearnedMapping* lm = &g_learning_cache->mappings[i];
        if (strcasecmp(lm->query, query) == 0) {
            SearchResult sr = {0};
            strncpy(sr.package_name, lm->resolved_name, sizeof(sr.package_name) - 1);
            strncpy(sr.manager_name, lm->manager_name, sizeof(sr.manager_name) - 1);
            sr.match_score = lm->confidence;
            sr.is_exact = (lm->confidence >= 0.95);
            sr.is_available = true;
            
            results->results[results->count++] = sr;
            
            /* If we found a high-confidence learned mapping, prioritize it */
            if (lm->confidence >= 0.9) {
                return results;
            }
        }
    }
    
    /* Search in package database */
    float best_score = 0.0;
    int best_index = -1;
    
    for (int i = 0; i < g_package_db->count; i++) {
        PackageAlias* alias = &g_package_db->aliases[i];
        float score = advanced_match_score(query, alias);
        
        if (score > 0.5) {  /* Threshold for considering a match */
            /* Check all available package managers */
            const struct {
                const char* manager_name;
                const char* package_name;
            } managers[] = {
                {"homebrew", alias->homebrew_name},
                {"vcpkg", alias->vcpkg_name},
                {"chocolatey", alias->choco_name},
                {"apt", alias->apt_name},
                {"snap", alias->snap_name},
                {"winget", alias->winget_name},
                {"pacman", alias->pacman_name},
                {"nix", alias->nix_name},
                {"scoop", alias->scoop_name}
            };
            
            for (int j = 0; j < 9; j++) {
                if (managers[j].package_name) {
                    /* Expand results array if needed */
                    if (results->count >= results->capacity) {
                        results->capacity *= 2;
                        results->results = realloc(results->results, 
                                                  results->capacity * sizeof(SearchResult));
                    }
                    
                    SearchResult sr = {0};
                    strncpy(sr.package_name, managers[j].package_name, 
                           sizeof(sr.package_name) - 1);
                    strncpy(sr.manager_name, managers[j].manager_name, 
                           sizeof(sr.manager_name) - 1);
                    sr.match_score = score * alias->confidence;
                    sr.is_exact = (score >= 0.95);
                    sr.is_available = true;
                    
                    /* Boost score if it matches preferred manager */
                    if (preferred_manager && 
                        strcasecmp(managers[j].manager_name, preferred_manager) == 0) {
                        sr.match_score *= 1.2;
                        if (sr.match_score > 1.0) sr.match_score = 1.0;
                    }
                    
                    results->results[results->count++] = sr;
                    
                    if (sr.match_score > best_score) {
                        best_score = sr.match_score;
                        best_index = results->count - 1;
                    }
                }
            }
        }
    }
    
    /* Sort results by score (descending) */
    for (int i = 0; i < results->count - 1; i++) {
        for (int j = i + 1; j < results->count; j++) {
            if (results->results[j].match_score > results->results[i].match_score) {
                SearchResult temp = results->results[i];
                results->results[i] = results->results[j];
                results->results[j] = temp;
            }
        }
    }
    
    return results;
}

/* Learn from successful resolution */
void learn_mapping(const char* query, const char* resolved_name, 
                  const char* manager, float confidence) {
    if (!g_learning_cache) {
        g_learning_cache = init_learning_cache();
    }
    
    /* Check if mapping already exists */
    for (int i = 0; i < g_learning_cache->count; i++) {
        LearnedMapping* lm = &g_learning_cache->mappings[i];
        if (strcasecmp(lm->query, query) == 0 && 
            strcasecmp(lm->manager_name, manager) == 0) {
            /* Update confidence (weighted average) */
            lm->confidence = (lm->confidence * 0.7) + (confidence * 0.3);
            lm->timestamp = time(NULL);
            return;
        }
    }
    
    /* Add new mapping */
    if (g_learning_cache->count >= g_learning_cache->capacity) {
        g_learning_cache->capacity *= 2;
        g_learning_cache->mappings = realloc(g_learning_cache->mappings,
                                            g_learning_cache->capacity * sizeof(LearnedMapping));
    }
    
    LearnedMapping* new_mapping = &g_learning_cache->mappings[g_learning_cache->count++];
    strncpy(new_mapping->query, query, sizeof(new_mapping->query) - 1);
    strncpy(new_mapping->resolved_name, resolved_name, sizeof(new_mapping->resolved_name) - 1);
    strncpy(new_mapping->manager_name, manager, sizeof(new_mapping->manager_name) - 1);
    new_mapping->confidence = confidence;
    new_mapping->timestamp = time(NULL);
}

/* Add custom package alias */
void add_package_alias(const char* canonical_name, const char* manager, 
                      const char* package_name) {
    if (!g_package_db) {
        g_package_db = init_package_database();
    }
    
    /* Find existing alias or create new */
    PackageAlias* alias = NULL;
    for (int i = 0; i < g_package_db->count; i++) {
        if (strcasecmp(g_package_db->aliases[i].canonical_name, canonical_name) == 0) {
            alias = &g_package_db->aliases[i];
            break;
        }
    }
    
    if (!alias) {
        /* Create new alias */
        if (g_package_db->count >= g_package_db->capacity) {
            g_package_db->capacity *= 2;
            g_package_db->aliases = realloc(g_package_db->aliases,
                                           g_package_db->capacity * sizeof(PackageAlias));
        }
        alias = &g_package_db->aliases[g_package_db->count++];
        strncpy(alias->canonical_name, canonical_name, sizeof(alias->canonical_name) - 1);
        alias->confidence = 0.8;  /* User-added mappings have good confidence */
    }
    
    /* Update specific manager name */
    char* name_copy = strdup(package_name);
    
    if (strcasecmp(manager, "homebrew") == 0) {
        if (alias->homebrew_name) free(alias->homebrew_name);
        alias->homebrew_name = name_copy;
    } else if (strcasecmp(manager, "vcpkg") == 0) {
        if (alias->vcpkg_name) free(alias->vcpkg_name);
        alias->vcpkg_name = name_copy;
    } else if (strcasecmp(manager, "chocolatey") == 0) {
        if (alias->choco_name) free(alias->choco_name);
        alias->choco_name = name_copy;
    } else if (strcasecmp(manager, "apt") == 0) {
        if (alias->apt_name) free(alias->apt_name);
        alias->apt_name = name_copy;
    } else if (strcasecmp(manager, "snap") == 0) {
        if (alias->snap_name) free(alias->snap_name);
        alias->snap_name = name_copy;
    } else if (strcasecmp(manager, "winget") == 0) {
        if (alias->winget_name) free(alias->winget_name);
        alias->winget_name = name_copy;
    } else if (strcasecmp(manager, "pacman") == 0) {
        if (alias->pacman_name) free(alias->pacman_name);
        alias->pacman_name = name_copy;
    } else if (strcasecmp(manager, "nix") == 0) {
        if (alias->nix_name) free(alias->nix_name);
        alias->nix_name = name_copy;
    } else if (strcasecmp(manager, "scoop") == 0) {
        if (alias->scoop_name) free(alias->scoop_name);
        alias->scoop_name = name_copy;
    } else {
        free(name_copy);
    }
    
    alias->last_used = time(NULL);
}

/* Free search results */
void free_search_results(SearchResults* results) {
    if (results) {
        free(results->results);
        free(results);
    }
}

/* Free package database */
void free_package_database(PackageDatabase* db) {
    if (db) {
        for (int i = 0; i < db->count; i++) {
            PackageAlias* alias = &db->aliases[i];
            /* Free dynamically allocated names */
            if (alias->homebrew_name && 
                (void*)alias->homebrew_name < (void*)&COMMON_PACKAGES[0] ||
                (void*)alias->homebrew_name > (void*)&COMMON_PACKAGES[sizeof(COMMON_PACKAGES)/sizeof(COMMON_PACKAGES[0])]) {
                free(alias->homebrew_name);
            }
            /* Repeat for other names... */
            
            if (alias->tags) {
                for (int j = 0; j < alias->tags_count; j++) {
                    free(alias->tags[j]);
                }
                free(alias->tags);
            }
        }
        free(db->aliases);
        free(db);
    }
}

/* Free learning cache */
void free_learning_cache(LearningCache* cache) {
    if (cache) {
        free(cache->mappings);
        free(cache);
    }
}

/* Test function */
#ifdef RESOLVER_TEST
int main() {
    printf("Testing package resolver...\n\n");
    
    /* Test various queries */
    const char* test_queries[] = {
        "curl", "json", "nlohmann", "libcurl", "boost", "openssl", "zlib"
    };
    
    for (int i = 0; i < sizeof(test_queries)/sizeof(test_queries[0]); i++) {
        printf("Resolving: %s\n", test_queries[i]);
        SearchResults* results = resolve_package(test_queries[i], NULL);
        
        for (int j = 0; j < results->count && j < 3; j++) {
            printf("  [%d] %s (%s) - Score: %.2f%s\n", 
                   j + 1,
                   results->results[j].package_name,
                   results->results[j].manager_name,
                   results->results[j].match_score * 100,
                   results->results[j].is_exact ? " [EXACT]" : "");
        }
        printf("\n");
        
        free_search_results(results);
    }
    
    /* Test learning */
    learn_mapping("json", "nlohmann-json", "vcpkg", 0.95);
    printf("Learned mapping: json -> nlohmann-json (vcpkg)\n\n");
    
    /* Test again after learning */
    SearchResults* results = resolve_package("json", "vcpkg");
    if (results->count > 0) {
        printf("After learning: json -> %s (%s)\n", 
               results->results[0].package_name,
               results->results[0].manager_name);
    }
    free_search_results(results);
    
    /* Cleanup */
    if (g_package_db) free_package_database(g_package_db);
    if (g_learning_cache) free_learning_cache(g_learning_cache);
    
    return 0;
}
#endif