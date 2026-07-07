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

/// Create an RBD snapshot `pool/image@snap` (idempotent-ish; errors if it already exists).
pub async fn rbd_snap_create(pool: &str, image: &str, snap: &str) -> Result<(), DriverError> {
    let spec = format!("{pool}/{image}@{snap}");
    let output = tokio::process::Command::new("rbd")
        .args(["snap", "create", &spec])
        .output()
        .await
        .map_err(|e| DriverError::Unreachable(format!("failed to spawn `rbd`: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(DriverError::Backend(format!(
            "rbd snap create {spec}: {stderr}"
        )));
    }
    Ok(())
}

/// Export an RBD snapshot as an incremental diff stream (`rbd export-diff pool/image@snap -`).
/// For a fresh/sparse image this is small; the caller must cap the size it buffers.
pub async fn rbd_export_diff(pool: &str, image: &str, snap: &str) -> Result<Vec<u8>, DriverError> {
    let spec = format!("{pool}/{image}@{snap}");
    let output = tokio::process::Command::new("rbd")
        .args(["export-diff", &spec, "-"])
        .output()
        .await
        .map_err(|e| DriverError::Unreachable(format!("failed to spawn `rbd`: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(DriverError::Backend(format!(
            "rbd export-diff {spec}: {stderr}"
        )));
    }
    Ok(output.stdout)
}

/// Apply an RBD diff stream (from `rbd export-diff`) into an existing image via stdin
/// (`rbd import-diff - pool/image`). Used by restore-from-data.
pub async fn rbd_import_diff(pool: &str, image: &str, data: Vec<u8>) -> Result<(), DriverError> {
    use tokio::io::AsyncWriteExt;
    let spec = format!("{pool}/{image}");
    let mut child = tokio::process::Command::new("rbd")
        .args(["import-diff", "-", &spec])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| DriverError::Unreachable(format!("failed to spawn `rbd`: {e}")))?;
    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| DriverError::Backend("rbd import-diff: no stdin".into()))?;
        stdin
            .write_all(&data)
            .await
            .map_err(|e| DriverError::Backend(format!("rbd import-diff write: {e}")))?;
        let _ = stdin.shutdown().await;
    }
    let output = child
        .wait_with_output()
        .await
        .map_err(|e| DriverError::Backend(format!("rbd import-diff wait: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(DriverError::Backend(format!(
            "rbd import-diff {spec}: {stderr}"
        )));
    }
    Ok(())
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
