/* Copyright (C) 2025 Pedro Henrique / phkaiser13
* Archive: src/core/cli/commands/policy_handler.c
*
* Este Archive implementa o manipulador para o grupo de comandos 'policy'. Ele atua como
* uma ponte para o módulo Rust 'policy_engine', que é responsável por
* executar verificações de Policy-as-Code (por exemplo, usando um motor Rego como OPA/Conftest).
*
* A função principal deste código C é:
* 1. Analisar os argumentos da linha de comando para subcomandos como 'scan', 'apply' e 'test'.
* 2. Extrair as informações necessárias, como o caminho para os manifestos do Kubernetes
*    a serem verificados, a localização das definições de política ou o nome do cluster alvo.
* 3. Construir um payload JSON bem formado que representa a solicitação do usuário.
* 4. Invocar a função FFI `run_policy_engine` do módulo Rust.
* 5. Traduzir o resultado de volta para um código de status de aplicação padrão.
*
* Isso permite que os desenvolvedores executem localmente as mesmas verificações de política que são
* aplicadas no pipeline de CI/CD, melhorando a velocidade de desenvolvimento e reduzindo
* falhas relacionadas à conformidade.
*
* SPDX-License-Identifier: Apache-2.0 */

#include "policy_handler.h"
#include "ui/tui.h"
#include "libs/liblogger/Logger.hpp"
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <errno.h> // Para strtol

// --- Foreign Function Interface (FFI) Declaration ---

/**
 * @brief Função externa exportada pelo módulo Rust 'policy_engine'.
 *
 * Esta função é o ponto de entrada para a lógica Rust de avaliação de políticas.
 * Ela aceita um payload JSON que define a ação (por exemplo, 'scan', 'apply', 'test') e seus
 * parâmetros (por exemplo, caminhos para manifestos e políticas, nome do cluster, número do PR).
 *
 * @param config_json Uma string UTF-8 terminada em nulo contendo a
 *                    configuração JSON para a operação do motor de políticas.
 * @return Um código de saída inteiro. 0 indica sucesso;
 *         diferente de zero indica falha (violação de política ou erro de execução).
 */
extern int run_policy_engine(const char* config_json);


// --- Private Helper Functions ---

/**
 * @brief Manipula o subcomando 'scan'.
 *
 * Analisa os argumentos para escanear manifestos (--path, --policy-repo),
 * constrói o payload JSON correspondente e chama a função FFI do Rust.
 *
 * @param argc A contagem de argumentos, começando pelos argumentos após 'scan'.
 * @param argv O vetor de argumentos, começando pelos argumentos após 'scan'.
 * @return phStatus indicando o resultado da operação.
 */
#include <stdbool.h>
static phStatus handle_scan_subcommand(int argc, const char** argv) {
    const char* path = NULL;
    const char* policy_repo = NULL;
    bool fail_on_violation = false;

    // 1. Analisa os argumentos da linha de comando
    for (int i = 0; i < argc; ++i) {
        if (strcmp(argv[i], "--path") == 0 && i + 1 < argc) {
            path = argv[++i];
        } else if (strcmp(argv[i], "--policy-repo") == 0 && i + 1 < argc) {
            policy_repo = argv[++i];
        } else if (strcmp(argv[i], "--fail-on-violation") == 0) {
            fail_on_violation = true;
        }
    }

    // 2. Valida que todos os argumentos necessários foram fornecidos
    if (path == NULL || policy_repo == NULL) {
        tui_print_error("Argumentos obrigatórios ausentes para 'scan'. Use --path e --policy-repo.");
        return ph_ERROR_INVALID_ARGS;
    }

    // 3. Constrói o payload JSON
    char json_buffer[1024];
    const char* json_format =
        "{"
        "  \"action\": \"scan\","
        "  \"parameters\": {"
        "    \"manifest_path\": \"%s\","
        "    \"policy_repo_path\": \"%s\","
        "    \"fail_on_violation\": %s"
        "  }"
        "}";

    int written = snprintf(json_buffer, sizeof(json_buffer), json_format, path, policy_repo, fail_on_violation ? "true" : "false");

    if (written < 0 || (size_t)written >= sizeof(json_buffer)) {
        logger_log(LOG_LEVEL_ERROR, "PolicyHandler", "Falha ao construir o payload JSON para 'scan': estouro de buffer.");
        tui_print_error("Erro interno: não foi possível construir a requisição.");
        return ph_ERROR_BUFFER_TOO_SMALL;
    }

    logger_log_fmt(LOG_LEVEL_DEBUG, "PolicyHandler", "Chamando FFI Rust com payload JSON: %s", json_buffer);

    // 4. Chama a função FFI do Rust e traduz o resultado
    int rust_exit_code = run_policy_engine(json_buffer);

    if (rust_exit_code == 0) {
        logger_log(LOG_LEVEL_INFO, "PolicyHandler", "Módulo Rust para 'policy scan' executado com sucesso.");
        tui_print_success("Verificação de política concluída. Todos os manifestos estão em conformidade.");
        return ph_SUCCESS;
    } else {
        logger_log_fmt(LOG_LEVEL_ERROR, "PolicyHandler", "Módulo Rust para 'policy scan' falhou com código de saída: %d.", rust_exit_code);
        tui_print_error("Verificação de política falhou. Violações foram encontradas ou ocorreu um erro.");
        return ph_ERROR_EXEC_FAILED;
    }
}

/**
 * @brief Manipula o subcomando 'apply'.
 *
 * Analisa os argumentos para aplicar políticas a um cluster (--policy-repo, --cluster),
 * constrói o payload JSON correspondente e chama a função FFI do Rust.
 *
 * @param argc A contagem de argumentos, começando pelos argumentos após 'apply'.
 * @param argv O vetor de argumentos, começando pelos argumentos após 'apply'.
 * @return phStatus indicando o resultado da operação.
 */
static phStatus handle_apply_subcommand(int argc, const char** argv) {
    const char* policy_repo = NULL;
    const char* cluster_name = NULL;
    const char* mode = NULL;

    // 1. Analisa os argumentos da linha de comando
    for (int i = 0; i < argc; ++i) {
        if (strcmp(argv[i], "--policy-repo") == 0 && i + 1 < argc) {
            policy_repo = argv[++i];
        } else if (strcmp(argv[i], "--cluster") == 0 && i + 1 < argc) {
            cluster_name = argv[++i];
        } else if (strcmp(argv[i], "--mode") == 0 && i + 1 < argc) {
            mode = argv[++i];
        }
    }

    // 2. Valida que o argumento obrigatório foi fornecido
    if (mode == NULL) {
        tui_print_error("Argumento obrigatório ausente para 'apply'. Use --mode.");
        return ph_ERROR_INVALID_ARGS;
    }

    // 3. Constrói o payload JSON
    char json_buffer[1024];
    char cluster_json_part[256];
    char policy_repo_json_part[256];

    if (cluster_name) {
        snprintf(cluster_json_part, sizeof(cluster_json_part), ",\"cluster_name\":\"%s\"", cluster_name);
    } else {
        cluster_json_part[0] = '\0';
    }

    if (policy_repo) {
        snprintf(policy_repo_json_part, sizeof(policy_repo_json_part), ",\"policy_repo_path\":\"%s\"", policy_repo);
    } else {
        policy_repo_json_part[0] = '\0';
    }

    int written = snprintf(json_buffer, sizeof(json_buffer),
        "{\"action\":\"apply\",\"parameters\":{\"mode\":\"%s\"%s%s}}",
        mode, cluster_json_part, policy_repo_json_part);

    if (written < 0 || (size_t)written >= sizeof(json_buffer)) {
        logger_log(LOG_LEVEL_ERROR, "PolicyHandler", "Falha ao construir o payload JSON para 'apply': estouro de buffer.");
        tui_print_error("Erro interno: não foi possível construir a requisição.");
        return ph_ERROR_BUFFER_TOO_SMALL;
    }

    logger_log_fmt(LOG_LEVEL_DEBUG, "PolicyHandler", "Chamando FFI Rust com payload JSON: %s", json_buffer);

    // 4. Chama a função FFI do Rust e traduz o resultado
    int rust_exit_code = run_policy_engine(json_buffer);

    if (rust_exit_code == 0) {
        logger_log(LOG_LEVEL_INFO, "PolicyHandler", "Módulo Rust para 'policy apply' executado com sucesso.");
        char success_msg[256];
        if (cluster_name) {
             snprintf(success_msg, sizeof(success_msg), "Políticas de '%s' aplicadas com sucesso ao cluster '%s'.", policy_repo, cluster_name);
        } else {
             snprintf(success_msg, sizeof(success_msg), "Políticas de '%s' aplicadas com sucesso ao cluster padrão.", policy_repo);
        }
        tui_print_success(success_msg);
        return ph_SUCCESS;
    } else {
        logger_log_fmt(LOG_LEVEL_ERROR, "PolicyHandler", "Módulo Rust para 'policy apply' falhou com código de saída: %d.", rust_exit_code);
        tui_print_error("Falha ao aplicar políticas. Verifique os logs para mais detalhes.");
        return ph_ERROR_EXEC_FAILED;
    }
}

/**
 * @brief Manipula o subcomando 'test'.
 *
 * Analisa os argumentos para testar políticas em um ambiente de preview (--policy-repo, --pr-number),
 * constrói o payload JSON correspondente e chama a função FFI do Rust.
 *
 * @param argc A contagem de argumentos, começando pelos argumentos após 'test'.
 * @param argv O vetor de argumentos, começando pelos argumentos após 'test'.
 * @return phStatus indicando o resultado da operação.
 */
static phStatus handle_test_subcommand(int argc, const char** argv) {
    const char* policy_repo = NULL;
    const char* pr_str = NULL;
    long pr_number = 0;

    // 1. Analisa os argumentos da linha de comando
    for (int i = 0; i < argc; ++i) {
        if (strcmp(argv[i], "--policy-repo") == 0 && i + 1 < argc) {
            policy_repo = argv[++i];
        } else if (strcmp(argv[i], "--pr") == 0 && i + 1 < argc) {
            pr_str = argv[++i];
        }
    }

    // 2. Valida que todos os argumentos necessários foram fornecidos
    if (pr_str == NULL) {
        tui_print_error("Argumento obrigatório ausente para 'test'. Use --pr.");
        return ph_ERROR_INVALID_ARGS;
    }

    // 3. Converte o número do PR de string para long de forma segura
    char* endptr;
    errno = 0; // Reseta errno antes da chamada
    pr_number = strtol(pr_str, &endptr, 10);

    if (errno != 0 || *endptr != '\0' || pr_number <= 0) {
        char error_msg[256];
        snprintf(error_msg, sizeof(error_msg), "Número de Pull Request inválido: '%s'. Deve ser um inteiro positivo.", pr_str);
        tui_print_error(error_msg);
        return ph_ERROR_INVALID_ARGS;
    }

    // 4. Constrói o payload JSON
    char json_buffer[1024];
    char policy_repo_json_part[256];

    if (policy_repo) {
        snprintf(policy_repo_json_part, sizeof(policy_repo_json_part), ",\"policy_repo_path\":\"%s\"", policy_repo);
    } else {
        policy_repo_json_part[0] = '\0';
    }

    const char* json_format =
        "{"
        "  \"action\": \"test\","
        "  \"parameters\": {"
        "    \"pr_number\": %ld%s"
        "  }"
        "}";

    int written = snprintf(json_buffer, sizeof(json_buffer), json_format, pr_number, policy_repo_json_part);

    if (written < 0 || (size_t)written >= sizeof(json_buffer)) {
        logger_log(LOG_LEVEL_ERROR, "PolicyHandler", "Falha ao construir o payload JSON para 'test': estouro de buffer.");
        tui_print_error("Erro interno: não foi possível construir a requisição.");
        return ph_ERROR_BUFFER_TOO_SMALL;
    }

    logger_log_fmt(LOG_LEVEL_DEBUG, "PolicyHandler", "Chamando FFI Rust com payload JSON: %s", json_buffer);

    // 5. Chama a função FFI do Rust e traduz o resultado
    int rust_exit_code = run_policy_engine(json_buffer);

    if (rust_exit_code == 0) {
        logger_log(LOG_LEVEL_INFO, "PolicyHandler", "Módulo Rust para 'policy test' executado com sucesso.");
        char success_msg[256];
        snprintf(success_msg, sizeof(success_msg), "Testes de política passaram para o ambiente de preview do PR #%ld.", pr_number);
        tui_print_success(success_msg);
        return ph_SUCCESS;
    } else {
        logger_log_fmt(LOG_LEVEL_ERROR, "PolicyHandler", "Módulo Rust para 'policy test' falhou com código de saída: %d.", rust_exit_code);
        char error_msg[256];
        snprintf(error_msg, sizeof(error_msg), "Testes de política falharam para o ambiente de preview do PR #%ld. Violações foram encontradas.", pr_number);
        tui_print_error(error_msg);
        return ph_ERROR_EXEC_FAILED;
    }
}


// --- Public Function Implementation ---

/**
 * @see policy_handler.h
 */
phStatus handle_policy_command(int argc, const char** argv) {
    if (argc < 1 || argv[0] == NULL) {
        tui_print_error("Nenhum subcomando fornecido para 'policy'. Use 'scan', 'apply' ou 'test'.");
        return ph_ERROR_INVALID_ARGS;
    }

    const char* subcommand = argv[0];
    logger_log_fmt(LOG_LEVEL_INFO, "PolicyHandler", "Despachando subcomando 'policy': '%s'", subcommand);

    if (strcmp(subcommand, "scan") == 0) {
        return handle_scan_subcommand(argc - 1, &argv[1]);
    } else if (strcmp(subcommand, "apply") == 0) {
        return handle_apply_subcommand(argc - 1, &argv[1]);
    } else if (strcmp(subcommand, "test") == 0) {
        return handle_test_subcommand(argc - 1, &argv[1]);
    } else {
        char error_msg[128];
        snprintf(error_msg, sizeof(error_msg), "Subcomando desconhecido para 'policy': '%s'", subcommand);
        tui_print_error(error_msg);
        return ph_ERROR_NOT_FOUND;
    }
}