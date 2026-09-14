// Copyright (c) 2026 ZyvorAI Labs Private Limited.
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial
//! Shared primitives for the Atlas storage control plane: configuration, the application error
//! type, tracing setup, and resource-id helpers.

pub mod config;
pub mod error;
pub mod ids;

pub use config::Config;
pub use error::{AppError, AppResult};

/// Initialize `tracing` once, honoring `RUST_LOG`. Safe to call multiple times.
pub fn init_tracing() {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer())
        .try_init();
}
