// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Ceph storage driver.
//!
//! [`RealCephDriver`] shells out to the `ceph`/`rbd` CLIs (arg-arrays only — never string
//! concatenation, per PDF §17.3) and normalizes the JSON output into Atlas DTOs.
//! [`FakeCephDriver`] returns canned fixtures for local dev / tests where no Ceph cluster exists.

mod cmd;
mod fake;
mod real;

pub use cmd::{
    ceph_cmd, ceph_osd_op, radosgw_admin_json, radosgw_bucket_quota, rbd_clone, rbd_cmd, rbd_create,
    rbd_du_image, rbd_export_diff, rbd_export_diff_child, rbd_flatten, rbd_import_diff,
    rbd_import_diff_child, rbd_info_size, rbd_list, rbd_mirror_op, rbd_qos_set, rbd_remove, rbd_resize,
    rbd_snap_create,
    rbd_snap_list, rbd_snap_protect, rbd_snap_rm, rbd_snap_rollback, rbd_snap_unprotect,
};
pub use fake::FakeCephDriver;
pub use real::RealCephDriver;
