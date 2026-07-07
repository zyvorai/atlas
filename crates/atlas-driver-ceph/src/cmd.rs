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

/// Set + enable an RGW per-bucket quota via `radosgw-admin` (Rook doesn't always apply the OBC
/// additionalConfig quota, so Atlas enforces it directly with the admin keyring in the pod).
pub async fn radosgw_bucket_quota(
    bucket: &str,
    max_objects: Option<i64>,
    max_size: Option<&str>,
) -> Result<(), DriverError> {
    let mo = max_objects.map(|n| n.to_string());
    let mut set_args: Vec<&str> = vec!["quota", "set", "--bucket", bucket, "--quota-scope=bucket"];
    if let Some(ref n) = mo {
        set_args.push("--max-objects");
        set_args.push(n);
    }
    if let Some(sz) = max_size {
        set_args.push("--max-size");
        set_args.push(sz);
    }
    radosgw_admin(&set_args).await?;
    radosgw_admin(&[
        "quota",
        "enable",
        "--bucket",
        bucket,
        "--quota-scope=bucket",
    ])
    .await?;
    Ok(())
}

async fn radosgw_admin(args: &[&str]) -> Result<(), DriverError> {
    let output = tokio::process::Command::new("radosgw-admin")
        .args(args)
        .output()
        .await
        .map_err(|e| DriverError::Unreachable(format!("failed to spawn `radosgw-admin`: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(DriverError::Backend(format!(
            "radosgw-admin {}: {stderr}",
            args.join(" ")
        )));
    }
    Ok(())
}

/// Remove an RBD snapshot `pool/image@snap` (best-effort; ignores "not found").
pub async fn rbd_snap_rm(pool: &str, image: &str, snap: &str) -> Result<(), DriverError> {
    let spec = format!("{pool}/{image}@{snap}");
    let output = tokio::process::Command::new("rbd")
        .args(["snap", "rm", &spec])
        .output()
        .await
        .map_err(|e| DriverError::Unreachable(format!("failed to spawn `rbd`: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("No such file") || stderr.contains("does not exist") {
            return Ok(());
        }
        return Err(DriverError::Backend(format!(
            "rbd snap rm {spec}: {}",
            stderr.trim()
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

/// Spawn `rbd export-diff pool/image@snap -` with stdout piped, for streaming the diff elsewhere
/// (e.g. straight into an S3 multipart upload) without buffering it in memory. The caller takes
/// `child.stdout`, streams it, then `wait()`s and checks the exit status.
pub fn rbd_export_diff_child(
    pool: &str,
    image: &str,
    snap: &str,
) -> Result<tokio::process::Child, DriverError> {
    let spec = format!("{pool}/{image}@{snap}");
    tokio::process::Command::new("rbd")
        .args(["export-diff", &spec, "-"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| DriverError::Unreachable(format!("failed to spawn `rbd`: {e}")))
}

/// Spawn `rbd import-diff - pool/image` with stdin piped, for streaming a diff into it (e.g. from
/// an S3 download) without buffering. The caller writes to `child.stdin`, drops it, then `wait()`s.
pub fn rbd_import_diff_child(
    pool: &str,
    image: &str,
) -> Result<tokio::process::Child, DriverError> {
    let spec = format!("{pool}/{image}");
    tokio::process::Command::new("rbd")
        .args(["import-diff", "-", &spec])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| DriverError::Unreachable(format!("failed to spawn `rbd`: {e}")))
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
