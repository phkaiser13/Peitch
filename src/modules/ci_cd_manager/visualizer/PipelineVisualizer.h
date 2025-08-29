/* Copyright (C) 2025 Pedro Henrique / phkaiser13
 * PipelineVisualizer.h - Interface for the C++ CI/CD Pipeline Visualizer.
 *
 * This header defines the public interface for the PipelineVisualizer class.
 * This class is responsible for taking a JSON string (produced by the Go
 * parser), deserializing it into a rich C++ object model, and then rendering
 * a textual representation of that model to the console.
 *
 * The use of C++ allows for a strong, object-oriented representation of the
 * pipeline's structure. The RAII paradigm ensures that all memory allocated
 * during the parsing and object creation process is automatically and safely
 * deallocated when the visualizer object is destroyed.
 *
 * A C-style wrapper function, `visualize_pipeline_from_json`, is also declared
 * to serve as the entry point for the C core, bridging the C and C++ worlds.
 *
 * SPDX-License-Identifier: Apache-2.0 */

#ifndef PIPELINE_VISUALIZER_H
#define PIPELINE_VISUALIZER_H

#include <string>
#include <vector>

class PipelineVisualizer {
public:
    /**
     * @brief Default constructor.
     */
    PipelineVisualizer();

    /**
     * @brief Destructor. Manages the cleanup of internal resources.
     */
    ~PipelineVisualizer();

    /**
     * @brief Loads and parses the pipeline data from a JSON string.
     * @param json_data A standard string containing the pipeline in JSON format.
     * @return true if parsing was successful, false otherwise.
     */
    bool loadFromJSON(const std::string& json_data);

    /**
     * @brief Renders the loaded pipeline structure to standard output.
     *        This method is const as it does not modify the pipeline's state.
     */
    void display() const;

private:
    // --- Internal Object Model ---
    // These structs represent the pipeline's components.

    struct Step {
        std::string name;
        std::string run_command;
    };

    struct Job {
        std::string name;
        std::string runs_on;
        std::vector<Step> steps;
    };

    struct Pipeline {
        std::string name;
        std::vector<Job> jobs;
    };

    // The top-level member variable holding the entire parsed pipeline.
    Pipeline m_pipeline;
};


// --- C-style FFI Wrapper ---
// This is the bridge function that the C core will call.

#ifdef __cplusplus
extern "C" {
#endif

/**
 * @brief The C-compatible entry point for this C++ functionality.
 *
 * This function creates an instance of the PipelineVisualizer, uses it to
 * process the JSON data, and ensures its destruction (triggering RAII cleanup).
 * This is the only symbol that needs to be exported for the C core.
 *
 * @param json_c_str A null-terminated C-style string containing the JSON data.
 */
void visualize_pipeline_from_json(const char* json_c_str);

#ifdef __cplusplus
} // extern "C"
#endif

#endif // PIPELINE_VISUALIZER_H