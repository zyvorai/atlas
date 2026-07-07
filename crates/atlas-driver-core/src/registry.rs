// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
//! A small registry mapping backend ids to live driver instances.

use std::collections::HashMap;
use std::sync::Arc;

use crate::StorageDriver;

/// Holds the set of active backend drivers, keyed by backend id.
#[derive(Clone, Default)]
pub struct DriverRegistry {
    drivers: HashMap<String, Arc<dyn StorageDriver>>,
}

impl DriverRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (or replace) a driver for its backend id.
    pub fn register(&mut self, driver: Arc<dyn StorageDriver>) {
        self.drivers.insert(driver.backend_id().to_string(), driver);
    }

    pub fn get(&self, backend_id: &str) -> Option<Arc<dyn StorageDriver>> {
        self.drivers.get(backend_id).cloned()
    }

    /// The first registered driver, if any — convenient for the single-backend MVP.
    pub fn any(&self) -> Option<Arc<dyn StorageDriver>> {
        self.drivers.values().next().cloned()
    }

    pub fn backend_ids(&self) -> Vec<String> {
        self.drivers.keys().cloned().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.drivers.is_empty()
    }
}
