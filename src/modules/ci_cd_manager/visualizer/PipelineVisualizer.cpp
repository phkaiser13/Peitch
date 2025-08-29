/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * PipelineVisualizer.cpp - Implementation of the C++ CI/CD Pipeline Visualizer.
 *
 * This file implements the logic for parsing a JSON data stream and rendering
 * it as a structured, human-readable text representation of a CI/CD pipeline.
 *
 * It uses the `nlohmann/json` library for robust and modern JSON parsing. The
 * parsing logic is wrapped in a try-catch block to gracefully handle potential
 * errors like malformed JSON or missing keys.
 *
 * The RAII pattern is heavily leveraged; object construction and destruction
 * handle all memory management for the internal data model, making the code
 * safe and leak-free.
 *
 * SPDX-License-Identifier: Apache-2.0 */

#include "PipelineVisualizer.h"
#include <iostream>
#include "nlohmann/json.hpp" // Assuming this library is available in the include path

// For convenience, we use a namespace alias for the json library.
using json = nlohmann::json;

// --- Class Implementation ---

PipelineVisualizer::PipelineVisualizer() {
    // The constructor could initialize default values if needed.
}

PipelineVisualizer::~PipelineVisualizer() {
    // The destructor is automatically called when the object goes out of scope.
    // Thanks to RAII, std::vector and std::string members will clean themselves
    // up, so no manual memory management is needed here.
}

bool PipelineVisualizer::loadFromJSON(const std::string& json_data) {
    try {
        // Parse the string into a json object.
        json j = json::parse(json_data);

        // Clear any previous data.
        m_pipeline = {};

        // Extract data into our C++ object model.
        // The .at() method throws an exception if the key is not found,
        // which will be caught by our catch block.
        j.at("name").get_to(m_pipeline.name);

        const json& jobs_json = j.at("jobs");
        for (auto const& [key, val] : jobs_json.items()) {
            Job current_job;
            val.at("name").get_to(current_job.name);
            val.at("runs_on").get_to(current_job.runs_on);

            const json& steps_json = val.at("steps");
            for (const auto& step_val : steps_json) {
                Step current_step;
                step_val.at("name").get_to(current_step.name);
                step_val.at("run").get_to(current_step.run_command);
                current_job.steps.push_back(current_step);
            }
            m_pipeline.jobs.push_back(current_job);
        }
    } catch (const json::exception& e) {
        // If parsing fails at any point, log the error and return false.
        std::cerr << "[C++ VISUALIZER ERROR] Failed to parse JSON: " << e.what() << std::endl;
        return false;
    }
    return true;
}

void PipelineVisualizer::display() const {
    // Render the structured data to the console.
    std::cout << "==================================================" << std::endl;
    std::cout << "  Workflow: " << m_pipeline.name << std::endl;
    std::cout << "==================================================" << std::endl;

    for (const auto& job : m_pipeline.jobs) {
        std::cout << "\n[JOB] " << job.name << " (Runs on: " << job.runs_on << ")" << std::endl;
        std::cout << "  `-------------------------------------------" << std::endl;
        for (const auto& step : job.steps) {
            std::cout << "    [STEP] Name: " << step.name << std::endl;
            std::cout << "      -> Run: " << step.run_command << std::endl;
        }
    }
    std::cout << "\n==================================================" << std::endl;
}


// --- C-style FFI Wrapper Implementation ---

void visualize_pipeline_from_json(const char* json_c_str) {
    if (!json_c_str) {
        std::cerr << "[C++ WRAPPER ERROR] Received null JSON string." << std::endl;
        return;
    }

    // Create an instance of the visualizer on the stack.
    // Its destructor will be called automatically upon function exit.
    PipelineVisualizer visualizer;

    // Convert C-string to C++ std::string and load the data.
    if (visualizer.loadFromJSON(json_c_str)) {
        // If loading was successful, display the result.
        visualizer.display();
    } else {
        // Loading failed; an error message was already printed by loadFromJSON.
        std::cout << "Could not display pipeline due to parsing errors." << std::endl;
    }
}