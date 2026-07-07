// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Safe `ceph`/`rbd` command wrappers.
//!
//! Security rule (PDF §17.3): pass arguments as an **array only**; never build a shell string.
//! Modeled on `machina/agent/src/provision_ops.rs`, but async and JSON-parsing.

use atlas_driver_core::DriverError;

/// Run `ceph <args...> --format json` and parse stdout as JSON.
pub async fn ceph_cmd(args: &[&str]) -> Result<serde_json::Value, DriverError> {
    run_json("ceph", args).await
}

/// Run `rbd <args...> --format json` and parse stdout as JSON.
pub async fn rbd_cmd(args: &[&str]) -> Result<serde_json::Value, DriverError> {
    run_json("rbd", args).await
}

async fn run_json(bin: &str, args: &[&str]) -> Result<serde_json::Value, DriverError> {
    let output = tokio::process::Command::new(bin)
        .args(args)
        .arg("--format")
        .arg("json")
        .output()
        .await
        .map_err(|e| {
            // e.g. binary missing, or no permission — treat as backend unreachable.
            DriverError::Unreachable(format!("failed to spawn `{bin}`: {e}"))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(DriverError::Backend(format!(
            "`{bin} {}` failed: {stderr}",
            args.join(" ")
        )));
    }

    serde_json::from_slice(&output.stdout)
        .map_err(|e| DriverError::Parse(format!("`{bin}` json: {e}")))
}
