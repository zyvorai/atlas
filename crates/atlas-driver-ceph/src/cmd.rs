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

/// Day-2 cross-cluster DR: per-image RBD mirroring op — `rbd mirror image enable <pool>/<image>
/// <mode>` / `disable` / `promote` (failover to this cluster) / `demote`. **UNVERIFIED** against a
/// live second Ceph cluster (needs a mirroring peer; see docs/CLAUDE.md — DR was deferred for this).
pub async fn rbd_mirror_op(
    op: &str,
    pool: &str,
    image: &str,
    mode: &str,
) -> Result<(), DriverError> {
    let spec = format!("{pool}/{image}");
    let args: Vec<String> = match op {
        "enable" => vec!["mirror".into(), "image".into(), "enable".into(), spec.clone(), mode.into()],
        "disable" => vec!["mirror".into(), "image".into(), "disable".into(), spec.clone()],
        "promote" => vec!["mirror".into(), "image".into(), "promote".into(), spec.clone()],
        "demote" => vec!["mirror".into(), "image".into(), "demote".into(), spec.clone()],
        other => return Err(DriverError::Backend(format!("unknown mirror op: {other}"))),
    };
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = tokio::process::Command::new("rbd")
        .args(&refs)
        .output()
        .await
        .map_err(|e| DriverError::Unreachable(format!("failed to spawn `rbd`: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(DriverError::Backend(format!("rbd {}: {stderr}", refs.join(" "))));
    }
    Ok(())
}

/// Day-2 per-image QoS: `rbd config image set <pool>/<image> rbd_qos_{iops,bps}_limit <n>` to cap a
/// noisy volume's IOPS / bandwidth. A limit of `0` removes that cap. Only the provided limits are set.
pub async fn rbd_qos_set(
    pool: &str,
    image: &str,
    iops_limit: Option<i64>,
    bps_limit: Option<i64>,
) -> Result<(), DriverError> {
    let spec = format!("{pool}/{image}");
    for (key, val) in [
        ("rbd_qos_iops_limit", iops_limit),
        ("rbd_qos_bps_limit", bps_limit),
    ] {
        let Some(v) = val else { continue };
        let vs = v.to_string();
        let output = tokio::process::Command::new("rbd")
            .args(["config", "image", "set", &spec, key, &vs])
            .output()
            .await
            .map_err(|e| DriverError::Unreachable(format!("failed to spawn `rbd`: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(DriverError::Backend(format!(
                "rbd config image set {spec} {key} {vs}: {stderr}"
            )));
        }
    }
    Ok(())
}

/// Day-2 OSD maintenance op: `ceph osd out|in <id>` (drain / return an OSD) or
/// `ceph osd reweight <id> <weight>` (rebalance data off/onto an OSD; weight in [0,1]).
pub async fn ceph_osd_op(op: &str, osd_id: i64, weight: Option<f64>) -> Result<(), DriverError> {
    let id = osd_id.to_string();
    let args: Vec<String> = match op {
        "out" => vec!["osd".into(), "out".into(), id],
        "in" => vec!["osd".into(), "in".into(), id],
        "reweight" => {
            let w = weight
                .ok_or_else(|| DriverError::Backend("reweight requires a weight".into()))?;
            if !(0.0..=1.0).contains(&w) {
                return Err(DriverError::Backend(
                    "reweight weight must be in [0.0, 1.0]".into(),
                ));
            }
            vec!["osd".into(), "reweight".into(), id, format!("{w}")]
        }
        other => return Err(DriverError::Backend(format!("unknown osd op: {other}"))),
    };
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = tokio::process::Command::new("ceph")
        .args(&refs)
        .output()
        .await
        .map_err(|e| DriverError::Unreachable(format!("failed to spawn `ceph`: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(DriverError::Backend(format!("ceph {}: {stderr}", refs.join(" "))));
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

/// Run `radosgw-admin <args...> --format json` and parse stdout (e.g. `bucket stats`).
pub async fn radosgw_admin_json(args: &[&str]) -> Result<serde_json::Value, DriverError> {
    let output = tokio::process::Command::new("radosgw-admin")
        .args(args)
        .arg("--format")
        .arg("json")
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
    serde_json::from_slice(&output.stdout)
        .map_err(|e| DriverError::Parse(format!("radosgw-admin json: {e}")))
}

/// List an RBD image's snapshot names (`rbd snap ls pool/image --format json`).
pub async fn rbd_snap_list(pool: &str, image: &str) -> Result<Vec<String>, DriverError> {
    let spec = format!("{pool}/{image}");
    let v = rbd_cmd(&["snap", "ls", &spec]).await?;
    Ok(v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default())
}

/// Roll an RBD image back to a snapshot (`rbd snap rollback pool/image@snap`). Destructive.
pub async fn rbd_snap_rollback(pool: &str, image: &str, snap: &str) -> Result<(), DriverError> {
    let spec = format!("{pool}/{image}@{snap}");
    let output = tokio::process::Command::new("rbd")
        .args(["snap", "rollback", &spec])
        .output()
        .await
        .map_err(|e| DriverError::Unreachable(format!("failed to spawn `rbd`: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(DriverError::Backend(format!(
            "rbd snap rollback {spec}: {stderr}"
        )));
    }
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

/// Create an RBD image directly (`rbd create pool/image --size <MiB>`) for non-CSI consumers.
pub async fn rbd_create(pool: &str, image: &str, size_bytes: i64) -> Result<(), DriverError> {
    let spec = format!("{pool}/{image}");
    let mib = std::cmp::max(1, size_bytes / (1024 * 1024)).to_string();
    let output = tokio::process::Command::new("rbd")
        .args(["create", &spec, "--size", &mib])
        .output()
        .await
        .map_err(|e| DriverError::Unreachable(format!("failed to spawn `rbd`: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(DriverError::Backend(format!("rbd create {spec}: {stderr}")));
    }
    Ok(())
}

/// Remove an RBD image directly (`rbd rm pool/image`; idempotent on "not found").
pub async fn rbd_remove(pool: &str, image: &str) -> Result<(), DriverError> {
    let spec = format!("{pool}/{image}");
    let output = tokio::process::Command::new("rbd")
        .args(["rm", &spec])
        .output()
        .await
        .map_err(|e| DriverError::Unreachable(format!("failed to spawn `rbd`: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("No such file") || stderr.contains("does not exist") {
            return Ok(());
        }
        return Err(DriverError::Backend(format!(
            "rbd rm {spec}: {}",
            stderr.trim()
        )));
    }
    Ok(())
}

/// Grow an RBD image (`rbd resize pool/image --size <MiB>`; grow-only, no shrink).
pub async fn rbd_resize(pool: &str, image: &str, size_bytes: i64) -> Result<(), DriverError> {
    let spec = format!("{pool}/{image}");
    let mib = std::cmp::max(1, size_bytes / (1024 * 1024)).to_string();
    let output = tokio::process::Command::new("rbd")
        .args(["resize", &spec, "--size", &mib])
        .output()
        .await
        .map_err(|e| DriverError::Unreachable(format!("failed to spawn `rbd`: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(DriverError::Backend(format!("rbd resize {spec}: {stderr}")));
    }
    Ok(())
}

/// Flatten a cloned image so it no longer depends on its parent snapshot (`rbd flatten`).
pub async fn rbd_flatten(pool: &str, image: &str) -> Result<(), DriverError> {
    let spec = format!("{pool}/{image}");
    let output = tokio::process::Command::new("rbd")
        .args(["flatten", &spec])
        .output()
        .await
        .map_err(|e| DriverError::Unreachable(format!("failed to spawn `rbd`: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(DriverError::Backend(format!(
            "rbd flatten {spec}: {stderr}"
        )));
    }
    Ok(())
}

/// Total provisioned size of an RBD image in bytes (`rbd info pool/image --format json`).
pub async fn rbd_info_size(pool: &str, image: &str) -> Result<i64, DriverError> {
    let spec = format!("{pool}/{image}");
    let v = rbd_cmd(&["info", &spec]).await?;
    v.get("size")
        .and_then(|s| s.as_i64())
        .ok_or_else(|| DriverError::Parse(format!("rbd info {spec}: no size")))
}

/// Actual used (allocated) bytes of an RBD image (`rbd du pool/image --format json`).
pub async fn rbd_du_image(pool: &str, image: &str) -> Result<i64, DriverError> {
    let spec = format!("{pool}/{image}");
    let v = rbd_cmd(&["du", &spec]).await?;
    v.get("images")
        .and_then(|a| a.as_array())
        .and_then(|a| a.first())
        .and_then(|img| img.get("used_size"))
        .and_then(|u| u.as_i64())
        .ok_or_else(|| DriverError::Parse(format!("rbd du {spec}: no used_size")))
}

/// Protect a snapshot so it can be used as a clone parent (`rbd snap protect`).
pub async fn rbd_snap_protect(pool: &str, image: &str, snap: &str) -> Result<(), DriverError> {
    snap_op(&["snap", "protect"], pool, image, snap).await
}

/// Unprotect a snapshot (`rbd snap unprotect`); best-effort on "not protected".
pub async fn rbd_snap_unprotect(pool: &str, image: &str, snap: &str) -> Result<(), DriverError> {
    let spec = format!("{pool}/{image}@{snap}");
    let output = tokio::process::Command::new("rbd")
        .args(["snap", "unprotect", &spec])
        .output()
        .await
        .map_err(|e| DriverError::Unreachable(format!("failed to spawn `rbd`: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not protected") || stderr.contains("does not exist") {
            return Ok(());
        }
        return Err(DriverError::Backend(format!(
            "rbd snap unprotect {spec}: {}",
            stderr.trim()
        )));
    }
    Ok(())
}

/// Clone a protected snapshot into a new COW image (`rbd clone parent@snap clone`).
pub async fn rbd_clone(
    parent_pool: &str,
    parent_image: &str,
    snap: &str,
    clone_pool: &str,
    clone_image: &str,
) -> Result<(), DriverError> {
    let src = format!("{parent_pool}/{parent_image}@{snap}");
    let dst = format!("{clone_pool}/{clone_image}");
    let output = tokio::process::Command::new("rbd")
        .args(["clone", &src, &dst])
        .output()
        .await
        .map_err(|e| DriverError::Unreachable(format!("failed to spawn `rbd`: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(DriverError::Backend(format!(
            "rbd clone {src} {dst}: {stderr}"
        )));
    }
    Ok(())
}

async fn snap_op(op: &[&str], pool: &str, image: &str, snap: &str) -> Result<(), DriverError> {
    let spec = format!("{pool}/{image}@{snap}");
    let mut args: Vec<&str> = op.to_vec();
    args.push(&spec);
    let output = tokio::process::Command::new("rbd")
        .args(&args)
        .output()
        .await
        .map_err(|e| DriverError::Unreachable(format!("failed to spawn `rbd`: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(DriverError::Backend(format!(
            "rbd {} {spec}: {stderr}",
            op.join(" ")
        )));
    }
    Ok(())
}

/// List RBD image names in a pool (`rbd ls pool --format json`).
pub async fn rbd_list(pool: &str) -> Result<Vec<String>, DriverError> {
    let v = rbd_cmd(&["ls", pool]).await?;
    Ok(v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default())
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
