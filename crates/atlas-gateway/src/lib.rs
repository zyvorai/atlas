// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Atlas gateway library: state, auth, routes, and startup wiring shared by the binary and tests.

pub mod auth;
pub mod routes;
pub mod startup;
pub mod state;

pub use state::AppState;
