/*
* Copyright (C) 2025 Pedro Henrique / phkaiser13
*
* SPDX-License-Identifier: Apache-2.0
*/

// CHANGE SUMMARY:
// - Included the `nlohmann/json.hpp` header for JSON parsing capabilities.
// - Implemented `download_to_string`, a new function that fetches content from a URL
//   and returns it as a string. It includes a required "User-Agent" header for
//   compatibility with the GitHub API.
// - Implemented `parse_github_latest_tag`, a new function that safely parses a JSON
//   string using `nlohmann::json` to extract the `tag_name` field, handling potential
//   errors gracefully.

#include "downloader.hpp"
#include <cpr/cpr.h>
/* BEGIN CHANGE: Add include for JSON parsing. */
#include <nlohmann/json.hpp>
/* END CHANGE */

#include <fstream>
#include <string>
#include <memory>

// Helper function to create a DownloadResult with an error message.
static DownloadResult make_error_result(DownloadStatusCode code, const std::string& message) {
    DownloadResult result;
    result.code = code;
    result.error_message = new char[message.length() + 1];
    std::strcpy(result.error_message, message.c_str());
    return result;
}

// Helper function to create a success result.
static DownloadResult make_success_result() {
    DownloadResult result;
    result.code = DOWNLOAD_SUCCESS;
    result.error_message = nullptr;
    return result;
}

/* BEGIN CHANGE: Add implementation for new string download and JSON parsing functions. */
// Helper for StringDownloadResult errors.
static StringDownloadResult make_string_error_result(DownloadStatusCode code, const std::string& message) {
    StringDownloadResult result;
    result.code = code;
    result.response_body = nullptr;
    result.error_message = new char[message.length() + 1];
    std::strcpy(result.error_message, message.c_str());
    return result;
}

extern "C" StringDownloadResult download_to_string(const char* url) {
    if (!url) {
        return make_string_error_result(DOWNLOAD_ERROR_INVALID_URL, "URL is null.");
    }

    // The GitHub API requires a User-Agent header.
    cpr::Response r = cpr::Get(cpr::Url{url},
                               cpr::Header{{"User-Agent", "phpkg-installer/1.0"}},
                               cpr::Timeout{30000}); // 30 seconds timeout

    if (r.error.code != cpr::ErrorCode::OK) {
        return make_string_error_result(DOWNLOAD_ERROR_NETWORK, "Network error: " + r.error.message);
    }

    if (r.status_code >= 400) {
        std::string error_msg = "HTTP error: " + std::to_string(r.status_code) + " " + r.reason;
        return make_string_error_result(DOWNLOAD_ERROR_HTTP, error_msg);
    }

    StringDownloadResult result;
    result.code = DOWNLOAD_SUCCESS;
    result.error_message = nullptr;
    result.response_body = new char[r.text.length() + 1];
    std::strcpy(result.response_body, r.text.c_str());

    return result;
}

extern "C" char* parse_github_latest_tag(const char* json_string) {
    if (!json_string) {
        return nullptr;
    }

    try {
        auto json = nlohmann::json::parse(json_string);
        if (json.contains("tag_name") && json["tag_name"].is_string()) {
            std::string tag_name = json["tag_name"];
            char* result = new char[tag_name.length() + 1];
            std::strcpy(result, tag_name.c_str());
            return result;
        }
    } catch (const nlohmann::json::parse_error&) {
        // Invalid JSON, return null quietly. The C side will report the error.
        return nullptr;
    } catch (...) {
        // Other exceptions (e.g., key not found, wrong type).
        return nullptr;
    }

    return nullptr;
}
/* END CHANGE */

// The C-linkage implementation of the download_file function.
extern "C" DownloadResult download_file(const char* url, const char* destination_path, const DownloadCallbacks* callbacks) {
    if (!url || !destination_path) {
        return make_error_result(DOWNLOAD_ERROR_INVALID_URL, "URL or destination path is null.");
    }

    std::ofstream ofs(destination_path, std::ios::binary);
    if (!ofs) {
        return make_error_result(DOWNLOAD_ERROR_FILESYSTEM, "Failed to open destination file for writing: " + std::string(destination_path));
    }

    cpr::Url cpr_url{url};

    cpr::ProgressCallback progress_cb;
    if (callbacks && callbacks->on_progress) {
        progress_cb = cpr::ProgressCallback([callbacks](cpr::cpr_off_t total, cpr::cpr_off_t downloaded, cpr::cpr_off_t, cpr::cpr_off_t, intptr_t) -> bool {
            callbacks->on_progress(total, downloaded, callbacks->user_data);
            return true; // Continue download
        });
    }

    auto write_cb = cpr::WriteCallback([&ofs](std::string data, intptr_t) -> bool {
        ofs.write(data.c_str(), data.length());
        return ofs.good();
    });

    cpr::Session session;
    session.SetUrl(cpr_url);
    session.SetWriteCallback(write_cb);
    if (progress_cb) {
        session.SetProgressCallback(progress_cb);
    }
    session.SetRedirect(true);
    session.SetTimeout(cpr::Timeout{300000}); // 300 seconds

    cpr::Response r = session.Get();

    if (!ofs.good()) {
        return make_error_result(DOWNLOAD_ERROR_FILESYSTEM, "An error occurred while writing to the destination file.");
    }

    if (r.error.code != cpr::ErrorCode::OK) {
        return make_error_result(DOWNLOAD_ERROR_NETWORK, "Network error: " + r.error.message);
    }

    if (r.status_code >= 400) {
        std::string error_msg = "HTTP error: " + std::to_string(r.status_code) + " " + r.reason;
        return make_error_result(DOWNLOAD_ERROR_HTTP, error_msg);
    }

    return make_success_result();
}