// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! Ceph storage driver.
//!
//! [`RealCephDriver`] shells out to the `ceph`/`rbd` CLIs (arg-arrays only — never string
//! concatenation, per PDF §17.3) and normalizes the JSON output into Atlas DTOs.
//! [`FakeCephDriver`] returns canned fixtures for local dev / tests where no Ceph cluster exists.

mod cmd;
mod fake;
mod real;

pub use cmd::{ceph_cmd, rbd_cmd};
pub use fake::FakeCephDriver;
pub use real::RealCephDriver;
