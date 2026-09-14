-- Copyright (c) 2026 ZyvorAI Labs Private Limited.
-- SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial
-- Durable job queue: track which worker claimed a job and when, so a multi-worker / multi-replica
-- deployment can reclaim stale `running` rows without failing every interrupted job on restart.
-- The in-memory mpsc channel remains a fast wake-up; the DB is the source of truth.
ALTER TABLE storage_jobs ADD COLUMN locked_by TEXT;
ALTER TABLE storage_jobs ADD COLUMN locked_at TEXT;
