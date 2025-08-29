/*
* Copyright (C) 2025 Pedro Henrique / phkaiser13
*
* File: src/core/tracing_layer/src/lib.rs
*
* This module provides the FFI interface for initializing the OpenTelemetry
* pipeline from the main C application.
*
* SPDX-License-Identifier: Apache-2.0
*/

use opentelemetry::{
    global,
    propagation::{Injector, TextMapPropagator},
    sdk::{propagation::TraceContextPropagator, trace as sdktrace},
    trace::{TraceContextExt, TraceError, Tracer},
};
use opentelemetry_jaeger::Uninstall;
use serde::Serialize;
use std::{
    collections::HashMap,
    ffi::{CStr, CString},
    os::raw::c_char,
};
use tracing::span;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

/// Initializes the OpenTelemetry tracer for the CLI and sets it as the global default.
fn init_cli_telemetry() -> Result<Uninstall, TraceError> {
    let tracer = opentelemetry_jaeger::new_agent_pipeline()
        .with_service_name("ph-cli")
        .install_batch(opentelemetry::runtime::Tokio)?;

    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(telemetry)
        .try_init()
        .expect("Failed to register tracer with registry");

    Ok(global::shutdown_tracer_provider)
}

/// FFI-safe entry point to initialize telemetry.
#[no_mangle]
pub extern "C" fn setup_telemetry() {
    if init_cli_telemetry().is_err() {
        eprintln!("[ph-cli] Failed to initialize OpenTelemetry.");
    }
}

/// Starts a new root span for a CLI command and returns its context as a JSON string.
/// The caller is responsible for freeing the returned string using `free_rust_string`.
#[no_mangle]
pub extern "C" fn start_trace_for_command(command_name_ptr: *const c_char) -> *mut c_char {
    let command_name = unsafe { CStr::from_ptr(command_name_ptr).to_string_lossy() };

    let tracer = global::tracer("ph-cli");
    let span = tracer.start(command_name.into_owned());
    let cx = opentelemetry::Context::current_with_span(span);

    let propagator = TraceContextPropagator::new();
    let mut injector = HashMap::new();
    propagator.inject_context(&cx, &mut injector);

    let json_context = serde_json::to_string(&injector).unwrap_or_else(|_| "{}".to_string());
    CString::new(json_context).unwrap().into_raw()
}

/// Frees a string that was allocated by Rust.
#[no_mangle]
pub extern "C" fn free_rust_string(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    unsafe {
        let _ = CString::from_raw(s);
    }
}
