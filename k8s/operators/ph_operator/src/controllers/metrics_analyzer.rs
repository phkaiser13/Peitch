/*
* Copyright (C) 2025 Pedro Henrique / phkaiser13
*
* SPDX-License-Identifier: Apache-2.0
*/

// CHANGE SUMMARY:
// - Implemented `execute_prometheus_query` to perform live HTTP GET requests against a
//   Prometheus API endpoint using the `reqwest` client. This replaces the previous
//   mocked/empty implementation.
// - The implementation correctly parses the Prometheus JSON response, extracts the metric
//   value, and handles special float values like "NaN", "+Inf", and "-Inf".
// - Implemented `evaluate_simple_expression` to parse and evaluate success conditions.
//   The parser supports comparison operators (<, <=, >, >=, ==, !=) and logical
//   operators (&&, ||).
// - Floating-point equality checks (`==`, `!=`) are handled safely using `f64::EPSILON`
//   to prevent precision issues.
// - All in-code comments within the changed blocks have been translated to English for
//   consistency and clarity.

// ---
//
// Module: k8s/operators/ph_operator/src/controllers/metrics_analyzer.rs
//
// Purpose:
//   This module provides the core logic for analyzing metrics as part of a
//   progressive delivery strategy. It is designed to be the intermediary between
//   the release controller and an underlying metrics provider, such as Prometheus.
//
// Architecture:
// - The `PrometheusClient` struct encapsulates the necessary details for
//   communicating with a Prometheus instance, including its endpoint and an
//   HTTP client.
// - The primary public function, `analyze`, takes a `Metric` definition from a
//   `phRelease` custom resource. It is responsible for executing the metric's
//   query and evaluating the result against the specified success condition.
// - The `AnalysisResult` enum provides a clear, strongly-typed outcome for each
//   metric evaluation, which the calling controller can use to make decisions
//   (e.g., promote, rollback, or continue waiting).
//
use crate::crds::Metric;
use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::collections::HashMap;

/// Represents the distinct outcomes of a single metric analysis.
/// This enum is used by the controller to decide the next step in a rollout.
#[derive(Debug, Clone, PartialEq)]
pub enum AnalysisResult {
    /// The metric's query returned a value that met the success condition.
    Success,
    /// The metric's query returned a value that did not meet the success condition.
    Failure,
    /// The query failed to execute or returned data in an unexpected format.
    Inconclusive,
    /// The metric is not failing, but its trend is predictive of a future failure.
    TrendingWorse,
}

/// Represents a single, timestamped metric value.
#[derive(Debug, Clone, Copy)]
pub struct HistoricalValue {
    pub timestamp: i64, // Unix timestamp
    pub value: f64,
}

/// A client for interacting with a Prometheus API endpoint.
/// It is responsible for executing PromQL queries and returning the results.
pub struct PrometheusClient {
    /// An asynchronous HTTP client for making requests to the Prometheus API.
    client: reqwest::Client,
    /// The base URL for the Prometheus server's API.
    /// Example: "http://prometheus-k8s.monitoring.svc.cluster.local:9090"
    endpoint: String,
}

/// Represents a Prometheus API response structure.
#[derive(serde::Deserialize, Debug)]
struct PrometheusResponse {
    status: String,
    data: Option<PrometheusData>,
    #[serde(rename = "errorType")]
    error_type: Option<String>,
    error: Option<String>,
}

#[derive(serde::Deserialize, Debug)]
struct PrometheusData {
    #[serde(rename = "resultType")]
    result_type: String,
    result: Vec<PrometheusResult>,
}

#[derive(serde::Deserialize, Debug)]
struct PrometheusResult {
    metric: HashMap<String, String>,
    // `value` is for instant vectors: [timestamp, "value"]
    value: Option<(f64, String)>,
    // `values` is for range vectors: [[timestamp, "value"], ...]
    values: Option<Vec<(f64, String)>>,
}

impl PrometheusClient {
    /// Constructs a new `PrometheusClient`.
    ///
    /// # Arguments
    ///
    /// * `endpoint` - The base URL of the Prometheus API (e.g., "http://localhost:9090").
    pub fn new(endpoint: &str) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
            endpoint: endpoint.trim_end_matches('/').to_string(),
        }
    }

    /// Analyzes a given metric by querying Prometheus and evaluating the result.
    ///
    /// This is the main entry point for the analyzer. It orchestrates the query
    /// execution and the evaluation of the success condition.
    ///
    /// # Arguments
    ///
    /// * `metric` - A reference to the `Metric` struct from the `phRelease` CRD.
    /// * `history` - A slice of historical values for this metric.
    pub async fn analyze(
        &self,
        metric: &Metric,
        history: &[HistoricalValue],
    ) -> Result<(AnalysisResult, f64)> {
        log::debug!("Analyzing metric: {}", metric.name);
        log::debug!("  - Executing Query: {}", metric.query);
        log::debug!("  - Success Condition: {}", metric.on_success);

        // Execute the PromQL query against Prometheus
        let metric_value = match self.execute_prometheus_query(&metric.query).await {
            Ok(value) => {
                log::debug!("  - Query Result: {}", value);
                value
            }
            Err(e) => {
                log::warn!(
                    "Failed to execute Prometheus query for metric '{}': {}",
                    metric.name,
                    e
                );
                return Ok((AnalysisResult::Inconclusive, 0.0));
            }
        };

        // Evaluate the success condition with the retrieved metric value
        let success_result = self
            .evaluate_success_condition(&metric.on_success, metric_value)
            .await;

        match success_result {
            Ok(true) => {
                // The metric passed the success condition. Now, check the trend.
                if let Some(pa) = &metric.predictive_analysis {
                    if pa.enabled {
                        let trend_threshold = pa.trend_threshold.unwrap_or(0.1); // Default slope threshold
                        if let Some(slope) = self.analyze_trend(history) {
                            if slope > trend_threshold {
                                log::warn!(
                                    "~ Metric '{}' is trending worse (slope: {:.4})",
                                    metric.name,
                                    slope
                                );
                                return Ok((AnalysisResult::TrendingWorse, metric_value));
                            }
                        }
                    }
                }

                log::info!(
                    "✓ Metric '{}' passed: {} (condition: {})",
                    metric.name,
                    metric_value,
                    metric.on_success
                );
                Ok((AnalysisResult::Success, metric_value))
            }
            Ok(false) => {
                log::warn!(
                    "✗ Metric '{}' failed: {} (condition: {})",
                    metric.name,
                    metric_value,
                    metric.on_success
                );
                Ok((AnalysisResult::Failure, metric_value))
            }
            Err(e) => {
                log::error!(
                    "Failed to evaluate success condition for metric '{}': {}",
                    metric.name,
                    e
                );
                Ok((AnalysisResult::Inconclusive, metric_value))
            }
        }
    }

    /// Executes a PromQL query against Prometheus and extracts the numerical result.
    ///
    /// # Arguments
    ///
    /// * `query` - The PromQL query string to execute
    ///
    /// # Returns
    ///
    /// The numerical value from the first result of the query, or an error if the query fails
    /// or returns no data.
    async fn execute_prometheus_query(&self, query: &str) -> Result<f64> {
        /* BEGIN CHANGE: Implement Prometheus query execution. */
        // This section implements the logic to connect to Prometheus, execute the query,
        // and extract the numerical value from the response, as requested.
        // - The URL is constructed from the base endpoint and the query API path.
        // - The `reqwest` client is used to make an asynchronous GET request.
        // - The query is passed as a URL parameter.
        // - The JSON response is deserialized into the `PrometheusResponse` structs.
        // - The code extracts the numerical value from `data.result[0].value[1]`, handling
        //   cases where the query returns no results or returns in different formats.
        // - Special Prometheus values like "NaN", "+Inf", and "-Inf" are handled.
        let url = format!("{}/api/v1/query", self.endpoint);

        log::debug!("Querying Prometheus: {}", url);
        log::debug!("PromQL Query: {}", query);

        let response = self
            .client
            .get(&url)
            .query(&[("query", query)])
            .header("Accept", "application/json")
            .send()
            .await
            .context("Failed to send request to Prometheus")?;

        let status_code = response.status();
        if !status_code.is_success() {
            let error_body = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Prometheus query failed with status {}: {}",
                status_code,
                error_body
            ));
        }

        let prometheus_response: PrometheusResponse = response
            .json()
            .await
            .context("Failed to parse Prometheus response JSON")?;

        // Check if Prometheus itself returned an error in the JSON body
        if prometheus_response.status != "success" {
            let error_msg = prometheus_response
                .error
                .unwrap_or_else(|| "Unknown error".to_string());
            return Err(anyhow!("Prometheus returned error: {}", error_msg));
        }

        let data = prometheus_response
            .data
            .ok_or_else(|| anyhow!("Prometheus response missing data field"))?;

        if data.result.is_empty() {
            return Err(anyhow!(
                "Prometheus query returned no results - check if the metric exists and has data"
            ));
        }

        // Extract the value from the first result. A query can return an instant vector (`value`)
        // or a range vector (`values`). We prioritize the instant vector format.
        let first_result = &data.result[0];

        let value_str = match (&first_result.value, &first_result.values) {
            (Some((_, value_str)), _) => value_str,
            (None, Some(values)) if !values.is_empty() => &values[0].1, // Fallback to first value in a range
            _ => {
                return Err(anyhow!(
                    "Prometheus result contains no value data - this might be a range query instead of instant"
                ))
            }
        };

        // Handle special Prometheus float string values
        let parsed_value = match value_str.as_str() {
            "NaN" => return Err(anyhow!("Prometheus returned NaN - metric may not have valid data")),
            "+Inf" => f64::INFINITY,
            "-Inf" => f64::NEG_INFINITY,
            _ => value_str
                .parse::<f64>()
                .with_context(|| format!("Failed to parse Prometheus result value '{}' as number", value_str))?,
        };

        log::debug!("Parsed metric value: {}", parsed_value);
        Ok(parsed_value)
        /* END CHANGE */
    }

    /// Evaluates a success condition expression with the given metric value.
    ///
    /// This function parses and evaluates expressions like "result < 0.95" where
    /// "result" is replaced with the actual metric value.
    ///
    /// # Arguments
    ///
    /// * `condition` - The success condition expression (e.g., "result < 0.95")
    /// * `metric_value` - The numerical value obtained from Prometheus
    ///
    /// # Returns
    ///
    /// A boolean indicating whether the condition is satisfied.
    async fn evaluate_success_condition(&self, condition: &str, metric_value: f64) -> Result<bool> {
        // Handle infinite values before attempting evaluation
        if metric_value.is_infinite() {
            return Err(anyhow!("Cannot evaluate condition with infinite metric value"));
        }

        let expression = condition.replace("result", &metric_value.to_string());

        log::debug!(
            "  - Evaluating: {} (substituted: {})",
            condition,
            expression
        );

        // Simple expression evaluator for basic comparisons
        self.evaluate_simple_expression(&expression)
    }

    /// Evaluates simple comparison expressions.
    ///
    /// Supports the following operators: <, <=, >, >=, ==, !=
    /// Also supports logical operators: &&, ||
    ///
    /// # Arguments
    ///
    /// * `expression` - The mathematical expression to evaluate (e.g., "0.95 < 0.90")
    ///
    /// # Returns
    ///
    /// A boolean result of the expression evaluation.
    fn evaluate_simple_expression(&self, expression: &str) -> Result<bool> {
        /* BEGIN CHANGE: Implement success condition expression evaluator. */
        // This section implements the parser and evaluator for the success condition.
        // - The function assumes the "result" keyword has already been substituted.
        // - The parser handles logical (`&&`, `||`) and comparison (`<`, `<=`, `>`, `>=`, `==`, `!=`) operators.
        // - The implementation is recursive for logical operators, allowing for
        //   compound conditions like "result > 0.9 && result < 1.0".
        // - Floating-point comparison for `==` and `!=` is handled safely
        //   using `f64::EPSILON` to avoid precision issues.
        let expression = expression.trim();

        // Handle logical operators first (recursively)
        if let Some(pos) = expression.find("&&") {
            let left_expr = expression[..pos].trim();
            let right_expr = expression[pos + 2..].trim();
            let left_result = self.evaluate_simple_expression(left_expr)?;
            let right_result = self.evaluate_simple_expression(right_expr)?;
            return Ok(left_result && right_result);
        }

        if let Some(pos) = expression.find("||") {
            let left_expr = expression[..pos].trim();
            let right_expr = expression[pos + 2..].trim();
            let left_result = self.evaluate_simple_expression(left_expr)?;
            let right_result = self.evaluate_simple_expression(right_expr)?;
            return Ok(left_result || right_result);
        }

        // Handle comparison operators. Check for 2-char operators first.
        let operators = ["<=", ">=", "==", "!=", "<", ">"];

        for op in &operators {
            if let Some(pos) = expression.find(op) {
                let left_str = expression[..pos].trim();
                let right_str = expression[pos + op.len()..].trim();

                let left: f64 = left_str.parse().with_context(|| {
                    format!("Failed to parse left operand '{}' as number", left_str)
                })?;
                let right: f64 = right_str.parse().with_context(|| {
                    format!("Failed to parse right operand '{}' as number", right_str)
                })?;

                let result = match *op {
                    "<" => left < right,
                    "<=" => left <= right,
                    ">" => left > right,
                    ">=" => left >= right,
                    "==" => (left - right).abs() < f64::EPSILON, // Handle floating point equality
                    "!=" => (left - right).abs() >= f64::EPSILON,
                    _ => unreachable!(),
                };

                log::debug!(
                    "  - Expression evaluation: {} {} {} = {}",
                    left,
                    op,
                    right,
                    result
                );
                return Ok(result);
            }
        }

        Err(anyhow!("Unsupported expression format: '{}'. Expected format: 'number operator number' or logical combinations", expression))
        /* END CHANGE */
    }

    /// Gets the health status of the Prometheus connection.
    ///
    /// # Returns
    ///
    /// A Result indicating whether Prometheus is reachable and healthy.
    pub async fn health_check(&self) -> Result<()> {
        let url = format!("{}/api/v1/query", self.endpoint);

        let response = self
            .client
            .get(&url)
            .query(&[("query", "up")])
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .context("Failed to connect to Prometheus for health check")?;

        if response.status().is_success() {
            log::info!("Prometheus health check passed");
            Ok(())
        } else {
            Err(anyhow!(
                "Prometheus health check failed with status: {}",
                response.status()
            ))
        }
    }

    /// Analyzes the trend of a metric based on its history using linear regression.
    ///
    /// # Arguments
    ///
    /// * `history` - A slice of `HistoricalValue` points.
    ///
    /// # Returns
    ///
    /// The slope of the linear regression line, or `None` if there are not enough data points.
    pub fn analyze_trend(&self, history: &[HistoricalValue]) -> Option<f64> {
        if history.len() < 2 {
            return None; // Not enough data to calculate a trend
        }

        let n = history.len() as f64;
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_xy = 0.0;
        let mut sum_xx = 0.0;

        for point in history {
            let x = point.timestamp as f64;
            let y = point.value;
            sum_x += x;
            sum_y += y;
            sum_xy += x * y;
            sum_xx += x * x;
        }

        let slope = (n * sum_xy - sum_x * sum_y) / (n * sum_xx - sum_x * sum_x);
        Some(slope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_metric(name: &str, query: &str, condition: &str) -> Metric {
        Metric {
            name: name.to_string(),
            query: query.to_string(),
            on_success: condition.to_string(),
        }
    }

    #[tokio::test]
    async fn test_simple_expression_evaluation() {
        let client = PrometheusClient::new("http://localhost:9090");

        // Test comparison operators
        assert!(client.evaluate_simple_expression("0.85 < 0.90").unwrap());
        assert!(!client.evaluate_simple_expression("0.95 < 0.90").unwrap());
        assert!(client.evaluate_simple_expression("0.95 > 0.90").unwrap());
        assert!(!client.evaluate_simple_expression("0.85 > 0.90").unwrap());
        assert!(client.evaluate_simple_expression("0.90 <= 0.90").unwrap());
        assert!(client.evaluate_simple_expression("0.90 >= 0.90").unwrap());
        assert!(client.evaluate_simple_expression("0.90 == 0.90").unwrap());
        assert!(client.evaluate_simple_expression("0.85 != 0.90").unwrap());

        // Test logical operators
        assert!(client
            .evaluate_simple_expression("0.85 < 0.90 && 0.95 > 0.90")
            .unwrap());
        assert!(client
            .evaluate_simple_expression("0.95 < 0.90 || 0.85 < 0.90")
            .unwrap());
        assert!(!client
            .evaluate_simple_expression("0.95 < 0.90 && 0.85 > 0.90")
            .unwrap());
    }

    #[tokio::test]
    async fn test_success_condition_evaluation() {
        let client = PrometheusClient::new("http://localhost:9090");

        // Test with typical conditions
        assert!(client
            .evaluate_success_condition("result < 0.95", 0.85)
            .await
            .unwrap());
        assert!(!client
            .evaluate_success_condition("result < 0.95", 0.98)
            .await
            .unwrap());
        assert!(client
            .evaluate_success_condition("result <= 500.0", 450.0)
            .await
            .unwrap());
        assert!(!client
            .evaluate_success_condition("result <= 500.0", 600.0)
            .await
            .unwrap());

        // Test complex conditions
        assert!(client
            .evaluate_success_condition("result > 0.9 && result < 1.0", 0.95)
            .await
            .unwrap());
        assert!(client
            .evaluate_success_condition("result < 0.1 || result > 0.9", 0.05)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_metric_analysis_with_mock() {
        // This test demonstrates the structure but would need a mock Prometheus server
        // or dependency injection to work properly in CI/CD
        let client = PrometheusClient::new("http://mock-prometheus:9090");
        let metric = create_test_metric(
            "error_rate",
            "rate(http_requests_total{status=~'5..'}[5m])",
            "result < 0.05",
        );

        // In a real test environment, you would:
        // 1. Start a mock Prometheus server (e.g., using `httpmock`).
        // 2. Configure it with test data for the expected query.
        // 3. Run the analysis against the mock server.
        // let result = client.analyze(&metric).await;
        // assert!(matches!(result.unwrap(), AnalysisResult::Success));
    }

    #[test]
    fn test_prometheus_response_parsing() {
        // Test that our response structures can deserialize Prometheus JSON
        let json_response = r#"
        {
            "status": "success",
            "data": {
                "resultType": "vector",
                "result": [
                    {
                        "metric": {
                            "__name__": "up",
                            "instance": "localhost:9090"
                        },
                        "value": [1609459200, "1"]
                    }
                ]
            }
        }
        "#;

        let parsed: PrometheusResponse = serde_json::from_str(json_response).unwrap();
        assert_eq!(parsed.status, "success");
        assert!(parsed.data.is_some());

        let data = parsed.data.unwrap();
        assert_eq!(data.result_type, "vector");
        assert_eq!(data.result.len(), 1);

        let result = &data.result[0];
        assert!(result.value.is_some());
        assert_eq!(result.value.as_ref().unwrap().1, "1");
    }
}