/*
 * Copyright (C) 2025 Pedro Henrique / phkaiser13
 *
 * File: Logger.cpp
 *
 * [
 * This file implements the singleton Logger class and its C-compatible wrapper
 * functions. It handles the low-level details of file I/O, message formatting,
 * and thread synchronization. This version includes a new formatted logging
 * function, `logger_log_fmt`, which dynamically allocates memory to prevent
 * buffer overflow vulnerabilities.
 *
 * The use of `std::lock_guard` makes the `log` method inherently thread-safe,
 * preventing interleaved or corrupted log entries when multiple modules write
 * to the log simultaneously. The singleton pattern is implemented using a
 * static local variable in `get_instance()`, which is a clean and thread-safe
 * approach in modern C++.
 *
 * Deadlock Resolution:
 * This implementation resolves a critical deadlock that occurred in the
 * previous version. The issue was that `Logger::init` acquired a lock and then
 * called the public `Logger::log`, which attempted to acquire the same lock
 * again, causing the thread to freeze. The fix introduces a private method,
 * `log_impl`, which handles the core logic of writing to the log file without
 * managing mutexes. Both `init` and the public `log` methods now acquire the
 * lock and then call this internal, non-locking implementation, ensuring the
 * mutex is never acquired twice by the same thread and thus preventing the
 * deadlock.
 * ]
 *
 * SPDX-License-Identifier: Apache-2.0
 */

#include "Logger.hpp"
#include <iostream>
#include <chrono>
#include <ctime>
#include <iomanip> // For std::put_time
#include <cstdarg> // For va_list, va_start, va_end
#include <vector>  // For std::vector as a safe dynamic buffer

// --- C++ Class Implementation ---

/**
 * @brief Provides the singleton instance.
 *
 * The `static` keyword ensures the instance is created only once, the first
 * time this function is called. This is the Meyers' Singleton pattern, which
 * is thread-safe in C++11 and later.
 */
Logger& Logger::get_instance() {
    static Logger instance;
    return instance;
}

/**
 * @brief Destructor closes the file stream automatically due to RAII.
 *
 * When the singleton instance is destroyed at program exit, this destructor
 * is called. It ensures any open log file is properly closed and a final
 * shutdown message is logged.
 */
Logger::~Logger() {
    if (m_log_file.is_open()) {
        // We acquire the lock one last time to ensure the final message is
        // written safely, without interleaving with other potential last-
        // minute log calls from other threads.
        std::lock_guard<std::mutex> lock(m_mutex);
        log_impl(LOG_LEVEL_INFO, "LOGGER", "Logging system shutting down.");
        m_log_file.close();
    }
}

/**
 * @brief Initializes the logger by opening the specified file.
 * @param filename The path to the log file.
 * @return true on success, false on failure.
 *
 * This method is thread-safe. It acquires a lock, opens the log file, and
 * then calls the internal `log_impl` to write an initialization message.
 * This avoids the deadlock that occurred in the previous version.
 */
bool Logger::init(const std::string& filename) {
    std::lock_guard<std::mutex> lock(m_mutex);

    if (m_log_file.is_open()) {
        return true; // Already initialized
    }

    m_log_file.open(filename, std::ios::out | std::ios::app);
    if (!m_log_file.is_open()) {
        // Use std::cerr for critical errors when the logger itself fails.
        std::cerr << "FATAL: Could not open log file: " << filename << std::endl;
        return false;
    }

    // Call the internal implementation directly to avoid re-locking the mutex.
    // This is the core of the deadlock fix.
    log_impl(LOG_LEVEL_INFO, "LOGGER", "Logging system initialized.");
    return true;
}

/**
 * @brief Converts a phLogLevel enum to its human-readable string representation.
 * @param level The log level enum.
 * @return A constant C-string representing the log level.
 */
static const char* level_to_string(phLogLevel level) {
    switch (level) {
        case LOG_LEVEL_DEBUG: return "DEBUG";
        case LOG_LEVEL_INFO:  return "INFO ";
        case LOG_LEVEL_WARN:  return "WARN ";
        case LOG_LEVEL_ERROR: return "ERROR";
        case LOG_LEVEL_FATAL: return "FATAL";
        default:              return "UNKWN";
    }
}

/**
 * @brief The internal, non-locking implementation of the log function.
 *
 * This method is the core of the logging logic. It formats the timestamp,
 * log level, and message and writes them to the file stream. It assumes
 * that a mutex lock has already been acquired by the calling function.
 */
void Logger::log_impl(phLogLevel level, const std::string& module_name, const std::string& message) {
    if (!m_log_file.is_open()) {
        std::cerr << "LOGGER NOT INITIALIZED: [" << module_name << "] " << message << std::endl;
        return;
    }

    // Get the current time with high precision.
    auto now = std::chrono::system_clock::now();
    auto time_t_now = std::chrono::system_clock::to_time_t(now);

    // Format the timestamp (e.g., "YYYY-MM-DD HH:MM:SS").
    // We use platform-specific thread-safe functions for converting the time.
    struct tm timeinfo;
#ifdef _WIN32
    localtime_s(&timeinfo, &time_t_now); // Windows-specific safe version
#else
    localtime_r(&time_t_now, &timeinfo); // POSIX-specific safe version
#endif

    // Write the complete, formatted log entry to the file.
    m_log_file << "[" << std::put_time(&timeinfo, "%Y-%m-%d %H:%M:%S") << "] "
               << "[" << level_to_string(level) << "] "
               << "[" << module_name << "] "
               << message << std::endl;
}


/**
 * @brief Public-facing log method for simple, pre-formatted messages.
 *
 * This function is the primary entry point for C++ code. It acquires the
 * mutex lock, ensuring exclusive access to the log file, and then calls
 * the internal implementation `log_impl` to perform the actual write.
 */
void Logger::log(phLogLevel level, const std::string& module_name, const std::string& message) {
    std::lock_guard<std::mutex> lock(m_mutex);
    log_impl(level, module_name, message);
}

/**
 * @brief Public-facing log method for formatted messages using a va_list.
 *
 * This function safely handles printf-style formatted logging. It determines
 * the required buffer size, allocates it dynamically using `std::vector`,
 * formats the message, and then calls the main `log` function to handle
 * locking and writing. This two-step process ensures both buffer safety and
 * thread safety.
 */
void Logger::log(phLogLevel level, const std::string& module_name, const char* format, va_list args) {
    // 1. Create a copy of va_list because vsnprintf can invalidate it.
    va_list args_copy;
    va_copy(args_copy, args);
    // Determine the required buffer size by performing a "dry run" format.
    int len = vsnprintf(nullptr, 0, format, args_copy);
    va_end(args_copy);

    if (len < 0) {
        // An encoding error occurred. Log a fallback message.
        log(LOG_LEVEL_ERROR, "LOGGER", "An error occurred during log message formatting.");
        return;
    }

    // 2. Allocate a buffer of the exact size plus one for the null terminator.
    std::vector<char> buffer(static_cast<size_t>(len) + 1);

    // 3. Format the string into the buffer.
    vsnprintf(buffer.data(), buffer.size(), format, args);

    // 4. Pass the final formatted string to the original log function,
    // which handles the mutex locking and file I/O.
    log(level, module_name, std::string(buffer.data()));
}


// --- C Wrapper Implementation ---

/**
 * @see logger.hpp
 */
int logger_init(const char* filename) {
    // This C-style function acts as a bridge to the C++ singleton.
    if (Logger::get_instance().init(filename)) {
        return 0; // Success
    }
    return -1; // Failure
}

/**
 * @see logger.hpp
 */
void logger_log(phLogLevel level, const char* module_name, const char* message) {
    if (module_name == nullptr || message == nullptr) {
        return; // Basic null-pointer safety check.
    }
    Logger::get_instance().log(level, module_name, message);
}

/**
 * @see logger.hpp
 */
void logger_log_fmt(phLogLevel level, const char* module_name, const char* format, ...) {
    if (module_name == nullptr || format == nullptr) {
        return;
    }

    va_list args;
    va_start(args, format);
    // Delegate all the complex, safe formatting logic to the C++ class.
    Logger::get_instance().log(level, module_name, format, args);
    va_end(args);
}

/**
 * @see logger.hpp
 */
void logger_cleanup() {
    // In this singleton implementation, the destructor is called automatically
    // at program exit, which handles file closing. This C function is kept
    // for API consistency and can be used for explicit cleanup if needed.
    logger_log(LOG_LEVEL_INFO, "MAIN", "Application cleanup requested.");
}